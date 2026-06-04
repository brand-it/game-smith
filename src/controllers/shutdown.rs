//! Shutdown status page controller.
//!
//! Renders a shutdown status page with per-server progress. A JSON API at
//! `/shutdown/status` returns the current state of all servers being stopped.

use crate::data::shutdown_tracker;
use crate::initializers::embedded_i18n::EmbeddedViews;
use axum::extract::State;
use axum::routing::get;
use axum::Json;
use loco_rs::app::AppContext;
use loco_rs::controller::views::ViewEngine;
use loco_rs::prelude::*;

/// GET /shutdown — render the shutdown page and begin graceful exit.
///
/// Captures the list of running servers, seeds the shutdown tracker,
/// then stops each server one at a time in a background task while
/// updating the tracker. The rendered page polls the status API for
/// live progress.
///
/// # Errors
/// Returns an error if rendering fails.
pub async fn show(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    // Find running servers and initialize the tracker.
    let running_servers = crate::models::game_servers::Model::find_running(&ctx)
        .await
        .unwrap_or_default();

    let server_list: Vec<(i32, String)> = running_servers
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();

    shutdown_tracker::init(&server_list).await;

    let ctx_clone = ctx.clone();
    tokio::spawn(async move {
        for server in &running_servers {
            shutdown_tracker::set_stopping(server.id).await;
            tracing::info!(
                server_id = server.id,
                server_name = %server.name,
                "stopping server during shutdown"
            );

            match server.stop(&ctx_clone).await {
                Ok(()) => {
                    shutdown_tracker::set_stopped(server.id).await;
                    tracing::info!(
                        server_id = server.id,
                        server_name = %server.name,
                        "stopped server during shutdown"
                    );
                }
                Err(e) => {
                    shutdown_tracker::set_failed(server.id, e.to_string()).await;
                    tracing::error!(
                        server_id = server.id,
                        server_name = %server.name,
                        err = %e,
                        "failed to stop server during shutdown"
                    );
                }
            }
        }

        shutdown_tracker::mark_complete().await;

        // Allow time for the browser to receive the final status update
        // before the process exits.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        tracing::info!("shutdown complete");
        std::process::exit(0);
    });

    crate::views::shutdown::show(&v)
}

/// GET /shutdown/status — JSON API returning current shutdown progress.
///
/// Returns a JSON body with the overall shutdown state and per-server
/// status. The shutdown page polls this endpoint every 500ms.
///
/// # Errors
/// Returns an error if serialization fails.
pub async fn status_api() -> Result<impl IntoResponse> {
    let status = shutdown_tracker::status().await;
    Ok(Json(status))
}

/// GET /ping — health check. Returns 200 OK while the server is alive.
///
/// During an active shutdown sequence, returns JSON with shutdown status
/// instead of a minimal OK object.
pub async fn ping() -> impl IntoResponse {
    let status = shutdown_tracker::status().await;
    let body: serde_json::Value = if status.shutting_down {
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
