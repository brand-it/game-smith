/// Background worker that spawns game server processes and streams their output to log
/// files.
///
/// On Linux, uses PTY-based streaming via `libc::openpty` for proper terminal behavior.
/// On Windows, uses `ConPTY` via `portable-pty` with VT cursor-position response to avoid hangs.
use async_trait::async_trait;
#[cfg(target_os = "linux")]
use libc::openpty;
use loco_rs::app::AppContext;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::Result;
#[cfg(target_os = "windows")]
use portable_pty;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
#[cfg(target_os = "windows")]
use std::io::Read;
#[cfg(target_os = "windows")]
use std::io::Write;
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;
use tokio::io::AsyncRead;
use tokio::process::Command;

use crate::data::steamcmd::{set_shared_store, SteamCmdHealthStatus};
use crate::models::command_runs::{CommandStatus, Model as CommandRunModel};
use crate::models::game_servers::ServerStatus;

pub struct CommandExecWorker {
    pub ctx: AppContext,
}

/// Delay before auto-restarting a game server process (seconds).
const AUTO_RESTART_DELAY_SECS: u64 = 5;

impl CommandExecWorker {
    /// Load the run record and verify it is still in "running" state.
    async fn fetch_and_validate(&self, run_id: i32) -> Result<CommandRunModel> {
        let model = CommandRunModel::find_by_id(&self.ctx, run_id)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to find run: {e}")))?
            .ok_or_else(|| loco_rs::Error::string("run not found"))?;

        if !model.is_running() {
            tracing::info!(run_id, "run already stopped, skipping execution");
            return Err(loco_rs::Error::string("run already stopped"));
        }

        Ok(model)
    }

    /// Mark a run as failed in the database (best-effort, fire-and-forget).
    async fn mark_spawn_failed(ctx: &AppContext, run_id: i32) {
        if let Ok(Some(m)) = CommandRunModel::find_by_id(ctx, run_id).await {
            let mut active: crate::models::command_runs::ActiveModel = m.into();
            let _ = active.finish(ctx, Some(-1), CommandStatus::Failed).await;
        }
    }

    /// Handle spawn failure: update DB and shared health status.
    async fn _log_spawn_failed(
        ctx: &AppContext,
        run_id: i32,
        is_health_check: bool,
        error_msg: &str,
        kind: std::io::ErrorKind,
    ) {
        Self::mark_spawn_failed(ctx, run_id).await;
        if is_health_check {
            let status = if kind == std::io::ErrorKind::NotFound {
                SteamCmdHealthStatus::NotInstalled
            } else {
                SteamCmdHealthStatus::Broken(error_msg.to_string())
            };
            set_shared_store(&ctx.shared_store);
            ctx.shared_store.insert(status);
        }
    }

    /// Store the child process PID in the database (best-effort).
    async fn store_pid(&self, run_id: i32, pid: u32) {
        if let Ok(Some(m)) = CommandRunModel::find_by_id(&self.ctx, run_id).await {
            let mut active: crate::models::command_runs::ActiveModel = m.into();
            let _ = active.update_pid(&self.ctx, i64::from(pid)).await;
        }
    }

    /// Determine the final status and exit code from a process exit status.
    fn determine_status(status: std::process::ExitStatus) -> (CommandStatus, Option<i32>) {
        if status.success() {
            (CommandStatus::Completed, Some(0))
        } else if let Some(code) = status.code() {
            (CommandStatus::Failed, Some(code))
        } else {
            (CommandStatus::Failed, None)
        }
    }

    /// Write the final status back to the database.
    async fn update_final_status(
        &self,
        run_id: i32,
        exit_code: Option<i32>,
        final_status: CommandStatus,
    ) -> Result<()> {
        let model = CommandRunModel::find_by_id(&self.ctx, run_id)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to find run: {e}")))?
            .ok_or_else(|| loco_rs::Error::string("run not found"))?;
        let mut active: crate::models::command_runs::ActiveModel = model.into();
        active
            .finish(&self.ctx, exit_code, final_status)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to update run status: {e}")))?;

        Ok(())
    }

