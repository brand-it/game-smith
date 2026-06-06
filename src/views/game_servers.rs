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
    pub fn new(server: &'a GameServerModel) -> Self {
        Self {
            is_running: server.is_running(),
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
        views.push(GameServerView::new(server));
    }
    format::render().view(&v, "game_servers/list.html", data!({ "servers": views }))
}

/// Render the new game server install form.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn new_form(v: impl ViewRenderer, steam_username: Option<&str>) -> Result<impl IntoResponse> {
    format::render().view(
        &v,
        "game_servers/new.html",
        data!({ "steam_username": steam_username }),
    )
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
    let view = GameServerView::new(server);

    let has_steam_creds = crate::models::steam_credentials::Model::is_configured(ctx)
        .await
        .unwrap_or(false);
    // Query latest command run for this server
    let latest_run_model =
        crate::models::command_runs::Model::find_latest_by_server(ctx, i64::from(server.id))
            .await
            .ok()
            .flatten();
    let latest_run = latest_run_model
        .as_ref()
        .map(crate::models::command_runs::CommandRunView::new);

    // Read log content if a run exists
    let run_log = if let Some(ref run_model) = latest_run_model {
        crate::data::command_runner::CommandRunner::new(ctx)
            .tail(run_model.id, None)
            .await
            .unwrap_or_default()
    } else {
        String::new()
    };

    format::render().view(
        &v,
        "game_servers/show.html",
        data!({
            "server": view,
            "latest_run": latest_run,
            "run_log": run_log,
            "has_steam_creds": has_steam_creds,
        }),
    )
}
