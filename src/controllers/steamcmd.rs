use axum::response::Redirect;
use axum::routing::{get, post};
use loco_rs::controller::views::engines::TeraView;
use loco_rs::prelude::*;

use crate::{
    data::steamcmd::{SteamCmd, SteamCmdError},
    resolve_data_home, AppDirs,
};

/// GET /steamcmd — check installation status and show install prompt.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if rendering fails.
pub async fn check_status(ViewEngine(v): ViewEngine<TeraView>) -> Result<impl IntoResponse> {
    let data_home = resolve_data_home();
    let dirs = AppDirs::new(data_home);
    let steamcmd = SteamCmd::new(&dirs);

    let installed = steamcmd.is_installed();
    let binary_path = steamcmd.binary_path().to_string_lossy().to_string();

    crate::views::steamcmd::status(v, &binary_path, installed)
}

/// POST /steamcmd/install — install `SteamCMD` after user confirmation.
///
/// Redirects back to the status page on success, or returns an error message
/// on failure.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the installation fails.
pub async fn install() -> Result<impl IntoResponse> {
    let data_home = resolve_data_home();
    let dirs = AppDirs::new(data_home);
    let steamcmd = SteamCmd::new(&dirs);

    match steamcmd.ensure_installed().await {
        Ok(()) => Ok(Redirect::to("/steamcmd").into_response()),
        Err(e) => {
            let msg = error_message(&e);
            Err(loco_rs::Error::string(&msg))
        }
    }
}

/// Format a user-facing error message from a [`SteamCmdError`].
#[allow(clippy::doc_markdown)]
fn error_message(e: &SteamCmdError) -> String {
    match e {
        SteamCmdError::MissingDependencies(hint) => {
            format!("`SteamCMD` requires 32-bit dependencies: {hint}")
        }
        other => format!("Installation failed: {other}"),
    }
}

/// Register the `SteamCMD` routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("steamcmd")
        .add("/", get(check_status))
        .add("/install", post(install))
}
