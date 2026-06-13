use std::path::PathBuf;

/// Prepare shell command for executing a boot script.
/// Returns `("/bin/sh", ["-c", script])`. `install_dir` is unused on Linux.
#[allow(clippy::unnecessary_wraps)]
pub fn prepare_boot_command(
    _install_dir: &str,
    script: &str,
) -> Result<(String, Vec<String>), std::io::Error> {
    Ok((
        "/bin/sh".to_string(),
        vec!["-c".to_string(), script.to_string()],
    ))
}

/// Find a server executable in the install directory.
///
/// Checks common server binary names for popular game servers on Linux.
pub fn find_server_executable(install_dir: &std::path::Path) -> Option<PathBuf> {
    let primary_candidates = [
        "srcds_run",
        "srcds",
        "hl_linux",
        "hlds_run",
        "server",
        "game-server",
    ];

    for candidate in &primary_candidates {
        let path = install_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    // Fallback: check remaining variants (e.g. .exe on Linux for cross-platform installs)
    let fallback = [
        "srcds_run.exe",
        "srcds.exe",
        "server.exe",
        "game-server.exe",
    ];
    for candidate in &fallback {
        let path = install_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Terminate all running processes for a server.
///
/// Tries to signal the entire process group first, then falls back to
/// the single PID if group kill fails.
pub fn terminate_processes(runs: &[crate::models::command_runs::Model], server_id: i32) {
    for run in runs {
        if let Some(pid) = run.pid {
            if !crate::models::process::check_pid_alive(pid) {
                continue;
            }
            let ret = crate::models::process::kill_process_group(
                pid,
                crate::models::process::TERM_SIGNAL,
            );
            if ret == 0 {
                tracing::info!(
                    server_id = server_id,
                    run_id = run.id,
                    pgid = pid,
                    "Sent SIGTERM to process group"
                );
            } else {
                tracing::warn!(
                    server_id = server_id,
                    run_id = run.id,
                    pgid = pid,
                    error = ?std::io::Error::last_os_error(),
                    "Failed to signal process group; falling back to single PID"
                );
                let _ = crate::models::process::kill_pid(pid, crate::models::process::TERM_SIGNAL);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_boot_command() {
        let (cmd, args) = prepare_boot_command("/opt/cs2", "./srcds_run -game csgo")
            .expect("failed to prepare boot command");
        assert_eq!(cmd, "/bin/sh");
        assert_eq!(args, ["-c", "./srcds_run -game csgo"]);
    }
}
