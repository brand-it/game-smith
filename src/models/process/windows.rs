/// Cross-platform signal constant for termination.
/// Resolves to `0` on Windows (ignored by `kill_pid`).
pub const TERM_SIGNAL: i32 = 0;

/// Check if a process is alive on Windows.
///
/// Opens the process with `PROCESS_QUERY_LIMITED_INFORMATION` (works without
/// elevation) and calls `GetExitCodeProcess` to verify it hasn't exited.
/// The handle is properly closed via `CloseHandle` to avoid handle leaks.
#[must_use]
pub fn check_pid_alive(pid: i64) -> bool {
    check_pid_alive_impl(pid)
}

/// Unsafe implementation of [`check_pid_alive`] with Win32 API calls.
///
/// Returns `true` if the process is still running.
#[must_use]
fn check_pid_alive_impl(pid: i64) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // STILL_ACTIVE (0x103) — not re-exported by the windows crate.
    const STILL_ACTIVE: u32 = 259;

    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32) }
    {
        Ok(h) => h,
        Err(_) => return false,
    };

    let mut exit_code: u32 = 0;
    let success = unsafe { GetExitCodeProcess(handle, &mut exit_code) }.is_ok();
    let alive = success && exit_code == STILL_ACTIVE;

    // Always close the handle to avoid leaks.
    // Always close the handle to avoid leaks. CloseHandle panics if the
    // Win32 call fails, which is the desired behavior (handle leak is a bug).
    unsafe { CloseHandle(handle) };

    alive
}

/// Terminate a process by PID on Windows using `TerminateProcess`.
///
/// Opens the process with `PROCESS_TERMINATE`, calls `TerminateProcess`,
/// and properly closes the handle. The `_signal` parameter is ignored
/// on Windows (process is always terminated).
#[must_use]
pub fn kill_pid(pid: i64, _signal: i32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    let Ok(handle) = (unsafe { OpenProcess(PROCESS_TERMINATE, false, pid as u32) }) else {
        return false;
    };

    let result = unsafe { TerminateProcess(handle, 1) }.is_ok();

    // Always close the handle to avoid leaks.
    unsafe { CloseHandle(handle) };

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_signal_is_zero_on_windows() {
        assert_eq!(TERM_SIGNAL, 0);
    }

    #[test]
    fn kill_pid_returns_false_for_nonexistent_process() {
        assert!(!kill_pid(999999, 0));
    }

    #[test]
    fn check_pid_alive_returns_true_for_current_process() {
        let pid = std::process::id() as i64;
        assert!(check_pid_alive(pid), "current process should be alive");
    }

    #[test]
    fn check_pid_alive_returns_false_for_nonexistent() {
        assert!(
            !check_pid_alive(999999),
            "nonexistent process should not be alive"
        );
    }

    #[test]
    fn check_pid_alive_returns_false_for_pid_zero() {
        // PID 0 on Windows refers to the current process but OpenProcess
        // with PID 0 returns ERROR_INVALID_PARAMETER.
        assert!(!check_pid_alive(0), "PID 0 should not be reported alive");
    }

    #[test]
    fn kill_pid_returns_false_for_pid_zero() {
        assert!(
            !kill_pid(0, 0),
            "cannot terminate PID 0 (system idle process)"
        );
    }

    #[test]
    fn check_pid_alive_lifecycle() {
        // Spawn a long-running process using cmd.exe
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "61", "127.0.0.1"])
            .spawn()
            .expect("spawn ping");
        let pid = child.id() as i64;

        // Give the process time to start
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Verify alive
        assert!(check_pid_alive(pid), "child process should be alive");

        // Kill it
        assert!(kill_pid(pid, 0), "kill_pid should succeed");

        // Reap the child
        child.wait().expect("wait for child");

        // Verify dead
        assert!(
            !check_pid_alive(pid),
            "child process should be dead after termination"
        );
    }

    #[test]
    fn kill_pid_on_already_dead_process() {
        // Spawn and immediately let the process exit
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "exit"])
            .spawn()
            .expect("spawn cmd exit");
        let pid = child.id() as i64;

        // Wait for it to finish
        child.wait().expect("wait for child");

        // Verify it's dead
        assert!(!check_pid_alive(pid), "exited process should not be alive");

        // kill_pid on a dead process should return false (can't open handle)
        assert!(
            !kill_pid(pid, 0),
            "kill_pid on dead process should return false"
        );
    }
}
