use crate::controllers::error::StandardError;
use crate::initializers::embedded_i18n::EmbeddedViews;
use crate::models::game_servers;
use crate::models::game_templates;
use axum::extract::Form;
use axum::response::Redirect;
use axum::routing::{get, post};
use loco_rs::model::ModelError;
use loco_rs::prelude::*;
use serde::Deserialize;

#[allow(clippy::struct_excessive_bools)]
/// Form data for creating or updating a template.
#[derive(Debug, Deserialize)]
pub struct TemplateForm {
    pub name: String,
    pub description: Option<String>,
    pub app_id: String,
    pub server_mod: Option<String>,
    pub beta_branch: Option<String>,
    pub boot_script: Option<String>,
    #[serde(default)]
    pub use_steam_login: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub auto_restart: bool,
    #[serde(default)]
    pub auto_update: bool,
    #[serde(default)]
    pub update_on_start: bool,
    pub restart_schedule: Option<String>,
}

/// Form data for importing a base64-encoded template.
#[derive(Debug, Deserialize)]
pub struct ImportForm {
    pub template_data: String,
}

/// GET /templates — list all templates.
///
/// # Errors
/// Returns a [`StandardError`] if the database query fails or rendering fails.
pub async fn list(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let templates = game_templates::Model::list(&ctx).await.map_err(|e| {
        StandardError::InternalServerError(format!("failed to list templates: {e}"))
    })?;
    Ok(crate::views::game_templates::list(&v, &templates)?)
}

/// GET /templates/new — show the create template form.
/// GET /templates/new — show the create template form.
///
/// # Errors
/// Returns a [`StandardError`] if rendering fails.
pub async fn new_form(
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    Ok(crate::views::game_templates::new_form(
        &v, None, None, None, None,
    )?)
}