    /// After a health-check run completes, update the shared store so the
    /// banner across all pages reflects the latest status.
    ///
    /// Health-check runs are identified by executing the `SteamCMD` binary
    /// with a single `+quit` argument.
    async fn maybe_update_health_status(
        &self,
        run_id: i32,
        final_status: CommandStatus,
        exit_code: Option<i32>,
    ) {
        let Ok(Some(model)) = CommandRunModel::find_by_id(&self.ctx, run_id).await else {
            return;
        };

        // Health-check runs: steamcmd binary with just +quit
        if !Self::is_health_check(&model) {
            return;
        }

        let status = match (final_status, exit_code) {
            (CommandStatus::Completed, Some(0)) => SteamCmdHealthStatus::Healthy,
            (CommandStatus::Failed, _) => model
                .log_path
                .as_ref()
                .and_then(|log_path| std::fs::read_to_string(log_path).ok())
                .and_then(|content| content.lines().last().map(String::from))
                .map_or_else(
                    || SteamCmdHealthStatus::Broken("health check failed".to_string()),
                    SteamCmdHealthStatus::Broken,
                ),
            _ => SteamCmdHealthStatus::Broken("health check failed".to_string()),
        };

        set_shared_store(&self.ctx.shared_store);
        self.ctx.shared_store.insert(status);
    }

    /// Returns true if this run is a `SteamCMD` health check.
    ///
    /// Identified by the command being a `steamcmd` binary and the
    /// argument list being exactly `["+quit"]`.
    fn is_health_check(model: &CommandRunModel) -> bool {
        let is_steamcmd =
            model.command.ends_with("steamcmd.sh") || model.command.ends_with("steamcmd.exe");
        let args_quit = model
            .args
            .as_array()
            .is_some_and(|arr| arr.len() == 1 && arr[0].as_str() == Some("+quit"));
        is_steamcmd && args_quit
    }

