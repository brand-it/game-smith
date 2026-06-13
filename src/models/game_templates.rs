use std::collections::HashMap;

use loco_rs::app::AppContext;
use loco_rs::model::ModelError;
use loco_rs::validation::Validatable;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue, QueryOrder};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::_entities::game_templates as _entities;
pub use super::_entities::game_templates::{ActiveModel, Entity, Model};

/// Validation rules for [`ActiveModel`].
#[derive(Debug, Validate)]
pub struct GameTemplatesValidator {
    #[validate(length(
        min = 2,
        max = 100,
        message = "Name must be between 2 and 100 characters."
    ))]
    pub name: String,
    #[validate(range(min = 1, message = "App ID must be a positive integer."))]
    pub app_id: i32,
}

impl Validatable for ActiveModel {
    fn validator(&self) -> Box<dyn Validate> {
        Box::new(GameTemplatesValidator {
            name: self.name.as_ref().clone(),
            app_id: *self.app_id.as_ref(),
        })
    }
}

#[allow(clippy::struct_excessive_bools)]
/// JSON payload for the export/import wire format.
/// Versioned so we can evolve the format without breaking older exports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateExport {
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub app_id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_mod: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beta_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_schedule: Option<String>,
}

impl TemplateExport {
    /// Serializes this template to a URL-safe base64 string (no padding).
    ///
    /// # Errors
    /// Returns a [`ModelError`] if JSON serialization fails.
    pub fn to_base64(&self) -> Result<String, ModelError> {
        let json = serde_json::to_string(self).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;
        Ok(data_encoding::BASE64URL_NOPAD.encode(json.as_bytes()))
    }

    /// Deserializes a URL-safe base64 string (no padding) into a [`TemplateExport`].
    ///
    /// # Errors
    /// Returns a [`ModelError`] if base64 decoding or JSON deserialization fails.
    pub fn from_base64(s: &str) -> Result<Self, ModelError> {
        let bytes = data_encoding::BASE64URL_NOPAD
            .decode(s.as_bytes())
            .map_err(|e| {
                ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            })?;
        let json = String::from_utf8(bytes).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;
        serde_json::from_str(&json)
            .map_err(|e| ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>))
    }
}

/// Build a [`TemplateExport`] from a persisted template record.
fn model_to_export(model: &Model) -> TemplateExport {
    TemplateExport {
        version: 1,
        name: model.name.clone(),
        description: model.description.clone(),
        app_id: model.app_id,
        server_mod: model.server_mod.clone(),
        beta_branch: model.beta_branch.clone(),
        boot_script: model.boot_script.clone(),
        use_steam_login: model.use_steam_login,
        auto_start: model.auto_start,
        auto_restart: model.auto_restart,
        auto_update: model.auto_update,
        update_on_start: model.update_on_start,
        restart_schedule: model.restart_schedule.clone(),
    }
}

/// Build a form-data map from a [`TemplateExport`] for use in HTML forms.
fn export_to_form_data(export: &TemplateExport) -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("name".into(), export.name.clone());
    map.insert("app_id".into(), export.app_id.to_string());
    if let Some(ref desc) = export.description {
        map.insert("description".into(), desc.clone());
    }
    if let Some(ref mod_name) = export.server_mod {
        map.insert("server_mod".into(), mod_name.clone());
    }
    if let Some(ref branch) = export.beta_branch {
        map.insert("beta_branch".into(), branch.clone());
    }
    if let Some(ref script) = export.boot_script {
        map.insert("boot_script".into(), script.clone());
    }
    map.insert("use_steam_login".into(), export.use_steam_login.to_string());
    map.insert("auto_start".into(), export.auto_start.to_string());
    map.insert("auto_restart".into(), export.auto_restart.to_string());
    map.insert("auto_update".into(), export.auto_update.to_string());
    map.insert("update_on_start".into(), export.update_on_start.to_string());
    if let Some(ref schedule) = export.restart_schedule {
        map.insert("restart_schedule".into(), schedule.clone());
    }
    map
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    #[allow(clippy::unused_async)]
    async fn before_save<C>(self, _ctx: &C, _inserted: bool) -> Result<Self, DbErr> {
        self.validate()?;
        Ok(self)
    }
}

