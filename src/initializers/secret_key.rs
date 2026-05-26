//! Generates the local secret key file at boot if it does not exist.
//!
//! The key is stored at `$DATA_HOME/game-smith/secret.key` and is used for
//! encrypting sensitive values at rest (e.g., Steam credentials).

use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Initializer},
    environment::Environment,
    Result,
};
use tracing::info;

use crate::data::encryption::generate_secret_key;
use crate::{resolve_data_home, AppDirs};

/// Initializer that ensures the secret key file exists at boot.
///
/// Generates a random 32-byte key if the file is missing.
/// Skips silently in test mode (tests use in-memory DB and ephemeral state).
pub struct SecretKeyInitializer;

#[async_trait]
impl Initializer for SecretKeyInitializer {
    fn name(&self) -> String {
        "secret-key-init".to_string()
    }

    async fn before_run(&self, _ctx: &AppContext) -> Result<()> {
        // Skip in test mode — tests don't need persistent encryption keys
        if matches!(_ctx.environment, Environment::Test) {
            return Ok(());
        }

        let data_home = resolve_data_home();
        let dirs = AppDirs::new(data_home);
        let key_path = dirs.app_dir.join("secret.key");

        if !key_path.exists() {
            match generate_secret_key(&key_path) {
                Ok(path) => {
                    info!(path = %path.display(), "generated new secret key");
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to generate secret key");
                }
            }
        }

        Ok(())
    }
}
