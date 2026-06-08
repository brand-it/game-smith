use crate::initializers::embedded_i18n::EmbeddedViews;
use axum::response::Redirect;
use axum::routing::{get, post};
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::prelude::*;

use crate::data::steamcmd::{health_status, SteamCmd, SteamCmdHealthStatus};
use crate::models::command_runs::{ActiveModel as CommandRunActiveModel, Model as CommandRunModel};
use crate::workers::steamcmd_install::{SteamCmdInstallWorker, SteamCmdInstallWorkerArgs};
use crate::{resolve_data_home, AppDirs};

/// GET /steamcmd — check installation status and show install prompt.
///
/// Reads the health status from the shared store and queries the last
/// health check record for historical context.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if rendering fails.
pub async fn check_status(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    let data_home = resolve_data_home();
    let dirs = AppDirs::new(data_home);
    let steamcmd = SteamCmd::new(&dirs);

    let installed = steamcmd.is_installed();
    let binary_path = steamcmd.binary_path().to_string_lossy().to_string();

    // Read health status from shared store
    let health = health_status().unwrap_or(SteamCmdHealthStatus::Checking);
    let health_str = health.as_str();

    // Query last health check record
    let last_check = CommandRunModel::find_last_health_check(&ctx)
        .await
        .ok()
        .flatten();
    let last_check_id = last_check.as_ref().map(|r| r.id);
    let last_check_status = last_check.as_ref().map(|r| r.status.clone());

    crate::views::steamcmd::status(
        &v,
        &binary_path,
        installed,
        health_str,
        last_check_id,
        last_check_status.as_deref(),
    )
}

/// POST /steamcmd/install — queue `SteamCMD` installation via worker.
///
/// Creates a `CommandRun` record, dispatches the install worker, and
/// redirects to the command detail page where WebSocket streams progress.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the run cannot be created or worker dispatched.
pub async fn install(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let data_home = resolve_data_home();
    let dirs = AppDirs::new(data_home);
    let steamcmd = SteamCmd::new(&dirs);

    // Create log file path
    let log_dir = dirs.logs_dir.join("steamcmd");
    let _ = std::fs::create_dir_all(&log_dir);
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let log_path = log_dir.join(format!("install_{timestamp}.log"));
    let log_path_str = Some(log_path.to_string_lossy().to_string());

    // Create CommandRun record
    let run = CommandRunActiveModel::create_run(
        &ctx,
        "steamcmd_install".to_string(),
        vec![],
        Some(steamcmd.steamcmd_dir().to_string_lossy().to_string()),
        None,
        log_path_str,
        Some("SteamCMD Install".to_string()),
        None,
    )
    .await
    .map_err(|e| loco_rs::Error::string(&format!("failed to create install record: {e}")))?;

    // Dispatch worker
    let args = SteamCmdInstallWorkerArgs { run_id: run.id };
    SteamCmdInstallWorker::perform_later(&ctx, args)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to dispatch worker: {e}")))?;
    // Redirect to command detail page
    Ok(Redirect::to(&format!("/commands/{}", run.id)).into_response())
}

/// Register the `SteamCMD` routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("steamcmd")
        .add("/", get(check_status))
        .add("/install", post(install))
}