/// GET /templates/new/from-server/:server_id — show the create template form
/// pre-filled with a game server's configuration.
///
/// # Errors
/// Returns a [`StandardError`] if the database query fails or rendering fails.
pub async fn new_form_from_server(
    Path(server_id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let server = game_servers::Model::find_by_id(&ctx, server_id)
        .await
        .map_err(|e| {
            StandardError::InternalServerError(format!("failed to find game server: {e}"))
        })?
        .ok_or_else(|| StandardError::NotFound("Game server not found".into()))?;
    Ok(crate::views::game_templates::new_form(
        &v,
        Some(&server.name),
        Some(&server),
        None,
        None,
    )?)
}

/// POST /templates — create a new template.
///
/// # Errors
/// Returns a [`StandardError`] if validation fails or the database operation fails.
#[debug_handler]
pub async fn create(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
    Form(form): Form<TemplateForm>,
) -> Result<impl IntoResponse, StandardError> {
    // Validate app_id
    let app_id: i32 = form
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

    // Filter empty strings from optional fields
    let description = form.description.filter(|s| !s.trim().is_empty());
    let server_mod = form.server_mod.filter(|s| !s.trim().is_empty());
    let beta_branch = form.beta_branch.filter(|s| !s.trim().is_empty());
    let boot_script = form.boot_script.filter(|s| !s.trim().is_empty());
    let restart_schedule = form.restart_schedule.filter(|s| !s.trim().is_empty());
    match game_templates::ActiveModel::create(
        &ctx,
        name.clone(),
        description.clone(),
        app_id,
        server_mod.clone(),
        beta_branch.clone(),
        boot_script.clone(),
        form.use_steam_login,
        form.auto_start,
        form.auto_restart,
        form.auto_update,
        form.update_on_start,
        restart_schedule.clone(),
    )
    .await
    {
        Ok(_) => Ok(Redirect::to("/templates").into_response()),
        Err(ModelError::EntityAlreadyExists) => {
            let form_data_model = game_templates::Model {
                created_at: chrono::Utc::now().into(),
                updated_at: chrono::Utc::now().into(),
                id: 0,
                name: name.clone(),
                description,
                app_id,
                server_mod,
                beta_branch,
                boot_script,
                use_steam_login: form.use_steam_login,
                auto_start: form.auto_start,
                auto_restart: form.auto_restart,
                auto_update: form.auto_update,
                update_on_start: form.update_on_start,
                restart_schedule,
            };
            Ok(crate::views::game_templates::new_form(
                &v,
                None,
                None,
                Some("A template with this name already exists."),
                Some(&form_data_model),
            )?
            .into_response())
        }
        Err(other) => Err(StandardError::InternalServerError(format!(
            "failed to create template: {other}"
        ))),
    }
}

/// GET /templates/:id — show template details.
///
/// # Errors
/// Returns a [`StandardError`] if the template is not found or rendering fails.
pub async fn show(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    let template = game_templates::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to find template: {e}")))?
        .ok_or_else(|| StandardError::NotFound("Template not found".into()))?;
    let export_str = template.export().map_err(|e| {
        StandardError::InternalServerError(format!("failed to export template: {e}"))
    })?;
    Ok(crate::views::game_templates::show(
        &v,
        &template,
        &export_str,
        None,
        None,
    )?)
}

/// GET /templates/:id/edit — redirect to show (inline-editable).
///
/// # Errors
/// Returns a [`StandardError`] if the template is not found.
pub async fn edit_form(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse, StandardError> {
    let _template = game_templates::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to find template: {e}")))?
        .ok_or_else(|| StandardError::NotFound("Template not found".into()))?;
    Ok(Redirect::to(&format!("/templates/{id}")).into_response())
}

/// POST /templates/:id — update an existing template.
///
/// # Errors
/// Returns a [`StandardError`] if the template is not found or update fails.
#[debug_handler]
pub async fn update(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
    Form(form): Form<TemplateForm>,
) -> Result<impl IntoResponse, StandardError> {
    let template = game_templates::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to find template: {e}")))?
        .ok_or_else(|| StandardError::NotFound("Template not found".into()))?;

    // Validate app_id
    let app_id: i32 = form
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

    // Filter empty strings from optional fields
    let description = form.description.filter(|s| !s.trim().is_empty());
    let server_mod = form.server_mod.filter(|s| !s.trim().is_empty());
    let beta_branch = form.beta_branch.filter(|s| !s.trim().is_empty());
    let boot_script = form.boot_script.filter(|s| !s.trim().is_empty());
    let restart_schedule = form.restart_schedule.filter(|s| !s.trim().is_empty());

    let mut active: game_templates::ActiveModel = template.clone().into();
    match active
        .update_from_form(
            &ctx,
            name.clone(),
            description.clone(),
            app_id,
            server_mod.clone(),
            beta_branch.clone(),
            boot_script.clone(),
            form.use_steam_login,
            form.auto_start,
            form.auto_restart,
            form.auto_update,
            form.update_on_start,
            restart_schedule.clone(),
        )
        .await
    {
        Ok(_) => Ok(Redirect::to(&format!("/templates/{id}")).into_response()),
        Err(ModelError::EntityAlreadyExists) => {
            let export_str = template.export().map_err(|e| {
                StandardError::InternalServerError(format!("failed to export template: {e}"))
            })?;
            let form_data_model = game_templates::Model {
                created_at: chrono::Utc::now().into(),
                updated_at: chrono::Utc::now().into(),
                id: 0,
                name,
                description,
                app_id,
                server_mod,
                beta_branch,
                boot_script,
                use_steam_login: form.use_steam_login,
                auto_start: form.auto_start,
                auto_restart: form.auto_restart,
                auto_update: form.auto_update,
                update_on_start: form.update_on_start,
                restart_schedule,
            };
            Ok(crate::views::game_templates::show(
                &v,
                &template,
                &export_str,
                Some("A template with this name already exists."),
                Some(&form_data_model),
            )
            .map_err(|e| StandardError::InternalServerError(format!("failed to render: {e}")))?
            .into_response())
        }
        Err(other) => Err(StandardError::InternalServerError(format!(
            "failed to update template: {other}"
        ))),
    }
}

/// POST /templates/:id/delete — delete a template.
///
/// # Errors
/// Returns a [`StandardError`] if the template is not found or the database operation fails.
pub async fn delete(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse, StandardError> {
    let template = game_templates::Model::find_by_id(&ctx, id)
        .await
        .map_err(|e| StandardError::InternalServerError(format!("failed to find template: {e}")))?
        .ok_or_else(|| StandardError::NotFound("Template not found".into()))?;
    template.delete(&ctx.db).await.map_err(|e| {
        StandardError::InternalServerError(format!("failed to delete template: {e}"))
    })?;
    Ok(Redirect::to("/templates").into_response())
}

/// GET /templates/import — show the import form.
///
/// # Errors
/// Returns a [`StandardError`] if rendering fails.
pub async fn import_form(
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse, StandardError> {
    Ok(crate::views::game_templates::import_form(&v, None, None)?)
}

/// POST /templates/import — import a base64-encoded template.
///
/// # Errors
/// Returns a [`StandardError`] if the template data is invalid or rendering fails.
pub async fn import_template(
    ViewEngine(v): ViewEngine<EmbeddedViews>,
    State(ctx): State<AppContext>,
    Form(form): Form<ImportForm>,
) -> Result<impl IntoResponse, StandardError> {
    let data = form.template_data.trim().to_string();
    if data.is_empty() {
        return Ok(crate::views::game_templates::import_form(
            &v,
            Some("Template data cannot be empty"),
            None,
        )?
        .into_response());
    }

    match game_templates::Model::import(&ctx, &data) {
        Ok(import) => {
            Ok(crate::views::game_templates::import_form(&v, None, Some(&import))?.into_response())
        }
        Err(e) => {
            let msg = if format!("{e}").contains("invalid") || format!("{e}").contains("base64") {
                "Invalid base64 encoding. Please check the template string."
            } else if format!("{e}").contains("EOF") || format!("{e}").contains("expected") {
                "Malformed JSON in template data. Please check the template string."
            } else {
                "Failed to decode template. Please check the template string."
            };
            Ok(crate::views::game_templates::import_form(&v, Some(msg), None)?.into_response())
        }
    }
}

/// Register the game template routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("templates")
        .add("/", get(list))
        .add("/new", get(new_form))
        .add("/new/from-server/{server_id}", get(new_form_from_server))
        .add("/", post(create))
        .add("/{id}", get(show))
        .add("/{id}/edit", get(edit_form))
        .add("/{id}", post(update))
        .add("/{id}/delete", post(delete))
        .add("/import", get(import_form))
        .add("/import", post(import_template))
}
