//! Autostart management controller.
//!
//! Provides a web page for toggling the "start on login" setting,
//! and REST-style endpoints for the tray menu and programmatic use.

use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::routing::{get, post};
use loco_rs::controller::views::ViewEngine;
use loco_rs::prelude::*;

use crate::desktop::autostart;
use crate::initializers::embedded_i18n::EmbeddedViews;
/// GET /autostart — show the autostart settings page.
///
/// # Errors
/// Returns an error if the autostart status cannot be determined or rendering fails.
pub async fn settings(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    let enabled = autostart::is_enabled().unwrap_or(false);
    let response = crate::views::autostart::settings(&ctx, v, enabled, None)?;
    Ok(with_cache_control(response))
}

/// POST /autostart — toggle the autostart setting.
///
/// Enables autostart if it is currently disabled, disables if enabled.
///
/// # Errors
/// Returns an error if the autostart operation fails.
pub async fn toggle(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    let was_enabled = autostart::is_enabled().unwrap_or(false);
    tracing::info!(?was_enabled, "autostart: toggle requested");
    let result = if was_enabled {
        autostart::disable()
    } else {
        autostart::enable()
    };
    tracing::info!(?result, "autostart: toggle operation completed");

    // Re-query the actual state after the operation — the auto-launch library
    // may cache state internally, so assuming the opposite is unreliable.
    let now_enabled = autostart::is_enabled().unwrap_or(was_enabled);
    tracing::info!(?now_enabled, "autostart: current state after toggle");
    Ok(match result {
        Ok(()) => with_cache_control(crate::views::autostart::settings(
            &ctx,
            v,
            now_enabled,
            Some(if now_enabled {
                "Autostart enabled"
            } else {
                "Autostart disabled"
            }),
        )?),
        Err(e) => with_cache_control(crate::views::autostart::settings(
            &ctx,
            v,
            now_enabled,
            Some(&format!("Failed to toggle autostart: {e}")),
        )?),
    })
}

/// Attach no-cache headers to prevent browser caching of autostart state.
fn with_cache_control<R: IntoResponse>(response: R) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Expires", HeaderValue::from_static("0"));
    (headers, response)
}

/// Register the autostart routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("autostart")
        .add("/", get(settings))
        .add("/", post(toggle))
}
