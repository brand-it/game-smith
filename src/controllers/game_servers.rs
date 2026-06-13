use crate::controllers::error::StandardError;
use crate::initializers::embedded_i18n::EmbeddedViews;
use axum::extract::Form;
use axum::extract::Query;
use axum::response::Redirect;
use axum::routing::{get, post};
use loco_rs::model::ModelError;
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::data::encryption::EncryptionKey;
use crate::data::game_server_installer::GameServerInstaller;
use crate::models::game_servers;
use crate::models::game_servers::CreateServerForm;
use crate::models::game_servers::ServerStatus;
use crate::models::steam_credentials;
use crate::{resolve_data_home, AppDirs};

/// GET /servers — list all game servers.
///
/// # Errors
/// Returns a [`StandardError`] if the database query fails or rendering fails.
pub async fn list(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let servers = game_servers::Model::list(&ctx).await.map_err(|e| {
        StandardError::InternalServerError(format!("failed to list game servers: {e}"))
    })?;
    Ok(crate::views::game_servers::list(&ctx, &v, &servers).await?)
}

/// GET /servers/new — show the install form.
///
/// # Errors
/// Returns a [`StandardError`] if rendering fails.
pub async fn new_form(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let username = steam_credentials::Model::find(&ctx)
        .await
        .ok()
        .flatten()
        .map(|record| record.username);
    let templates = crate::models::game_templates::Model::list(&ctx)
        .await
        .unwrap_or_default();
    Ok(crate::views::game_servers::new_form(
        &v,
        username.as_deref(),
        &templates,
    )?)
}

/// GET /servers/new/form — show the install form, optionally pre-filled from a template.
///
/// If a `template_id` query parameter is provided and the template exists,
/// the form fields are pre-filled from the template data.
///
/// # Errors
/// Returns a [`StandardError`] if rendering fails.
pub async fn new_form_with_template(
    Query(params): Query<NewServerFormQuery>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let username = steam_credentials::Model::find(&ctx)
        .await
        .ok()
        .flatten()
        .map(|record| record.username);
    let template = if let Some(id) = params.template_id {
        Some(
            crate::models::game_templates::Model::find_by_id(&ctx, id)
                .await
                .map_err(|e| StandardError::InternalServerError(e.to_string()))?
                .ok_or_else(|| StandardError::NotFound("Template not found".into()))?,
        )
    } else {
        None
    };
    Ok(crate::views::game_servers::new_form_with_template(
        &v,
        username.as_deref(),
        template.as_ref(),
        None,
        None,
    )?)
}

/// GET /servers/new/select-template — show a card-based template browser.
///
/// # Errors
/// Returns a [`StandardError`] if the database query fails or rendering fails.
pub async fn select_template(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let templates = crate::models::game_templates::Model::list(&ctx)
        .await
        .unwrap_or_default();
    Ok(crate::views::game_servers::select_template(&v, &templates)?)
}

