//! Shutdown status page view.
//!
//! Renders a standalone HTML page showing per-server shutdown progress
//! with a live-updating polling client.

use crate::initializers::embedded_i18n::EmbeddedViews;
use crate::models::game_servers::Model as GameServerModel;
use loco_rs::prelude::*;
use serde::Serialize;

/// Minimal server view for shutdown template rendering.
#[derive(Debug, Serialize)]
pub struct ShutdownServerView<'a> {
    pub id: i32,
    pub name: &'a str,
    pub app_id: i32,
}

impl<'a> ShutdownServerView<'a> {
    #[must_use]
    pub fn new(server: &'a GameServerModel) -> Self {
        Self {
            id: server.id,
            name: &server.name,
            app_id: server.app_id,
        }
    }
}

/// Render the shutdown status page.
///
/// The initial server list is passed to the template so it can render
/// server cards immediately. The JS on the page polls `/shutdown/status`
/// for live status updates.
///
/// # Errors
/// Returns an error if template rendering fails.
pub fn show(
    v: &EmbeddedViews,
    servers: &[GameServerModel],
) -> Result<impl axum::response::IntoResponse> {
    let server_views: Vec<ShutdownServerView<'_>> =
        servers.iter().map(ShutdownServerView::new).collect();
    format::render().view(
        v,
        "game_servers/shutdown.html",
        data!({ "servers": &server_views }),
    )
}
