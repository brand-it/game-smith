//! Background worker that spawns game server processes and streams their output to log
//! files.
//!
//! On Linux, uses PTY-based streaming via `libc::openpty` for proper terminal behavior.
//! On Windows, uses `ConPTY` via `portable-pty` with VT cursor-position response to avoid hangs.

use async_trait::async_trait;
use loco_rs::app::AppContext;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use tokio::io::AsyncRead;
use tokio::process::Command;

use crate::data::steamcmd::{set_shared_store, SteamCmdHealthStatus};
use crate::models::command_runs::{CommandStatus, Model as CommandRunModel};
use crate::models::game_servers::ServerStatus;

// Platform-specific modules
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

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
            if let Err(e) = active.finish(ctx, Some(-1), CommandStatus::Failed).await {
                tracing::warn!(run_id, error = %e, "failed to mark run as spawn-failed in DB");
            }
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
            if let Err(e) = active.update_pid(&self.ctx, i64::from(pid)).await {
                tracing::warn!(run_id, pid, error = %e, "failed to store PID in DB");
            }
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
            (CommandStatus::Failed, _) => {
                let raw_msg = model
                    .log_path
                    .as_ref()
                    .and_then(|log_path| std::fs::read_to_string(log_path).ok())
                    .and_then(|content| content.lines().last().map(String::from))
                    .unwrap_or_else(|| "health check failed".to_string());

                SteamCmdHealthStatus::Broken(raw_msg)
            }
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
    /// update command), that the server's `auto_restart` flag is set, and that
    /// the server's current status is [`ServerStatus::Running`]. A user-initiated
    /// stop sets the status to [`ServerStatus::Stopped`], which prevents restart.
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

        server.auto_restart && server.status() == ServerStatus::Running
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
    ///
    /// On Linux, also applies `LD_PRELOAD` for `SteamCMD` commands when a
    /// persisted hint file (`<steamcmd_dir>/.ld_preload`) is present, to
    /// work around 32-bit segfaults on glibc >= 2.38.
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

        // Apply LD_PRELOAD hint for steamcmd commands (glibc >= 2.38 workaround).
        // The hint file lives next to the binary: `<steamcmd_dir>/.ld_preload`.
        #[cfg(target_os = "linux")]
        if let Some(basename) = std::path::Path::new(&model.command)
            .file_name()
            .and_then(|n| n.to_str())
        {
            if basename == "steamcmd.sh" {
                if let Some(parent) = std::path::Path::new(&model.command).parent() {
                    if let Ok(hint) = std::fs::read_to_string(parent.join(".ld_preload")) {
                        let hint = hint.trim();
                        if !hint.is_empty() {
                            cmd.env("LD_PRELOAD", hint);
                        }
                    }
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

/// Drain PTY master output: forward to log file and reply to ANSI DSR queries.
///
/// Reads `reader` until EOF, writing non-DSR bytes to `log_writer`. When an
/// `ESC[6n` (Device Status Report / cursor-position query) is detected — even
/// across read-call boundaries — an `ESC[1;1R` CPR reply is written to `writer`
/// so the child process never stalls waiting for a terminal response.
///
/// The DSR byte sequence is stripped from the log so raw escape bytes do not
/// appear in the visible output.
///
/// # Arguments
/// * `reader`     — source of PTY output (from `portable_pty` or a pipe in tests).
/// * `writer`     — sink for CPR replies (to `portable_pty` master writer or pipe).
/// * `log_writer` — optional buffered sink for cleaned log output.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn drain_pty_output(
    mut reader: impl std::io::Read,
    mut writer: impl std::io::Write,
    mut log_writer: Option<impl std::io::Write>,
) {
    // ESC[6n  — ANSI DSR cursor-position query sent by child.
    const DSR: &[u8] = b"\x1b[6n";
    // ESC[1;1R — CPR response: row 1, col 1.
    const CPR: &[u8] = b"\x1b[1;1R";
    // Ring buffer to detect DSR split across read boundaries.
    // Holds up to DSR.len() - 1 bytes of unconfirmed suffix from the previous chunk.
    let mut pending: Vec<u8> = Vec::with_capacity(DSR.len() - 1);

    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                // Combine leftover tail from last chunk with new data so we
                // can detect DSR sequences that straddle a read boundary.
                let mut window: Vec<u8> = pending.clone();
                window.extend_from_slice(&buf[..n]);

                let mut log_out: Vec<u8> = Vec::with_capacity(window.len());
                let mut i = 0usize;
                while i < window.len() {
                    // Check if DSR starts at position i.
                    if window[i..].starts_with(DSR) {
                        // Send CPR reply; swallow the DSR bytes from the log.
                        if let Err(e) = writer.write_all(CPR) {
                            tracing::warn!(error = %e, "failed to write CPR reply to PTY master");
                        }
                        i += DSR.len();
                    } else if DSR.starts_with(&window[i..]) && i + DSR.len() > window.len() {
                        // Possible DSR prefix at the very end of the window —
                        // don't write it to the log yet; carry it forward.
                        break;
                    } else {
                        log_out.push(window[i]);
                        i += 1;
                    }
                }
                // Save whatever we didn't consume as the new pending tail.
                pending = window[i..].to_vec();

                if let Some(ref mut w) = log_writer {
                    if let Err(e) = w.write_all(&log_out) {
                        tracing::warn!(error = %e, "failed to write PTY output to log file");
                    } else if let Err(e) = w.flush() {
                        tracing::warn!(error = %e, "failed to flush PTY output to log file");
                    }
                }
            }
        }
    }
    // Flush any unconsumed pending bytes (partial non-DSR tail) to the log.
    if !pending.is_empty() {
        if let Some(ref mut w) = log_writer {
            if let Err(e) = w.write_all(&pending) {
                tracing::warn!(error = %e, "failed to write final PTY bytes to log file");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Run drain_pty_output with in-memory reader/writer and collect results.
    fn drain(input: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut replies: Vec<u8> = Vec::new();
        let mut log: Vec<u8> = Vec::new();
        drain_pty_output(input, &mut replies, Some(&mut log));
        (replies, log)
    }

    // ── DSR reply ────────────────────────────────────────────────────────────

    /// A lone ESC[6n must produce exactly one ESC[1;1R reply.
    #[test]
    fn dsr_single_produces_reply() {
        let (replies, _log) = drain(b"\x1b[6n");
        assert_eq!(replies, b"\x1b[1;1R", "expected CPR reply");
    }

    /// Two DSR queries in one read must produce two CPR replies.
    #[test]
    fn dsr_double_produces_two_replies() {
        let (replies, _log) = drain(b"\x1b[6n\x1b[6n");
        assert_eq!(replies, b"\x1b[1;1R\x1b[1;1R");
    }

    // ── DSR filtered from log ─────────────────────────────────────────────────

    /// ESC[6n bytes must NOT appear in the log output.
    #[test]
    fn dsr_not_written_to_log() {
        let (_replies, log) = drain(b"hello\x1b[6nworld");
        assert_eq!(log, b"helloworld", "DSR must be stripped from log");
        assert!(!log.contains(&0x1b), "no escape bytes in log");
    }

    /// Pure log content (no DSR) is written through unmodified.
    #[test]
    fn plain_text_passes_through() {
        let (_replies, log) = drain(b"Loading Steam API...OK\n");
        assert_eq!(log, b"Loading Steam API...OK\n");
    }

    // ── split-read detection ──────────────────────────────────────────────────

    /// ESC[6n split as [ESC] then [[6n] across two separate reads must still
    /// produce one CPR reply and no DSR bytes in the log.
    #[test]
    fn dsr_split_across_reads_produces_reply() {
        // Simulate two reads by concatenating with a sentinel that forces
        // the ring buffer path: feed bytes one at a time.
        let input = b"\x1b[6n";
        let mut replies: Vec<u8> = Vec::new();
        let mut log: Vec<u8> = Vec::new();

        // Use a cursor over byte-at-a-time chunks via a custom reader.
        struct ByteByByte<'a> {
            data: &'a [u8],
            pos: usize,
        }
        impl std::io::Read for ByteByByte<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.pos >= self.data.len() {
                    return Ok(0);
                }
                buf[0] = self.data[self.pos];
                self.pos += 1;
                Ok(1)
            }
        }

        drain_pty_output(
            ByteByByte {
                data: input,
                pos: 0,
            },
            &mut replies,
            Some(&mut log),
        );

        assert_eq!(replies, b"\x1b[1;1R", "split DSR must still get a reply");
        assert!(!log.contains(&0x1b), "split DSR must not appear in log");
        assert!(log.is_empty(), "no other bytes to log");
    }

    /// ESC[6n split as [ESC[] then [6n] across two reads (2+2 byte split).
    #[test]
    fn dsr_split_two_plus_two_produces_reply() {
        let input = b"AB\x1b[6nCD";
        let mut replies: Vec<u8> = Vec::new();
        let mut log: Vec<u8> = Vec::new();

        // Feed in two-byte chunks
        struct TwoBytes<'a> {
            data: &'a [u8],
            pos: usize,
        }
        impl std::io::Read for TwoBytes<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.pos >= self.data.len() {
                    return Ok(0);
                }
                let n = std::cmp::min(2, self.data.len() - self.pos);
                buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
                self.pos += n;
                Ok(n)
            }
        }

        drain_pty_output(
            TwoBytes {
                data: input,
                pos: 0,
            },
            &mut replies,
            Some(&mut log),
        );

        assert_eq!(replies, b"\x1b[1;1R");
        assert_eq!(log, b"ABCD", "surrounding bytes intact, DSR stripped");
    }

    // ── no DSR, no reply ─────────────────────────────────────────────────────

    /// Output with no DSR query must produce no replies.
    #[test]
    fn no_dsr_no_reply() {
        let (replies, log) = drain(b"Waiting for confirmation...\n");
        assert!(replies.is_empty());
        assert_eq!(log, b"Waiting for confirmation...\n");
    }

    /// Empty input must produce no replies and empty log.
    #[test]
    fn empty_input() {
        let (replies, log) = drain(b"");
        assert!(replies.is_empty());
        assert!(log.is_empty());
    }
}