/// POST /servers — create a new game server and start installation.
///
/// Creates the game server record, kicks off the `SteamCMD` installation,
/// and redirects to the game server show page where installation progress is displayed.
///
/// # Errors
/// Returns a [`StandardError`] if validation fails, the record cannot be
/// created, or the installation cannot be started.
#[debug_handler]
#[allow(clippy::too_many_lines)]
pub async fn create(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
    Form(form): Form<CreateServerForm>,
) -> Result<impl IntoResponse, StandardError> {
    // Validate app_id (needed for error messages)
    let app_id: u32 = form
        .app_id
        .parse()
        .map_err(|_| StandardError::BadRequest("Invalid App ID".into()))?;

    if app_id == 0 {
        return Err(StandardError::BadRequest("App ID cannot be zero".into()));
    }

    // Validate name
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(StandardError::BadRequest("Name cannot be empty".into()));
    }

    // Load template if provided
    let template = if let Some(id) = form.template_id {
        match crate::models::game_templates::Model::find_by_id(&ctx, id).await {
            Ok(Some(t)) => Some(t),
            Ok(None) => {
                tracing::warn!(template_id = id, "Template not found, ignoring");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load template, ignoring");
                None
            }
        }
    } else {
        None
    };

    // Save Steam credentials if provided
    let steam_username = form
        .steam_username
        .clone()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());
    let steam_password = form.steam_password.clone().filter(|s| !s.trim().is_empty());

    if form.use_steam_login {
        if let (Some(ref username), Some(ref password)) = (steam_username, steam_password) {
            if !username.is_empty() && !password.is_empty() {
                match save_steam_credentials(&ctx, username, password).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to save Steam credentials");
                    }
                }
            }
        }
    }

    // Create game server record — model handles all data extraction and template merging
    let server = match game_servers::ActiveModel::create(&ctx, &form, template.as_ref()).await {
        Ok(server) => server,
        Err(ModelError::EntityAlreadyExists) => {
            let username = steam_credentials::Model::find(&ctx)
                .await
                .ok()
                .flatten()
                .map(|record| record.username);
            let form_data_model = game_servers::Model {
                id: 0,
                created_at: chrono::Utc::now().into(),
                updated_at: chrono::Utc::now().into(),
                app_id: app_id
                    .try_into()
                    .map_err(|_| StandardError::BadRequest("App ID out of range".into()))?,
                name: form.name.clone(),
                install_dir: game_servers::default_install_dir(&form.name),
                platform: String::new(),
                status: String::new(),
                boot_script: None,
                auto_start: false,
                auto_restart: false,
                auto_update: false,
                update_on_start: false,
                restart_schedule: None,
                last_error: None,
                server_mod: form.server_mod.clone(),
                beta_branch: form.beta_branch.clone(),
                use_steam_login: form.use_steam_login,
                template_id: form.template_id,
            };
            return Ok(crate::views::game_servers::new_form_with_template(
                &v,
                username.as_deref(),
                None,
                Some("A game server with this name or install directory already exists."),
                Some(&form_data_model),
            )?
            .into_response());
        }
        Err(other) => {
            return Err(StandardError::InternalServerError(format!(
                "failed to create game server: {other}"
            )));
        }
    };

    // Start installation
    let installer = GameServerInstaller::new(&ctx);
    let _run = installer.install(&server).await.map_err(|e| {
        StandardError::InternalServerError(format!("failed to start installation: {e}"))
    })?;

    // Update server status to installing
    if let Ok(Some(active_server)) = game_servers::Model::find_by_id(&ctx, server.id).await {
        let mut active: game_servers::ActiveModel = active_server.into();
        crate::log_result(
            active
                .update_status(&ctx, ServerStatus::Installing, None)
                .await,
            "updated server status to Installing",
            "failed to update server status to Installing",
        );
    }

    // Redirect to server show page
    Ok(Redirect::to(&format!("/servers/{}", server.id)).into_response())
}

/// GET /servers/:id — show game server details.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found or rendering fails.
pub async fn show(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    Ok(crate::views::game_servers::show(&ctx, &v, &server, None).await?)
}

/// POST /servers/:id/start — start a game server.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found or cannot be started.
pub async fn start_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    server
        .start(&ctx)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to start server: {e}")))?;
    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/stop — stop a running game server.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found or cannot be stopped.
pub async fn stop_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    server
        .stop(&ctx)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to stop server: {e}")))?;

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/update — update a game server via `SteamCMD`.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found or update fails.
pub async fn update_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;

    // Don't double-launch if an install/update is already in progress.
    if server.status() == ServerStatus::Installing {
        return Ok(Redirect::to(&format!("/servers/{id}")).into_response());
    }

    let installer = GameServerInstaller::new(&ctx);
    let _run = installer
        .update(&server)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to update server: {e}")))?;

    // Update server status
    if let Ok(Some(active_server)) = game_servers::Model::find_by_id(&ctx, id).await {
        let mut active: game_servers::ActiveModel = active_server.into();
        crate::log_result(
            active
                .update_status(&ctx, ServerStatus::Installing, None)
                .await,
            "updated server status to Installing",
            "failed to update server status to Installing",
        );
    }

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/boot-script — update the boot script for a game server.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found.
pub async fn update_boot_script(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    Form(form): Form<BootScriptForm>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    if game_servers::is_alive(&ctx, &server).await {
        return Err(StandardError::BadRequest(
            "Cannot update settings while server is running. Stop the server first.".into(),
        ));
    }

    let mut active: game_servers::ActiveModel = server.into();
    active
        .update_boot_script(&ctx, Some(form.boot_script))
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to update boot script: {e}"))
        })?;

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/delete — delete a game server record.
///
/// Does not remove files from the install directory.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found or deletion fails.
pub async fn delete_server(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
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
        .map_err(|e| StandardError::InternalServerError(format!("failed to delete server: {e}")))?;

    Ok(Redirect::to("/servers").into_response())
}

