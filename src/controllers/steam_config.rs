//! Steam credential configuration controller.
//!
//! Provides a web form for entering Steam username and password.
//! The password is encrypted before storage.

use crate::initializers::embedded_i18n::EmbeddedViews;
use axum::extract::Form;
use axum::routing::{get, post};
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::data::encryption::EncryptionKey;
use crate::models::steam_credentials;
use crate::{resolve_data_home, AppDirs};

/// Form data for configuring Steam credentials.
#[derive(Debug, Deserialize)]
pub struct SteamConfigForm {
    pub steam_username: String,
    pub steam_password: String,
}

/// GET /steam-config — show the Steam credential configuration form.
///
/// Pre-populates the username field if credentials are already configured.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if rendering fails.
pub async fn show_config(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    let username = steam_credentials::Model::find(&ctx)
        .await
        .ok()
        .flatten()
        .map(|record| record.username);

    crate::views::steam_config::config(&ctx, v, username.as_deref(), None)
}

/// POST /steam-config — save or update Steam credentials.
///
/// Validates input, encrypts the password, and upserts the credential record.
/// On success, redirects to the config page.
/// On validation error, re-renders the form with the error message.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if encryption fails or the database operation fails.
pub async fn save_config(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
    Form(form): Form<SteamConfigForm>,
) -> Result<impl IntoResponse> {
    let username = form.steam_username.trim().to_string();
    let password = form.steam_password.trim().to_string();

    // Fetch current username for pre-population on error
    let existing_username = steam_credentials::Model::find(&ctx)
        .await
        .ok()
        .flatten()
        .map(|record| record.username);

    if username.is_empty() {
        return crate::views::steam_config::config(
            &ctx,
            v,
            existing_username.as_deref(),
            Some("Steam username cannot be empty"),
        );
    }
    if password.is_empty() {
        return crate::views::steam_config::config(
            &ctx,
            v,
            existing_username.as_deref(),
            Some("Steam password cannot be empty"),
        );
    }

    // Load encryption key
    let data_home = resolve_data_home();
    let dirs = AppDirs::new(data_home);
    let key_path = dirs.app_dir.join("secret.key");

    let key = EncryptionKey::load(&key_path)
        .map_err(|e| loco_rs::Error::string(&format!("failed to load encryption key: {e}")))?;

    // Encrypt password
    let (nonce, ciphertext) = key
        .encrypt(&password)
        .map_err(|e| loco_rs::Error::string(&format!("failed to encrypt password: {e}")))?;

    // Store credentials
    let _record = steam_credentials::ActiveModel::store(&ctx, username.clone(), nonce, ciphertext)
        .await
        .map_err(|e| loco_rs::Error::string(&format!("failed to save steam credentials: {e}")))?;

    crate::views::steam_config::config(&ctx, v, Some(&username), None)
}

/// Register the steam config routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("steam-config")
        .add("/", get(show_config))
        .add("/", post(save_config))
}
