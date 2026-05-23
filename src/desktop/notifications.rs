//! Desktop notification helper.
//!
//! Uses `notify-rust` on Linux; falls back to logging on other platforms.

#[cfg(target_os = "linux")]
use notify_rust::Notification;

/// Sends a desktop notification.
///
/// Errors are logged but not propagated, since notification failures
/// should never crash the application.
pub fn notify(title: &str, message: &str) {
    #[cfg(target_os = "linux")]
    if let Err(e) = Notification::new()
        .appname("game-smith")
        .summary(title)
        .body(message)
        .show()
    {
        tracing::warn!(error = %e, "failed to send desktop notification");
    }

    #[cfg(not(target_os = "linux"))]
    tracing::info!(title = %title, message = %message, "desktop notification");
}
