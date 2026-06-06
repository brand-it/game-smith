use libc;

/// Cross-platform signal constant for termination.
/// Resolves to `libc::SIGTERM` on Linux.
pub const TERM_SIGNAL: libc::c_int = libc::SIGTERM;

/// Check if a process is alive by sending signal 0.
/// Returns `true` if the process exists and is accessible.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn check_pid_alive(pid: i64) -> bool {
    let result = unsafe { libc::kill(pid as libc::c_int, 0) };
    result == 0
}

/// Send a signal to a process by PID (Linux: `libc::kill`).
/// Returns `0` on success, `-1` on error.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn kill_pid(pid: i64, signal: libc::c_int) -> libc::c_int {
    unsafe { libc::kill(pid as libc::c_int, signal) }
}

/// Send a signal to an entire process group by PGID.
///
/// Passing a negative process ID to `kill(2)` targets the process group
/// rather than a single process. The PGID is stored as a positive `i64`
/// in the database (it's the child's PID after `setpgid(0, 0)`).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn kill_process_group(pgid: i64, signal: libc::c_int) -> libc::c_int {
    // Negative PGID signals the entire group.
    unsafe { libc::kill(-(pgid as libc::c_int), signal) }
}

/// Check if any process in a process group is still alive.
///
/// Signal-0 to a negative PGID succeeds if the group exists.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn check_process_group_alive(pgid: i64) -> bool {
    unsafe { libc::kill(-(pgid as libc::c_int), 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_signal_is_sigterm() {
        assert_eq!(TERM_SIGNAL, libc::SIGTERM);
    }

    #[test]
    fn check_pid_alive_returns_true_for_self() {
        let pid = std::process::id() as i64;
        assert!(check_pid_alive(pid));
    }

    #[test]
    fn check_pid_alive_returns_false_for_nonexistent() {
        assert!(!check_pid_alive(999999));
    }

    #[test]
    fn check_pid_alive_lifecycle() {
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id() as i64;

        // Verify alive
        assert!(check_pid_alive(pid));

        // Kill it
        kill_pid(pid, libc::SIGKILL);

        // Reap the zombie so the PID is fully cleaned up.
        let _ = child.wait();

        // Verify dead
        assert!(!check_pid_alive(pid));
    }
}
