//! Steam credential configuration views.

use loco_rs::app::AppContext;
use loco_rs::prelude::*;

/// Render the Steam credential configuration page.
///
/// Shows a form for entering Steam username and password.
/// Pre-populates the username field if credentials are already configured.
/// Displays error messages from validation failures.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn config(
    _ctx: &AppContext,
    v: impl ViewRenderer,
    username: Option<&str>,
    error: Option<&str>,
) -> Result<impl IntoResponse> {
    format::render().view(
        &v,
        "steam_config/config.html",
        data!({
            "username": username.unwrap_or(""),
            "error": error,
        }),
    )
}
