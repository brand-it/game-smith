use async_trait::async_trait;
#[cfg(target_os = "linux")]
use libc::openpty;
use loco_rs::app::AppContext;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;
use tokio::process::Command;

use crate::data::steamcmd::{set_shared_store, SteamCmdHealthStatus};
use crate::models::command_runs::{CommandStatus, Model as CommandRunModel};
use crate::models::game_servers::ServerStatus;

pub struct CommandExecWorker {
    pub ctx: AppContext,
}

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

    /// After an install/update command completes, update the associated game
    /// server's status so it doesn't stay stuck at "installing".
    async fn maybe_update_server_status(&self, run_id: i32, final_status: CommandStatus) {
        let Ok(Some(model)) = CommandRunModel::find_by_id(&self.ctx, run_id).await else {
            return;
        };

        // Only act on install/update commands
        if !Self::is_install_update(&model) {
            return;
        }

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

        let (new_status, last_error) = match final_status {
            CommandStatus::Completed => (ServerStatus::Installed, None),
            CommandStatus::Failed => {
                // Try to extract the last meaningful error line from the log
                let last_error = model
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
                    .or_else(|| Some("Installation failed".to_string()));
                (ServerStatus::Error, last_error)
            }
            CommandStatus::Running => return,
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
                "Failed to update game server status after install"
            );
        }
    }
    /// Shared inner execution logic after platform-specific stdio setup.
    ///
    /// The `setup_stdio` closure configures stdout/stderr on the [`Command`]
    /// and returns an optional reader handle (used by the PTY master on Linux,
    /// or `None` on Windows where the child's stdout pipe is used directly).
    #[allow(unused_variables)]
    async fn perform_inner<F>(
        &self,
        run_id: i32,
        model: CommandRunModel,
        setup_stdio: F,
    ) -> Result<()>
    where
        F: FnOnce(&mut Command) -> std::result::Result<Option<std::fs::File>, loco_rs::Error>
            + Send,
    {
        // 3. Build the command
        let cmd_args: Vec<String> = model
            .args
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let mut cmd = Command::new(&model.command);
        cmd.args(&cmd_args);

        // Configure stdout/stderr (platform-specific)
        let master_reader = setup_stdio(&mut cmd)?;

        cmd.kill_on_drop(true);

        if let Some(dir) = &model.working_dir {
            cmd.current_dir(dir);
        }

        if let Some(env_json) = &model.env {
            if let Ok(variables) =
                serde_json::from_value::<HashMap<String, String>>(env_json.clone())
            {
                for (key, value) in variables {
                    cmd.env(key, value);
                }
            }
        }

        // 4. Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            let ctx = self.ctx.clone();
            let rid = run_id;
            let is_health_check = Self::is_health_check(&model);
            let error_msg = e.to_string();
            let kind = e.kind();
            let error_for_log = error_msg.clone();
            tokio::spawn(async move {
                Self::_log_spawn_failed(&ctx, rid, is_health_check, &error_for_log, kind).await;
            });
            loco_rs::Error::string(&format!("failed to spawn process: {error_msg}"))
        })?;

        // 5. Store PID in database
        if let Some(pid) = child.id() {
            self.store_pid(run_id, pid).await;
        }

        // 6. Read output into log file
        #[cfg(target_os = "linux")]
        {
            // PTY master reader
            let log_path = model.log_path.clone();
            if let Some(log_path) = &log_path {
                let log_file = std::fs::File::create(log_path).map_err(|e| {
                    loco_rs::Error::string(&format!("failed to create log file: {e}"))
                })?;
                let log_file = tokio::fs::File::from_std(log_file);
                if let Some(master) = master_reader {
                    let master = tokio::fs::File::from_std(master);
                    let log_path_str = log_path.clone();
                    tokio::spawn(async move {
                        let mut master = tokio::io::BufReader::new(master);
                        let mut log_file = log_file;
                        if let Err(e) = tokio::io::copy_buf(&mut master, &mut log_file).await {
                            tracing::warn!(%log_path_str, error = %e, "PTY reader failed");
                        }
                    });
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Pipe child stdout/stderr to log file
            let log_path = model.log_path.clone();
            if let Some(log_path) = &log_path {
                let log_file = std::fs::File::create(log_path).map_err(|e| {
                    loco_rs::Error::string(&format!("failed to create log file: {e}"))
                })?;
                let log_file = tokio::fs::File::from_std(log_file);
                let log_path_str = log_path.clone();
                if let Some(stdout) = child.stdout.take() {
                    let mut stdout = tokio::io::BufReader::new(stdout);
                    tokio::spawn(async move {
                        let mut log_file = log_file;
                        if let Err(e) = tokio::io::copy_buf(&mut stdout, &mut log_file).await {
                            tracing::warn!(%log_path_str, error = %e, "stdout pipe reader failed");
                        }
                    });
                }
            }
            // Discard stderr (already captured by kill_on_drop cleanup)
            let _ = child.stderr.take();
        }

        // 7. Wait for completion
        let status = child
            .wait()
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to wait for process: {e}")))?;

        // 8. Determine and persist final status
        let (final_status, exit_code) = Self::determine_status(status);
        self.update_final_status(run_id, exit_code, final_status)
            .await?;
        self.maybe_update_health_status(run_id, final_status, exit_code)
            .await;
        self.maybe_update_server_status(run_id, final_status).await;

        Ok(())
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

    /// Linux implementation: uses PTY for line-buffered real-time output.
    #[cfg(target_os = "linux")]
    async fn perform(&self, args: CommandExecWorkerArgs) -> Result<()> {
        let CommandExecWorkerArgs { run_id } = args;

        // 1. Validate run exists and is still active
        let Ok(model) = self.fetch_and_validate(run_id).await else {
            return Ok(());
        };

        // 2. Create PTY so child's stdout is line-buffered (real-time output)
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
        // master_fd and slave_fd are now owned by their respective std::fs::File handles

        // 3. Build the command with PTY as stdout/stderr
        self.perform_inner(run_id, model, |cmd| {
            cmd.stdout(std::process::Stdio::from(slave.try_clone().map_err(
                |e| loco_rs::Error::string(&format!("failed to clone slave for stdout: {e}")),
            )?));
            cmd.stderr(std::process::Stdio::from(slave.try_clone().map_err(
                |e| loco_rs::Error::string(&format!("failed to clone slave for stderr: {e}")),
            )?));
            // slave (original) is dropped here, closing the original slave fd in parent
            Ok::<_, loco_rs::Error>(Some(master))
        })
        .await
    }

    /// Windows implementation: uses piped stdio.
    #[cfg(target_os = "windows")]
    async fn perform(&self, args: CommandExecWorkerArgs) -> Result<()> {
        let CommandExecWorkerArgs { run_id } = args;

        // 1. Validate run exists and is still active
        let Ok(model) = self.fetch_and_validate(run_id).await else {
            return Ok(());
        };

        // Use piped stdio instead of PTY
        self.perform_inner(run_id, model, |cmd| {
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            Ok::<_, loco_rs::Error>(None)
        })
        .await
    }
}
