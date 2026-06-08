use libc::openpty;
use std::os::fd::FromRawFd;
use tokio::process::Command;

use crate::models::command_runs::{CommandStatus, Model as CommandRunModel};

impl super::CommandExecWorker {
    /// Spawn a single process using a PTY (Linux: `libc::openpty`).
    ///
    /// Creates a PTY pair, sets stdout/stderr to the slave, puts the child in
    /// its own process group, and streams the master fd to the log file.
    pub(super) async fn spawn_one(
        &self,
        run_id: i32,
        model: &CommandRunModel,
    ) -> loco_rs::Result<(CommandStatus, Option<i32>)> {
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

        // Put the child in its own process group so we can terminate the
        // entire process tree (including grandchildren) during shutdown.
        unsafe {
            cmd.pre_exec(|| {
                let ret = libc::setpgid(0, 0);
                if ret != 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify successful exit maps to Completed with code 0.
    #[test]
    fn determine_status_maps_success() {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 0")
            .status()
            .expect("failed to run command");
        let (cmd_status, code) = super::super::CommandExecWorker::determine_status(status);
        assert_eq!(cmd_status, CommandStatus::Completed);
        assert_eq!(code, Some(0));
    }

    /// Verify non-zero exit maps to Failed with the correct code.
    #[test]
    fn determine_status_maps_failure() {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 42")
            .status()
            .expect("failed to run command");
        let (cmd_status, code) = super::super::CommandExecWorker::determine_status(status);
        assert_eq!(cmd_status, CommandStatus::Failed);
        assert_eq!(code, Some(42));
    }
}
