//! Shutdown status page view.
//!
//! Renders a standalone HTML page showing per-server shutdown progress
//! with a live-updating polling client.

use crate::initializers::embedded_i18n::EmbeddedViews;
use loco_rs::prelude::*;

/// Render the shutdown status page.
///
/// The initial server list is passed to the template so it can render
/// placeholders immediately. The JS on the page polls `/shutdown/status`
/// for live updates.
///
/// # Errors
/// Returns an error if template rendering fails.
pub fn show(v: &EmbeddedViews) -> Result<impl axum::response::IntoResponse> {
    format::render().view(v, "game_servers/shutdown.html", data!({}))
}
