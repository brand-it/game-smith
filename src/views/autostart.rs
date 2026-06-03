//! Autostart settings views.

use loco_rs::app::AppContext;
use loco_rs::prelude::*;

/// Render the autostart settings page.
///
/// Shows the current autostart state with a toggle button.
/// Displays success/error messages from the last toggle operation.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn settings(
    _ctx: &AppContext,
    v: impl ViewRenderer,
    enabled: bool,
    message: Option<&str>,
) -> Result<impl IntoResponse> {
    format::render().view(
        &v,
        "autostart/settings.html",
        data!({
            "enabled": enabled,
            "message": message,
        }),
    )
}
