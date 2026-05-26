//! Steam credentials model with encrypted password storage.
//!
//! Single-row table — only one set of Steam credentials at a time.

pub use super::_entities::steam_credentials::{ActiveModel, Entity, Model};
use loco_rs::app::AppContext;
use loco_rs::model::ModelError;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue};

/// Current encryption version for stored credentials.
pub const ENC_VERSION: i32 = 1;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert && self.updated_at.is_unchanged() {
            let mut this = self;
            this.updated_at = ActiveValue::Set(chrono::Utc::now().into());
            Ok(this)
        } else {
            Ok(self)
        }
    }
}

/// Domain operations for managing Steam credentials.
impl ActiveModel {
    /// Store or update Steam credentials.
    ///
    /// Encrypts the password and upserts the row. If a row already exists,
    /// it is replaced (single-row table semantics).
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn store(
        ctx: &AppContext,
        username: String,
        nonce: Vec<u8>,
        ciphertext: Vec<u8>,
    ) -> Result<Model, ModelError> {
        let now = chrono::Utc::now();

        // Try to find existing row first
        let existing = Entity::find()
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)?;

        if let Some(record) = existing {
            // Update existing row
            let mut active: Self = record.into();
            active.username = ActiveValue::Set(username);
            active.nonce = ActiveValue::Set(nonce);
            active.ciphertext = ActiveValue::Set(ciphertext);
            active.enc_version = ActiveValue::Set(ENC_VERSION);
            active.updated_at = ActiveValue::Set(now.into());
            active.update(&ctx.db).await.map_err(ModelError::from)
        } else {
            // Insert new row
            let record = Self {
                id: ActiveValue::NotSet,
                created_at: ActiveValue::Set(now.into()),
                updated_at: ActiveValue::Set(now.into()),
                username: ActiveValue::Set(username),
                enc_version: ActiveValue::Set(ENC_VERSION),
                nonce: ActiveValue::Set(nonce),
                ciphertext: ActiveValue::Set(ciphertext),
            };
            record.insert(&ctx.db).await.map_err(ModelError::from)
        }
    }
}

/// Read-oriented helpers on persisted records.
impl Model {
    /// Find the single stored credential record.
    ///
    /// Returns `None` if no credentials are configured.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database query fails.
    pub async fn find(ctx: &AppContext) -> Result<Option<Self>, ModelError> {
        Entity::find().one(&ctx.db).await.map_err(ModelError::from)
    }

    /// Check whether Steam credentials have been configured.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database query fails.
    pub async fn is_configured(ctx: &AppContext) -> Result<bool, ModelError> {
        Ok(Self::find(ctx).await?.is_some())
    }

    /// Delete the stored credential record.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn delete(&self, ctx: &AppContext) -> Result<(), ModelError> {
        use sea_orm::Delete;

        let active: ActiveModel = self.clone().into();
        Delete::one(active)
            .exec(&ctx.db)
            .await
            .map_err(ModelError::from)?;
        Ok(())
    }
}

impl Entity {}
