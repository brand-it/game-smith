use axum::extract::Form;
use axum::response::Redirect;
use axum::routing::{get, post};
use loco_rs::controller::views::engines::TeraView;
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::data::game_server_installer::GameServerInstaller;
use crate::models::game_servers;
use crate::models::game_servers::ServerStatus;

/// Form data for creating a new game server.
#[derive(Debug, Deserialize)]
pub struct CreateServerForm {
    pub app_id: String,
    pub name: String,
    pub server_mod: Option<String>,
    pub beta_branch: Option<String>,
}

/// GET /servers — list all game servers.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the database query fails or rendering fails.
pub async fn list(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<TeraView>,
) -> Result<impl IntoResponse> {
    let servers = game_servers::Model::list(&ctx)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to list game servers: {e}")))?;
    crate::views::game_servers::list(&ctx, v, &servers).await
}

/// GET /servers/new — show the install form.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if rendering fails.
pub async fn new_form(ViewEngine(v): ViewEngine<TeraView>) -> Result<impl IntoResponse> {
    crate::views::game_servers::new_form(v)
}

/// POST /servers — create a new game server and start installation.
///
/// Creates the game server record, kicks off the `SteamCMD` installation,
/// and redirects to the command detail page where progress is streamed.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if validation fails, the record cannot be
/// created, or the installation cannot be started.
pub async fn create(
    State(ctx): State<AppContext>,
    Form(form): Form<CreateServerForm>,
) -> Result<impl IntoResponse> {
    // Validate app_id
    let app_id: u32 = form
        .app_id
        .parse()
        .map_err(|_| loco_rs::Error::string("Invalid App ID"))?;

    if app_id == 0 {
        return Err(loco_rs::Error::string("App ID cannot be zero"));
    }

    // Validate name
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(loco_rs::Error::string("Name cannot be empty"));
    }

    // Detect platform from the current OS
    let platform = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        _ => "linux",
    }
    .to_string();

    // Compute install directory
    let install_dir = game_servers::default_install_dir(&name);

    // Filter empty strings from optional fields
    let server_mod = form.server_mod.filter(|s| !s.trim().is_empty());
    let beta_branch = form.beta_branch.filter(|s| !s.trim().is_empty());

    // Create game server record
    let server = game_servers::ActiveModel::create(
        &ctx,
        app_id,
        name,
        install_dir,
        platform,
        server_mod,
        beta_branch,
    )
    .await
    .map_err(|e| loco_rs::Error::string(&format!("failed to create game server: {e}")))?;

    // Start installation
    let installer = GameServerInstaller::new(&ctx);
    let run = installer
        .install(&server)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to start installation: {e}")))?;

    // Update server status to installing
    if let Ok(Some(active_server)) = game_servers::Model::find_by_id(&ctx, server.id).await {
        let mut active: game_servers::ActiveModel = active_server.into();
        let _ = active
            .update_status(&ctx, ServerStatus::Installing, None)
            .await;
    }

    // Redirect to command detail page
    Ok(Redirect::to(&format!("/commands/{}", run.id)).into_response())
}

/// GET /servers/:id — show game server details.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the server is not found or rendering fails.
pub async fn show(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<TeraView>,
) -> Result<impl IntoResponse> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to find game server: {e}")))?
        .ok_or_else(|| loco_rs::Error::string("Game server not found"))?;
    crate::views::game_servers::show(&ctx, v, &server).await
}

/// POST /servers/:id/start — start a game server.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the server is not found or cannot be started.
pub async fn start_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to find game server: {e}")))?
        .ok_or_else(|| loco_rs::Error::string("Game server not found"))?;

    if game_servers::is_alive(&ctx, &server).await {
        return Ok(Redirect::to(&format!("/servers/{id}")).into_response());
    }

    let installer = GameServerInstaller::new(&ctx);
    let _run = installer
        .start(&server)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to start server: {e}")))?;

    // Update server status to running
    // PID is tracked asynchronously by CommandExecWorker
    if let Ok(Some(active_server)) = game_servers::Model::find_by_id(&ctx, id).await {
        let mut active: game_servers::ActiveModel = active_server.into();
        let _ = active
            .update_status(&ctx, ServerStatus::Running, None)
            .await;
    }

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/stop — stop a running game server.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the server is not found or cannot be stopped.
pub async fn stop_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to find game server: {e}")))?
        .ok_or_else(|| loco_rs::Error::string("Game server not found"))?;

    let installer = GameServerInstaller::new(&ctx);
    installer
        .stop(&server)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to stop server: {e}")))?;

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/update — update a game server via `SteamCMD`.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the server is not found or update fails.
pub async fn update_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to find game server: {e}")))?
        .ok_or_else(|| loco_rs::Error::string("Game server not found"))?;

    let installer = GameServerInstaller::new(&ctx);
    let run = installer
        .update(&server)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to update server: {e}")))?;

    // Update server status
    if let Ok(Some(active_server)) = game_servers::Model::find_by_id(&ctx, id).await {
        let mut active: game_servers::ActiveModel = active_server.into();
        let _ = active
            .update_status(&ctx, ServerStatus::Installing, None)
            .await;
    }

    Ok(Redirect::to(&format!("/commands/{}", run.id)).into_response())
}

/// POST /servers/:id/boot-script — update the boot script for a game server.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the server is not found.
pub async fn update_boot_script(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    Form(form): Form<BootScriptForm>,
) -> Result<impl IntoResponse> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to find game server: {e}")))?
        .ok_or_else(|| loco_rs::Error::string("Game server not found"))?;

    let mut active: game_servers::ActiveModel = server.into();
    active
        .update_boot_script(&ctx, Some(form.boot_script))
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to update boot script: {e}")))?;

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/delete — delete a game server record.
///
/// Does not remove files from the install directory.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the server is not found or deletion fails.
pub async fn delete_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to find game server: {e}")))?
        .ok_or_else(|| loco_rs::Error::string("Game server not found"))?;
    // Stop server if actually running
    if game_servers::is_alive(&ctx, &server).await {
        let installer = GameServerInstaller::new(&ctx);
        match installer.stop(&server).await {
            Ok(()) => (),
            Err(e) => {
                // Log the error but proceed with deletion anyway
                tracing::error!(server_id = server.id, error = %e, "failed to stop server before deletion");
            }
        }
    }

    let installer = GameServerInstaller::new(&ctx);
    installer
        .delete(&server)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to delete server: {e}")))?;

    Ok(Redirect::to("/servers").into_response())
}

/// Form data for updating the boot script.
#[derive(Debug, Deserialize)]
pub struct BootScriptForm {
    pub boot_script: String,
}

/// Register the game server routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("servers")
        .add("/", get(list))
        .add("/new", get(new_form))
        .add("/", post(create))
        .add("/{id}", get(show))
        .add("/{id}/start", post(start_server))
        .add("/{id}/stop", post(stop_server))
        .add("/{id}/update", post(update_server))
        .add("/{id}/boot-script", post(update_boot_script))
        .add("/{id}/delete", post(delete_server))
}