/// Domain operations for creating and querying game template records.
impl ActiveModel {
    /// Create a new game template record.
    ///
    /// # Arguments
    /// * `ctx` - Application context with database connection.
    /// * `name` - Template display name.
    /// * `description` - Optional human-readable description.
    /// * `app_id` - Steam App ID.
    /// * `server_mod` - Optional mod name for HL1 games.
    /// * `beta_branch` - Optional beta branch name.
    /// * `boot_script` - Optional custom boot command.
    /// * `use_steam_login` - Whether to use Steam credentials.
    /// * `auto_start` - Whether to auto-start the server.
    /// * `auto_restart` - Whether to auto-restart on crash.
    /// * `auto_update` - Whether to auto-update the game.
    /// * `update_on_start` - Whether to update on server start.
    /// * `restart_schedule` - Optional cron schedule for restarts.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::fn_params_excessive_bools)]
    pub async fn create(
        ctx: &AppContext,
        name: String,
        description: Option<String>,
        app_id: i32,
        server_mod: Option<String>,
        beta_branch: Option<String>,
        boot_script: Option<String>,
        use_steam_login: bool,
        auto_start: bool,
        auto_restart: bool,
        auto_update: bool,
        update_on_start: bool,
        restart_schedule: Option<String>,
    ) -> Result<Model, ModelError> {
        // Enforce uniqueness of name
        if Model::find_by_name(ctx, &name).await?.is_some() {
            return Err(ModelError::EntityAlreadyExists);
        }

        let now = chrono::Utc::now();
        let record = Self {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::Set(now.into()),
            updated_at: ActiveValue::Set(now.into()),
            name: ActiveValue::Set(name),
            description: ActiveValue::Set(description),
            app_id: ActiveValue::Set(app_id),
            server_mod: ActiveValue::Set(server_mod),
            beta_branch: ActiveValue::Set(beta_branch),
            boot_script: ActiveValue::Set(boot_script),
            use_steam_login: ActiveValue::Set(use_steam_login),
            auto_start: ActiveValue::Set(auto_start),
            auto_restart: ActiveValue::Set(auto_restart),
            auto_update: ActiveValue::Set(auto_update),
            update_on_start: ActiveValue::Set(update_on_start),
            restart_schedule: ActiveValue::Set(restart_schedule),
        };

        record.insert(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update an existing template record from form data.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::fn_params_excessive_bools)]
    pub async fn update_from_form(
        &mut self,
        ctx: &AppContext,
        name: String,
        description: Option<String>,
        app_id: i32,
        server_mod: Option<String>,
        beta_branch: Option<String>,
        boot_script: Option<String>,
        use_steam_login: bool,
        auto_start: bool,
        auto_restart: bool,
        auto_update: bool,
        update_on_start: bool,
        restart_schedule: Option<String>,
    ) -> Result<Model, ModelError> {
        // Enforce uniqueness of name
        {
            let existing = Model::find_by_name(ctx, &name).await?;
            if existing.is_some_and(|e| e.id != *self.id.as_ref()) {
                return Err(ModelError::EntityAlreadyExists);
            }
        }

        self.name = ActiveValue::Set(name);
        self.description = ActiveValue::Set(description);
        self.app_id = ActiveValue::Set(app_id);
        self.server_mod = ActiveValue::Set(server_mod);
        self.beta_branch = ActiveValue::Set(beta_branch);
        self.boot_script = ActiveValue::Set(boot_script);
        self.use_steam_login = ActiveValue::Set(use_steam_login);
        self.auto_start = ActiveValue::Set(auto_start);
        self.auto_restart = ActiveValue::Set(auto_restart);
        self.auto_update = ActiveValue::Set(auto_update);
        self.update_on_start = ActiveValue::Set(update_on_start);
        self.restart_schedule = ActiveValue::Set(restart_schedule);
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }
}

/// Read-oriented helpers on persisted records.
impl Model {
    /// Find a template by its primary key.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_by_id(ctx: &AppContext, id: i32) -> Result<Option<Self>, ModelError> {
        Entity::find_by_id(id)
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find a template by its display name.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_by_name(ctx: &AppContext, name: &str) -> Result<Option<Self>, ModelError> {
        Entity::find()
            .filter(_entities::Column::Name.eq(name))
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// List all templates, ordered by creation time descending.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn list(ctx: &AppContext) -> Result<Vec<Self>, ModelError> {
        use _entities::Column;
        Entity::find()
            .order_by_desc(Column::CreatedAt)
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Export this template as a URL-safe base64 string.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if serialization fails.
    pub fn export(&self) -> Result<String, ModelError> {
        model_to_export(self).to_base64()
    }

    /// Import a template from a URL-safe base64 string.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if decoding or deserialization fails.
    pub fn import(_ctx: &AppContext, s: &str) -> Result<TemplateImport, ModelError> {
        let export = TemplateExport::from_base64(s)?;
        Ok(TemplateImport {
            form_data: export_to_form_data(&export),
            export,
        })
    }
}

/// Result of a successful template import, ready for preview in the UI.
#[derive(Debug, Serialize)]
pub struct TemplateImport {
    /// Parsed export object.
    pub export: TemplateExport,
    /// Form data map suitable for pre-populating a form.
    pub form_data: HashMap<String, String>,
}