    /// Returns true if this run is a `SteamCMD` install or update command.
    ///
    /// Identified by the command being a `steamcmd` binary and the argument
    /// list containing `+runscript`.
    fn is_install_update(model: &CommandRunModel) -> bool {
        let is_steamcmd =
            model.command.ends_with("steamcmd.sh") || model.command.ends_with("steamcmd.exe");
        let has_runscript = model
            .args
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("+runscript")));
        is_steamcmd && has_runscript
    }

    /// Returns true if this server should auto-restart after process exit.
    ///
    /// Checks that the run is associated with a game server (not an install/
    /// update command) and that the server's `auto_restart` flag is set.
    async fn should_auto_restart(&self, run_id: i32) -> bool {
        let Ok(Some(model)) = CommandRunModel::find_by_id(&self.ctx, run_id).await else {
            return false;
        };

        if Self::is_install_update(&model) {
            return false;
        }

        let Some(server_id) = model.server_id else {
            return false;
        };

        let Ok(server_id_i32) = i32::try_from(server_id) else {
            return false;
        };

        let Ok(Some(server)) =
            crate::models::game_servers::Model::find_by_id(&self.ctx, server_id_i32).await
        else {
            return false;
        };

        server.auto_restart
    }

    /// Append an auto-restart marker line to the run's log file.
    async fn write_restart_marker(&self, run_id: i32) {
        let Ok(Some(model)) = CommandRunModel::find_by_id(&self.ctx, run_id).await else {
            return;
        };
        let Some(ref log_path) = model.log_path else {
            return;
        };

        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let marker = format!("\n[{timestamp}] Auto-restarting server...\n");
        if let Err(e) = std::fs::OpenOptions::new()
            .append(true)
            .open(log_path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, marker.as_bytes()))
        {
            tracing::warn!(run_id, error = %e, "failed to write restart marker to log");
        }
    }

    /// Extract the last non-empty line from a log file as an error message.
    fn last_log_line(model: &CommandRunModel, fallback: &str) -> Option<String> {
        model
            .log_path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| {
                content
                    .lines()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .map(String::from)
            })
            .or_else(|| Some(fallback.to_string()))
    }

    /// Update the game server's status after a command run completes.
    /// Handles both install/update commands and regular server start/stop.
    async fn maybe_update_server_status(&self, run_id: i32, final_status: CommandStatus) {
        let Ok(Some(model)) = CommandRunModel::find_by_id(&self.ctx, run_id).await else {
            return;
        };

        let Some(server_id) = model.server_id else {
            return;
        };

        let Ok(server_id_i32) = i32::try_from(server_id) else {
            return;
        };

        let Ok(Some(server)) =
            crate::models::game_servers::Model::find_by_id(&self.ctx, server_id_i32).await
        else {
            return;
        };

        let (new_status, last_error) = if Self::is_install_update(&model) {
            // Install/update command
            match final_status {
                CommandStatus::Completed => (ServerStatus::Installed, None),
                CommandStatus::Failed => {
                    let last_error = Self::last_log_line(&model, "Installation failed");
                    (ServerStatus::Error, last_error)
                }
                CommandStatus::Running => return,
            }
        } else {
            // Start command — update status when the process exits
            match final_status {
                CommandStatus::Completed => (ServerStatus::Stopped, None),
                CommandStatus::Failed => {
                    let last_error =
                        Self::last_log_line(&model, "Server process exited with failure");
                    (ServerStatus::Error, last_error)
                }
                CommandStatus::Running => return,
            }
        };

        let mut active: crate::models::game_servers::ActiveModel = server.into();
        if let Err(e) = active
            .update_status(&self.ctx, new_status, last_error)
            .await
        {
            tracing::warn!(
                server_id,
                %new_status,
                error = %e,
                "Failed to update game server status"
            );
        }
    }

    /// Spawn a single process and wait for it to exit.
    ///
    /// Returns the [`CommandStatus`] and exit code. Does _not_ persist the
    /// final status — the caller is responsible for that so it can decide
    /// whether to auto-restart first.
    #[cfg(target_os = "linux")]
    async fn spawn_one(
        &self,
        run_id: i32,
        model: &CommandRunModel,
    ) -> Result<(CommandStatus, Option<i32>)> {
        let cmd_args = Self::resolve_args(model);

        // Create PTY for line-buffered output
        let mut master_fd: libc::c_int = 0;
        let mut slave_fd: libc::c_int = 0;
        if unsafe {
            openpty(
                &raw mut master_fd,
                &raw mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0
        {
            return Err(loco_rs::Error::string(&format!(
                "failed to create pty: {}",
                std::io::Error::last_os_error()
            )));
        }

        let slave = unsafe { std::fs::File::from_raw_fd(slave_fd) };
        let master = unsafe { std::fs::File::from_raw_fd(master_fd) };

        let mut cmd = Command::new(&model.command);
        cmd.args(&cmd_args);
        cmd.stdout(std::process::Stdio::from(slave.try_clone().map_err(
            |e| loco_rs::Error::string(&format!("failed to clone slave for stdout: {e}")),
        )?));
        cmd.stderr(std::process::Stdio::from(slave.try_clone().map_err(
            |e| loco_rs::Error::string(&format!("failed to clone slave for stderr: {e}")),
        )?));
        // Original slave dropped here — closes slave fd in parent process
        drop(slave);

        cmd.kill_on_drop(true);

        Self::configure_common(&mut cmd, model);

        let mut child = cmd.spawn().map_err(|e| {
            Self::handle_spawn_error(
                &self.ctx,
                run_id,
                Self::is_health_check(model),
                &e,
                e.kind(),
            )
        })?;

        if let Some(pid) = child.id() {
            self.store_pid(run_id, pid).await;
        }

        // Stream PTY master → log file
        if let Some(ref lp) = model.log_path {
            let master = tokio::fs::File::from_std(master);
            Self::spawn_reader(lp, master);
        }

        let status = child
            .wait()
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to wait for process: {e}")))?;

        Ok(Self::determine_status(status))
    }

    /// Spawn a single process and wait for it to exit (Windows, ConPTY).
    ///
    /// Uses `portable-pty` to open a ConPTY pair. This is required because:
    ///
    /// - With plain pipes, the Windows CRT detects a non-TTY and switches to
    ///   64 KB full-buffering, so SteamCMD output is invisible until the process
    ///   exits (or the buffer fills).
    /// - ConPTY allocates an invisible pseudo-console that satisfies the CRT's
    ///   TTY check, so SteamCMD line-buffers and output flows promptly.
    /// - ConPTY suppresses the visible console window automatically.
    ///
    /// The one wrinkle: SteamCMD probes the terminal size by sending `ESC[6n`
    /// (ANSI Device Status Report / cursor-position query) and blocks until it
    /// receives a `ESC[row;colR` response. The drain thread watches for this
    /// sequence and immediately writes the reply back through the master writer,
    /// so the child never stalls.
    ///
    /// Shutdown on demand works via `TerminateProcess` against the stored PID
    /// (see `kill_pid`). The PTY pair is dropped after the child exits, which
    /// closes the pseudo-console and signals EOF to the drain thread.
    #[cfg(target_os = "windows")]
    async fn spawn_one(
        &self,
        run_id: i32,
        model: &CommandRunModel,
    ) -> Result<(CommandStatus, Option<i32>)> {
        let cmd_args = Self::resolve_args(model);

        // Open a ConPTY pair. The child sees a real terminal (no console window
        // appears — ConPTY creates a headless pseudo-console).
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: 25,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| loco_rs::Error::string(&format!("failed to create pty: {e}")))?;

        let mut cmd = portable_pty::CommandBuilder::new(&model.command);
        cmd.args(&cmd_args);
        if let Some(dir) = &model.working_dir {
            cmd.cwd(dir);
        }
        if let Some(ref env_json) = model.env {
            if let Ok(variables) =
                serde_json::from_value::<HashMap<String, String>>(env_json.clone())
            {
                for (key, value) in variables {
                    cmd.env(&key, &value);
                }
            }
        }

        let mut child = pair.slave.spawn_command(cmd).map_err(|e| {
            Self::handle_spawn_error(
                &self.ctx,
                run_id,
                Self::is_health_check(model),
                &e,
                io::ErrorKind::Other,
            )
        })?;

        if let Some(pid) = child.process_id() {
            self.store_pid(run_id, pid).await;
        }

        // Obtain a reader and a writer for the PTY master.
        // Reader: receives the child's output.
        // Writer: sends input to the child (used to reply to VT queries).
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| loco_rs::Error::string(&format!("failed to clone pty reader: {e}")))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|e| loco_rs::Error::string(&format!("failed to take pty writer: {e}")))?;

        let log_path = model.log_path.clone();

        // Drain thread: read PTY output, reply to ESC[6n, write to log file.
        //
        // SteamCMD sends ESC[6n (ANSI DSR — cursor position request) to probe
        // the terminal and blocks until it receives ESC[row;colR back. We reply
        // immediately with ESC[1;1R so the child never stalls.
        let drain_handle = std::thread::spawn(move || {
            // ESC[6n  — DSR cursor-position query sent by the child.
            const DSR_QUERY: &[u8] = b"\x1b[6n";
            // ESC[1;1R — CPR response: row 1, column 1.
            const CPR_REPLY: &[u8] = b"\x1b[1;1R";

            let mut log_writer: Option<std::io::BufWriter<std::fs::File>> =
                log_path.as_deref().and_then(|p| {
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(p)
                        .ok()
                        .map(std::io::BufWriter::new)
                });

            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = &buf[..n];
                        // Reply to cursor-position queries before writing to log
                        // so the escape sequence itself doesn't pollute output.
                        if chunk.windows(DSR_QUERY.len()).any(|w| w == DSR_QUERY) {
                            let _ = writer.write_all(CPR_REPLY);
                        }
                        if let Some(ref mut w) = log_writer {
                            let _ = w.write_all(chunk);
                            let _ = w.flush();
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Poll for child exit non-blocking so the drain thread can run
        // concurrently and keep the PTY buffer empty.
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(e) => {
                    return Err(loco_rs::Error::string(&format!(
                        "failed to poll process status: {e}"
                    )));
                }
            }
        };

        // Drop the PTY pair to close the ConPTY — this signals EOF to the
        // drain thread's reader so it exits cleanly.
        drop(pair);
        let _ = drain_handle.join();

        Ok(Self::determine_pty_status(exit_status))
    }

    /// Map a [`portable_pty::ExitStatus`] to our internal status tuple.
    #[cfg(target_os = "windows")]
    fn determine_pty_status(status: portable_pty::ExitStatus) -> (CommandStatus, Option<i32>) {
        if status.success() {
            (CommandStatus::Completed, Some(0))
        } else {
            let code = status.exit_code() as i32;
            (CommandStatus::Failed, Some(code))
        }
    }

    /// Parse args from the `SeaORM` JSON array.
    fn resolve_args(model: &CommandRunModel) -> Vec<String> {
        model
            .args
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Apply platform-agnostic command configuration: working directory and
    /// environment variables from the [`CommandRunModel`].
    fn configure_common(cmd: &mut Command, model: &CommandRunModel) {
        if let Some(dir) = &model.working_dir {
            cmd.current_dir(dir);
        }
        if let Some(ref env_json) = model.env {
            if let Ok(variables) =
                serde_json::from_value::<HashMap<String, String>>(env_json.clone())
            {
                for (key, value) in variables {
                    cmd.env(key, value);
                }
            }
        }
    }

    /// Handle spawn failure: log the error asynchronously and return a
    /// [`loco_rs::Error`].
    fn handle_spawn_error<E>(
        ctx: &AppContext,
        run_id: i32,
        is_health_check: bool,
        e: &E,
        kind: io::ErrorKind,
    ) -> loco_rs::Error
    where
        E: std::fmt::Display,
    {
        let ctx = ctx.clone();
        let rid = run_id;
        let msg = e.to_string();
        tokio::spawn(async move {
            Self::_log_spawn_failed(&ctx, rid, is_health_check, &msg, kind).await;
        });
        loco_rs::Error::string(&format!("failed to spawn process: {e}"))
    }

    /// Spawn a background task that copies output from an [`AsyncRead`] stream
    /// into the run's log file. Used for PTY master on Linux.
    fn spawn_reader<R>(log_path: &str, reader: R)
    where
        R: AsyncRead + Send + 'static,
    {
        let lp = log_path.to_string();
        if let Ok(log_file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let log_file = tokio::fs::File::from_std(log_file);
            tracing::debug!(log_path = %lp, "log reader started");
            tokio::spawn(async move {
                let mut reader = Box::pin(reader);
                let mut log_file = log_file;
                if let Err(e) = tokio::io::copy(&mut reader, &mut log_file).await {
                    tracing::warn!(%lp, error = %e, "log reader failed");
                } else {
                    tracing::debug!(log_path = %lp, "log reader completed");
                }
            });
        }
    }

    /// Main execution loop: spawn the process, wait for it, auto-restart if
    /// configured.
    ///
    /// Keeps the same command run record (same `run_id`) across restarts,
    /// appending restart markers to the log and updating the stored PID each
    /// time a new process is spawned.
    async fn perform_inner(
        &self,
        run_id: i32,
        model: &CommandRunModel,
    ) -> Result<(CommandStatus, Option<i32>)> {
        let result = loop {
            let (status, exit_code) = self.spawn_one(run_id, model).await?;

            if !self.should_auto_restart(run_id).await {
                break Ok((status, exit_code));
            }

            // Run was manually stopped — don't restart
            let updated = CommandRunModel::find_by_id(&self.ctx, run_id).await;
            if let Ok(Some(m)) = updated {
                if !m.is_running() {
                    break Ok((status, exit_code));
                }
            }

            tracing::info!(run_id, "auto-restarting game server process");
            self.write_restart_marker(run_id).await;
            tokio::time::sleep(std::time::Duration::from_secs(AUTO_RESTART_DELAY_SECS)).await;
        };
        result
    }
}

/// Arguments for executing a command via [`CommandExecWorker`].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandExecWorkerArgs {
    pub run_id: i32,
}

#[async_trait]
impl BackgroundWorker<CommandExecWorkerArgs> for CommandExecWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    /// Fetches the command run, executes (potentially auto-restarting), then
    /// updates final status. Platform-specific spawning is handled by
    /// [`spawn_one`].
    async fn perform(&self, args: CommandExecWorkerArgs) -> Result<()> {
        let CommandExecWorkerArgs { run_id } = args;

        let Ok(model) = self.fetch_and_validate(run_id).await else {
            return Ok(());
        };

        let (final_status, exit_code) = self.perform_inner(run_id, &model).await?;

        self.update_final_status(run_id, exit_code, final_status)
            .await?;
        self.maybe_update_health_status(run_id, final_status, exit_code)
            .await;
        self.maybe_update_server_status(run_id, final_status).await;

        Ok(())
    }
}
