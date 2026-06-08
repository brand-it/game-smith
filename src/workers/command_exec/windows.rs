use std::collections::HashMap;
use std::io;
use tokio::process::Command;

use portable_pty;

use crate::models::command_runs::{CommandStatus, Model as CommandRunModel};

impl super::CommandExecWorker {
    /// Spawn a single process using ConPTY (Windows: `portable-pty`).
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
    pub(super) async fn spawn_one(
        &self,
        run_id: i32,
        model: &CommandRunModel,
    ) -> loco_rs::Result<(CommandStatus, Option<i32>)> {
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
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| loco_rs::Error::string(&format!("failed to clone pty reader: {e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| loco_rs::Error::string(&format!("failed to take pty writer: {e}")))?;

        let log_path = model.log_path.clone();

        // Drain thread: read PTY output, reply to ESC[6n, write to log file.
        // See drain_pty_output for the full description of the algorithm.
        let drain_handle = std::thread::spawn(move || {
            let log_writer = log_path.as_deref().and_then(|p| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .ok()
            });
            super::drain_pty_output(reader, writer, log_writer);
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
    fn determine_pty_status(status: portable_pty::ExitStatus) -> (CommandStatus, Option<i32>) {
        if status.success() {
            (CommandStatus::Completed, Some(0))
        } else {
            let code = status.exit_code() as i32;
            (CommandStatus::Failed, Some(code))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spawn a command via ConPTY and verify determine_pty_status maps
    /// success to Completed/0.
    #[test]
    fn determine_pty_status_maps_success() {
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: 25,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("failed to create pty");

        let mut cmd = portable_pty::CommandBuilder::new("cmd.exe");
        cmd.args(&["/c", "exit", "/b", "0"]);
        let child = pair
            .slave
            .spawn_command(cmd)
            .expect("failed to spawn command");

        // Wait for exit (polling loop to avoid blocking)
        let mut child = child;
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(_) => panic!("failed to poll child status"),
            }
        };

        drop(pair);
        let (cmd_status, code) = super::super::CommandExecWorker::determine_pty_status(exit_status);
        assert_eq!(cmd_status, CommandStatus::Completed);
        assert_eq!(code, Some(0));
    }

    /// Spawn a command via ConPTY and verify determine_pty_status maps
    /// failure to Failed with the correct exit code.
    #[test]
    fn determine_pty_status_maps_failure() {
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: 25,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("failed to create pty");

        let mut cmd = portable_pty::CommandBuilder::new("cmd.exe");
        cmd.args(&["/c", "exit", "/b", "42"]);
        let child = pair
            .slave
            .spawn_command(cmd)
            .expect("failed to spawn command");

        let mut child = child;
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(_) => panic!("failed to poll child status"),
            }
        };

        drop(pair);
        let (cmd_status, code) = super::super::CommandExecWorker::determine_pty_status(exit_status);
        assert_eq!(cmd_status, CommandStatus::Failed);
        assert_eq!(code, Some(42));
    }
}
