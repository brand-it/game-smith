//! Shutdown status page controller.
//!
//! Renders a shutdown status page with per-server progress. A JSON API at
//! `/shutdown/status` returns the current state of all servers being stopped.
//!
//! Ground truth for server liveness comes from `is_alive()` (OS PID check),
//! not from any synthetic tracking state.

use crate::initializers::embedded_i18n::EmbeddedViews;
use axum::extract::State;
use axum::routing::get;
use axum::Json;
use loco_rs::app::AppContext;
use loco_rs::controller::views::ViewEngine;
use loco_rs::prelude::*;
use serde::Serialize;

/// Per-server shutdown status.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerShutdownStatus {
    /// Termination signal sent; waiting for process to exit.
    Stopping,
    /// Server process confirmed stopped.
    Stopped,
    /// Failed to stop (includes error reason).
    Failed(String),
}

/// A single server's shutdown status record.
#[derive(Clone, Debug, Serialize)]
pub struct ShutdownServer {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub status: ServerShutdownStatus,
}

/// Top-level shutdown state exposed via the status API.
#[derive(Clone, Debug, Serialize)]
pub struct ShutdownStatus {
    /// Whether a shutdown sequence is currently in progress.
    pub shutting_down: bool,
    /// Per-server status.
    pub servers: Vec<ShutdownServer>,
}

/// Build a [`ShutdownStatus`] from live PID checks across all servers.
async fn build_status(ctx: &AppContext) -> ShutdownStatus {
    let all_servers = crate::models::game_servers::Model::list(ctx)
        .await
        .unwrap_or_default();

    let mut servers = Vec::with_capacity(all_servers.len());
    for s in &all_servers {
        let alive = crate::models::game_servers::is_alive(ctx, s).await;
        let status = if alive {
            ServerShutdownStatus::Stopping
        } else {
            ServerShutdownStatus::Stopped
        };
        servers.push(ShutdownServer {
            id: s.id,
            name: s.name.clone(),
            error: None,
            status,
        });
    }

    let shutting_down = servers
        .iter()
        .any(|s| matches!(s.status, ServerShutdownStatus::Stopping));

    ShutdownStatus {
        shutting_down,
        servers,
    }
}

/// GET /shutdown — render the shutdown page and begin graceful exit.
///
/// Spawns a background task that stops every alive server, then polls
/// `is_alive()` until all processes are dead before exiting the HTTP server.
///
/// # Errors
/// Returns an error if rendering fails.
pub async fn show(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    let all_servers = crate::models::game_servers::Model::list(&ctx)
        .await
        .unwrap_or_default();

    let ctx_clone = ctx.clone();
    let spawn_servers = all_servers.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await; // Brief delay to allow the initial page response to be sent before we start killing processes. makes thing look fancy

        // Send SIGTERM to every alive server.
        for server in &spawn_servers {
            let alive = crate::models::game_servers::is_alive(&ctx_clone, server).await;
            if !alive {
                continue;
            }
            tracing::info!(server_id = server.id, "stopping server during shutdown");
            let _ = server.stop(&ctx_clone).await;
        }

        // Wait until all processes are actually dead (max 30s).
        for _ in 0..60 {
            let mut still_alive = false;
            for s in &spawn_servers {
                if crate::models::game_servers::is_alive(&ctx_clone, s).await {
                    still_alive = true;
                    break;
                }
            }
            if !still_alive {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        // Allow time for the browser to receive the final status update.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        tracing::info!("shutdown complete");
        std::process::exit(0);
    });

    crate::views::shutdown::show(&v, &all_servers)
}

/// GET /shutdown/status — JSON API returning current shutdown progress.
///
/// Queries all servers from the DB and computes live status via `is_alive()`.
///
/// # Errors
/// Returns an error if serialization fails.
pub async fn status_api(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let status = build_status(&ctx).await;
    Ok(Json(status))
}

/// GET /ping — health check. Returns 200 OK while the server is alive.
///
/// During an active shutdown sequence, returns JSON with shutdown status
/// instead of a minimal OK object.
pub async fn ping(State(ctx): State<AppContext>) -> impl IntoResponse {
    let all_servers = crate::models::game_servers::Model::list(&ctx)
        .await
        .unwrap_or_default();

    let mut shutting_down = false;
    for s in &all_servers {
        if crate::models::game_servers::is_alive(&ctx, s).await {
            shutting_down = true;
            break;
        }
    }

    let body: serde_json::Value = if shutting_down {
        let status = build_status(&ctx).await;
        serde_json::to_value(&status)
            .unwrap_or_else(|_| serde_json::json!({ "status": "shutting_down" }))
    } else {
        serde_json::json!({ "status": "ok" })
    };
    Json(body)
}

/// Register the shutdown routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("shutdown")
        .add("/", get(show))
        .add("/status", get(status_api))
}

/// Register the ping route at the top level.
pub fn ping_route() -> Routes {
    Routes::new().add("/ping", get(ping))
}