/// POST /servers/:id/auto-restart — update the auto-restart setting.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found.
pub async fn update_auto_restart(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    request: axum::extract::Request,
) -> Result<impl IntoResponse, StandardError> {
    // Parse form body manually to avoid deserialization errors on malformed input
    let body_bytes = axum::body::to_bytes(request.into_body(), 8192)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to read request body: {e}"))
        })?;
    let body_text = String::from_utf8_lossy(&body_bytes);
    let auto_restart = body_text.contains("auto_restart=true");

    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    if game_servers::is_alive(&ctx, &server).await {
        return Err(StandardError::BadRequest(
            "Cannot update settings while server is running. Stop the server first.".into(),
        ));
    }

    let mut active: game_servers::ActiveModel = server.into();
    active
        .update_auto_restart(&ctx, auto_restart)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to update auto-restart: {e}"))
        })?;

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/auto-start — update the auto-start setting.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found.
pub async fn update_auto_start(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    request: axum::extract::Request,
) -> Result<impl IntoResponse, StandardError> {
    let body_bytes = axum::body::to_bytes(request.into_body(), 8192)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to read request body: {e}"))
        })?;
    let body_text = String::from_utf8_lossy(&body_bytes);
    let auto_start = body_text.contains("auto_start=true");

    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    if game_servers::is_alive(&ctx, &server).await {
        return Err(StandardError::BadRequest(
            "Cannot update settings while server is running. Stop the server first.".into(),
        ));
    }

    let mut active: game_servers::ActiveModel = server.into();
    active
        .update_auto_start(&ctx, auto_start)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to update auto-start: {e}"))
        })?;

    Ok(Redirect::to(&format!("/servers/{id}")).into_response())
}

/// POST /servers/:id/settings — update game server settings.
///
/// # Errors
/// Returns a [`StandardError`] if the server is not found or update fails.
pub async fn update_settings(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
    Form(form): Form<UpdateSettingsForm>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;

    if game_servers::is_alive(&ctx, &server).await {
        return Err(StandardError::BadRequest(
            "Cannot update settings while server is running. Stop the server first.".into(),
        ));
    }

    let mut active: game_servers::ActiveModel = server.clone().into();
    match active
        .update_settings(
            &ctx,
            form.name,
            form.install_dir,
            form.server_mod,
            form.beta_branch,
            form.use_steam_login,
        )
        .await
    {
        Ok(_) => Ok(Redirect::to(&format!("/servers/{id}")).into_response()),
        Err(ModelError::EntityAlreadyExists) => crate::views::game_servers::show(
            &ctx,
            &v,
            &server,
            Some("A game server with this name or install directory already exists."),
        )
        .await
        .map(axum::response::IntoResponse::into_response)
        .map_err(|e| StandardError::InternalServerError(format!("failed to render: {e}"))),
        Err(other) => Err(StandardError::InternalServerError(format!(
            "failed to update settings: {other}"
        ))),
    }
}

/// Form data for updating the boot script.
#[derive(Debug, Deserialize)]
pub struct BootScriptForm {
    pub boot_script: String,
}

/// Form data for updating game server settings.
#[derive(Debug, Deserialize)]
pub struct UpdateSettingsForm {
    pub name: String,
    pub install_dir: String,
    pub server_mod: Option<String>,
    pub beta_branch: Option<String>,
    #[serde(default = "crate::models::game_servers::default_false")]
    pub use_steam_login: bool,
}

/// Query parameters for the new server form page.
#[derive(Debug, Deserialize)]
pub struct NewServerFormQuery {
    pub template_id: Option<i32>,
}

/// Register the game server routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("servers")
        .add("/", get(list))
        .add("/new", get(new_form))
        .add("/new/form", get(new_form_with_template))
        .add("/new/select-template", get(select_template))
        .add("/", post(create))
        .add("/{id}", get(show))
        .add("/{id}/start", post(start_server))
        .add("/{id}/stop", post(stop_server))
        .add("/{id}/update", post(update_server))
        .add("/{id}/boot-script", post(update_boot_script))
        .add("/{id}/delete", post(delete_server))
        .add("/{id}/auto-restart", post(update_auto_restart))
        .add("/{id}/auto-start", post(update_auto_start))
        .add("/{id}/settings", post(update_settings))
}

/// Save or update Steam credentials in the database.
///
/// Encrypts the password and stores it alongside the username.
async fn save_steam_credentials(
    ctx: &AppContext,
    username: &str,
    password: &str,
) -> Result<(), StandardError> {
    // Load encryption key
    let data_home = resolve_data_home();
    let dirs = AppDirs::new(data_home);
    let key_path = dirs.app_dir.join("secret.key");

    let key = EncryptionKey::load(&key_path).map_err(|e| {
        StandardError::InternalServerError(format!("failed to load encryption key: {e}"))
    })?;

    // Encrypt password
    let (nonce, ciphertext) = key.encrypt(password).map_err(|e| {
        StandardError::InternalServerError(format!("failed to encrypt password: {e}"))
    })?;

    // Store credentials
    let _record =
        steam_credentials::ActiveModel::store(ctx, username.to_string(), nonce, ciphertext)
            .await
            .map_err(|e| {
                StandardError::InternalServerError(format!("failed to save steam credentials: {e}"))
            })?;

    Ok(())
}
