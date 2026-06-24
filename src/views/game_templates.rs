use loco_rs::prelude::*;
use serde::Serialize;

use crate::models::game_templates::{Model as TemplateModel, TemplateImport};

#[derive(Debug, Serialize)]
pub struct TemplateView<'a> {
    #[serde(flatten)]
    inner: &'a TemplateModel,
}

impl<'a> TemplateView<'a> {
    #[must_use]
    pub const fn new(template: &'a TemplateModel) -> Self {
        Self { inner: template }
    }
}

/// Render the template list page.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::unused_async)]
pub fn list(v: &impl ViewRenderer, templates: &[TemplateModel]) -> Result<impl IntoResponse> {
    let mut views = Vec::with_capacity(templates.len());
    for template in templates {
        views.push(TemplateView::new(template));
    }
    format::render().view(v, "game_templates/list.html", data!({ "templates": views }))
}

/// Render the new template form.
///
/// # Errors
/// Returns an error if template rendering fails.
pub fn new_form(
    v: &impl ViewRenderer,
    prefilled_name: Option<&str>,
    server: Option<&crate::models::game_servers::Model>,
    error: Option<&str>,
    form_data: Option<&crate::models::game_templates::Model>,
) -> Result<impl IntoResponse> {
    format::render().view(
        v,
        "game_templates/new.html",
        data!({
            "prefilled_name": prefilled_name,
            "server": server.map(TemplateView::new_from_server),
            "source_server_name": server.map(|s| s.name.as_str()),
            "error": error,
            "form_data": form_data,
        }),
    )
}

impl TemplateView<'_> {
    /// Create a view for pre-filling from a game server.
    #[must_use]
    pub fn new_from_server(server: &crate::models::game_servers::Model) -> ServerTemplateView {
        ServerTemplateView {
            name: format!("Template from {}", server.name),
            app_id: server.app_id,
            server_mod: server.server_mod.clone(),
            beta_branch: server.beta_branch.clone(),
            boot_script: server.boot_script.clone(),
            use_steam_login: server.use_steam_login,
            auto_start: server.auto_start,
            auto_restart: server.auto_restart,
            auto_update: server.auto_update,
            update_on_start: server.update_on_start,
            restart_schedule: server.restart_schedule.clone(),
        }
    }
}

/// View data for pre-filling a template form from a game server.
#[derive(Debug, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ServerTemplateView {
    pub name: String,
    pub app_id: i32,
    pub server_mod: Option<String>,
    pub beta_branch: Option<String>,
    pub boot_script: Option<String>,
    pub use_steam_login: bool,
    pub auto_start: bool,
    pub auto_restart: bool,
    pub auto_update: bool,
    pub update_on_start: bool,
    pub restart_schedule: Option<String>,
}

/// Render the template detail page.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::unused_async)]
pub fn show(
    v: &impl ViewRenderer,
    template: &TemplateModel,
    export_str: &str,
    error: Option<&str>,
    form_data: Option<&crate::models::game_templates::Model>,
) -> Result<impl IntoResponse> {
    let view = TemplateView::new(template);
    format::render().view(
        v,
        "game_templates/show.html",
        data!({
            "template": view,
            "export_string": export_str,
            "error": error,
            "form_data": form_data,
        }),
    )
}

/// Render the edit template form.
///
/// # Errors
/// Returns an error if template rendering fails.
pub fn edit_form(v: &impl ViewRenderer, template: &TemplateModel) -> Result<impl IntoResponse> {
    let view = TemplateView::new(template);
    format::render().view(v, "game_templates/edit.html", data!({ "template": view }))
}

/// Render the import form.
///
/// # Errors
/// Returns an error if template rendering fails.
pub fn import_form(
    v: &impl ViewRenderer,
    error: Option<&str>,
    import_result: Option<&TemplateImport>,
) -> Result<impl IntoResponse> {
    format::render().view(
        v,
        "game_templates/import.html",
        data!({
            "error": error,
            "import_result": import_result,
        }),
    )
}
