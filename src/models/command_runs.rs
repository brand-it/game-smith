use super::_entities::command_runs::Column;
pub use super::_entities::command_runs::{ActiveModel, Entity, Model};
use loco_rs::app::AppContext;
use loco_rs::model::ModelError;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue, QueryOrder, QuerySelect};
use std::collections::HashMap;
pub type CommandRuns = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert && self.updated_at.is_unchanged() {
            let mut this = self;
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
            Ok(this)
        } else {
            Ok(self)
        }
    }
}

/// Domain operations for creating and querying command run records.
impl ActiveModel {
    /// Create a new "running" command run record.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_run(
        ctx: &AppContext,
        command: String,
        args: Vec<String>,
        working_dir: Option<String>,
        env: Option<HashMap<String, String>>,
        log_path: Option<String>,
        title: Option<String>,
    ) -> Result<Model, ModelError> {
        let now = chrono::Utc::now();
        let args_json = serde_json::to_value(&args).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;
        let env_json = env
            .map(|h| serde_json::to_value(&h))
            .transpose()
            .map_err(|e| {
                ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            })?;
        let record = Self {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::Set(now.into()),
            updated_at: ActiveValue::Set(now.into()),
            command: ActiveValue::Set(command),
            args: ActiveValue::Set(args_json),
            working_dir: ActiveValue::Set(working_dir),
            env: ActiveValue::Set(env_json),
            log_path: ActiveValue::Set(log_path),
            status: ActiveValue::Set("running".to_string()),
            exit_code: ActiveValue::NotSet,
            started_at: ActiveValue::Set(now.naive_utc()),
            completed_at: ActiveValue::NotSet,
            server_id: ActiveValue::NotSet,
            log_removed: ActiveValue::Set(false),
            pid: ActiveValue::NotSet,
            title: ActiveValue::Set(title),
        };
        record.insert(&ctx.db).await.map_err(ModelError::from)
    }

    /// Mark a run as finished with an exit code and final status.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn finish(
        &mut self,
        ctx: &AppContext,
        exit_code: Option<i32>,
        status: String,
    ) -> Result<Model, ModelError> {
        self.status = ActiveValue::Set(status);
        self.exit_code = ActiveValue::Set(exit_code);
        self.completed_at = ActiveValue::Set(Some(chrono::Utc::now().naive_utc()));
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Mark the log file as removed (nullify path, set flag).
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn mark_log_removed(&mut self, ctx: &AppContext) -> Result<Model, ModelError> {
        self.log_path = ActiveValue::Set(None);
        self.log_removed = ActiveValue::Set(true);
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update the process ID for a running command.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_pid(&mut self, ctx: &AppContext, pid: i64) -> Result<Model, ModelError> {
        self.pid = ActiveValue::Set(Some(pid));
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }
}

/// Read-oriented helpers on persisted records.
impl Model {
    /// Check whether this run is still in "running" status.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    /// Find a command run by its primary key.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_by_id(ctx: &AppContext, id: i32) -> Result<Option<Self>, ModelError> {
        Entity::find_by_id(id)
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find a command run by its process ID.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_by_pid(ctx: &AppContext, pid: i64) -> Result<Option<Self>, ModelError> {
        Entity::find()
            .filter(Column::Pid.eq(pid))
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find all runs currently in "running" status.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_running(ctx: &AppContext) -> Result<Vec<Self>, ModelError> {
        Entity::find()
            .filter(Column::Status.eq("running"))
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find completed runs older than the given cutoff that still have log files.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_stale(
        ctx: &AppContext,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Self>, ModelError> {
        let cutoff_naive = cutoff.naive_utc();
        Entity::find()
            .filter(Column::Status.ne("running"))
            .filter(Column::StartedAt.lt(cutoff_naive))
            .filter(Column::LogPath.is_not_null())
            .filter(Column::LogRemoved.eq(false))
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find non-running runs that still have log files (for size-based checks).
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_with_log(ctx: &AppContext) -> Result<Vec<Self>, ModelError> {
        Entity::find()
            .filter(Column::Status.ne("running"))
            .filter(Column::LogPath.is_not_null())
            .filter(Column::LogRemoved.eq(false))
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// List the most recent command runs, ordered by creation time descending.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn list_recent(ctx: &AppContext, limit: u64) -> Result<Vec<Self>, ModelError> {
        Entity::find()
            .order_by_desc(Column::CreatedAt)
            .limit(limit)
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find the most recent `SteamCMD` health check run.
    ///
    /// Health check runs are identified by the `SteamCMD` binary path with
    /// a single `+quit` argument.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_last_health_check(ctx: &AppContext) -> Result<Option<Self>, ModelError> {
        let args_value = serde_json::to_value(vec!["+quit"]).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;
        Entity::find()
            .filter(
                Column::Command
                    .like("%steamcmd.sh")
                    .or(Column::Command.like("%steamcmd.exe")),
            )
            .filter(Column::Args.eq(args_value))
            .order_by_desc(Column::CreatedAt)
            .limit(1)
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find the most recent `SteamCMD` installation run.
    ///
    /// Installation runs are identified by `command == "steamcmd_install"`.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_last_install(ctx: &AppContext) -> Result<Option<Self>, ModelError> {
        Entity::find()
            .filter(Column::Command.eq("steamcmd_install"))
            .order_by_desc(Column::CreatedAt)
            .limit(1)
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)
    }
}

impl Entity {}
