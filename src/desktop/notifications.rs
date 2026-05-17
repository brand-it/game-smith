//! Desktop notification helper using `notify-rust`.

use notify_rust::Notification;

/// Sends a desktop notification.
///
/// Errors are logged but not propagated, since notification failures
/// should never crash the application.
pub fn notify(title: &str, message: &str) {
    if let Err(e) = Notification::new()
        .appname("game-smith")
        .summary(title)
        .body(message)
        .show()
    {
        tracing::warn!(error = %e, "failed to send desktop notification");
    }
}
