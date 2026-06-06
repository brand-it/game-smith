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
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, STILL_ACTIVE,
    };

    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32) }
    {
        Ok(h) => h,
        Err(_) => return false,
    };

    let mut exit_code: u32 = 0;
    let success = unsafe { GetExitCodeProcess(handle, &mut exit_code) }.is_ok();
    let alive = success && exit_code == STILL_ACTIVE;

    // Always close the handle to avoid leaks.
    let _ = unsafe { CloseHandle(handle) };

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
    let _ = unsafe { CloseHandle(handle) };

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
}
