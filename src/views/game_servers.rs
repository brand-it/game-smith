use loco_rs::app::AppContext;
use loco_rs::prelude::*;
use serde::Serialize;

use crate::models::game_servers::Model as GameServerModel;

/// Server view wrapper that adds PID-verified `is_running` for templates.
///
/// Serde serializes only struct fields — `is_running` is a method on
/// [`GameServerModel`] and won't appear in serialized output. This wrapper
/// adds `is_running` as a real field so Tera templates can use it in conditionals.
#[derive(Debug, Serialize)]
pub struct GameServerView<'a> {
    /// True when there is a running command run for this server with a live PID.
    pub is_running: bool,

    /// True when the server has been installed.
    pub is_installed: bool,

    /// True when the server is in an error state.
    pub is_error: bool,

    #[serde(flatten)]
    inner: &'a GameServerModel,
}

impl<'a> GameServerView<'a> {
    #[must_use]
    pub fn new_with_running(server: &'a GameServerModel, is_running: bool) -> Self {
        Self {
            is_running,
            is_installed: server.is_installed(),
            is_error: server.is_error(),
            inner: server,
        }
    }
}

/// Render the game server list page.
///
/// # Errors
/// Returns an error if template rendering fails.
pub async fn list(
    ctx: &AppContext,
    v: impl ViewRenderer,
    servers: &[GameServerModel],
) -> Result<impl IntoResponse> {
    let mut views = Vec::with_capacity(servers.len());
    for server in servers {
        let is_running = crate::models::game_servers::is_alive(ctx, server).await;
        views.push(GameServerView::new_with_running(server, is_running));
    }
    format::render().view(&v, "game_servers/list.html", data!({ "servers": views }))
}

/// Render the new game server install form.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn new_form(v: impl ViewRenderer) -> Result<impl IntoResponse> {
    format::render().view(&v, "game_servers/new.html", data!({}))
}

/// Render a single game server detail page.
///
/// # Errors
/// Returns an error if template rendering fails.
pub async fn show(
    ctx: &AppContext,
    v: impl ViewRenderer,
    server: &GameServerModel,
) -> Result<impl IntoResponse> {
    let is_running = crate::models::game_servers::is_alive(ctx, server).await;
    let view = GameServerView::new_with_running(server, is_running);
    format::render().view(&v, "game_servers/show.html", data!({ "server": view }))
}
