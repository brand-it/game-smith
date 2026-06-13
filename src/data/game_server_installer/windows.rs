use std::path::PathBuf;

/// Prepare shell command for executing a boot script on Windows.
///
/// Writes the script to a `.bat` file and returns `("cmd.exe", ["/C", bat_path])`.
/// Writing to a file first avoids fragile quoting when passing multiline scripts
/// inline to `cmd.exe /C`.
pub fn prepare_boot_command(
    install_dir: &str,
    script: &str,
) -> Result<(String, Vec<String>), std::io::Error> {
    let bat_path = write_boot_script_batch(install_dir, script)?;
    Ok((
        "cmd.exe".to_string(),
        vec!["/C".to_string(), bat_path.to_string_lossy().to_string()],
    ))
}

/// Writes the boot script to a `.bat` file and returns the path.
fn write_boot_script_batch(install_dir: &str, script: &str) -> Result<PathBuf, std::io::Error> {
    let bat_path = PathBuf::from(install_dir).join("game-smith-start.bat");
    let content = format!("@echo off\r\n{script}");
    std::fs::write(&bat_path, content)?;
    Ok(bat_path)
}

/// Find a server executable in the install directory.
///
/// Checks common server binary names for popular game servers on Windows.
pub fn find_server_executable(install_dir: &std::path::Path) -> Option<PathBuf> {
    let primary_candidates = [
        "srcds.exe",
        "srcds_run.exe",
        "hl.exe",
        "hlds.exe",
        "hlds_run.exe",
        "server.exe",
    ];

    for candidate in &primary_candidates {
        let path = install_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    // Fallback: check remaining variants
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
/// Sends termination signal to each individual process PID.
pub fn terminate_processes(runs: &[crate::models::command_runs::Model], server_id: i32) {
    for run in runs {
        if let Some(pid) = run.pid {
            if !crate::models::process::check_pid_alive(pid) {
                continue;
            }
            let _ = crate::models::process::kill_pid(pid, crate::models::process::TERM_SIGNAL);
            tracing::info!(
                server_id = server_id,
                run_id = run.id,
                pid = pid,
                "Sent termination signal to process"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_boot_command() {
        let dir = std::env::temp_dir().join(format!("game-smith-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        let dir_str = dir.to_string_lossy().to_string();

        let script = "start /B server.exe -port 27015\r\necho done";
        let (cmd, args) =
            prepare_boot_command(&dir_str, script).expect("failed to prepare boot command");

        assert_eq!(cmd, "cmd.exe");
        assert_eq!(args[0], "/C");

        let bat_path = PathBuf::from(&args[1]);
        assert!(bat_path.exists(), "batch file was not created");
        assert_eq!(bat_path.file_name().unwrap(), "game-smith-start.bat");

        let content = std::fs::read_to_string(&bat_path).expect("failed to read batch file");
        assert!(content.starts_with("@echo off\r\n"));
        assert!(content.contains("start /B server.exe -port 27015"));
        assert!(content.contains("echo done"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
