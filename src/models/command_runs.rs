use super::_entities::command_runs::Column;
pub use super::_entities::command_runs::{ActiveModel, Entity, Model};
use loco_rs::app::AppContext;
use loco_rs::model::ModelError;
use loco_rs::validation::Validatable;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

/// Validation rules for [`ActiveModel`].
#[derive(Debug, Validate)]
pub struct CommandRunsValidator {
    #[validate(length(min = 1, message = "Command must not be empty."))]
    pub command: String,
}

impl Validatable for ActiveModel {
    fn validator(&self) -> Box<dyn Validate> {
        Box::new(CommandRunsValidator {
            command: self.command.as_ref().clone(),
        })
    }
}

/// Command run view wrapper that adds computed `is_running` for templates.
///
/// Serde serializes only struct fields — `is_running` is a method on
/// [`Model`] and won't appear in serialized output. This wrapper
/// adds `is_running` as a real field so Tera templates can use it in conditionals.
#[derive(Debug, Serialize)]
pub struct CommandRunView<'a> {
    /// True when this run is still in "running" status.
    pub is_running: bool,

    #[serde(flatten)]
    inner: &'a Model,
}

impl<'a> CommandRunView<'a> {
    #[must_use]
    pub fn new(run: &'a Model) -> Self {
        Self {
            is_running: run.is_running(),
            inner: run,
        }
    }
}

impl std::ops::Deref for CommandRunView<'_> {
    type Target = Model;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

/// Possible values for the `command_runs.status` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandStatus {
    Running,
    Completed,
    Failed,
}

impl CommandStatus {
    /// Returns the canonical lowercase database representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl std::str::FromStr for CommandStatus {
    type Err = std::convert::Infallible;

    /// Parse a database string into a [`CommandStatus`].
    /// Unknown values default to [`CommandStatus::Running`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Running,
        })
    }
}

impl AsRef<str> for CommandStatus {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for CommandStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        self.validate()?;
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
        server_id: Option<i64>,
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
            status: ActiveValue::Set(CommandStatus::Running.as_str().to_string()),
            exit_code: ActiveValue::NotSet,
            started_at: ActiveValue::Set(now.naive_utc()),
            completed_at: ActiveValue::NotSet,
            server_id: ActiveValue::Set(server_id),
            log_removed: ActiveValue::Set(false),
            pid: ActiveValue::Set(Some(i64::from(std::process::id()))),
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
        status: CommandStatus,
    ) -> Result<Model, ModelError> {
        self.status = ActiveValue::Set(status.as_str().to_string());
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
        self.status() == CommandStatus::Running
    }

    /// Append a line to the command run's log file.
    ///
    /// If `log_path` is `None`, or if the write fails, the method logs
    /// a warning and returns silently — callers do not need to handle errors.
    pub async fn log_write(&self, message: &str) {
        let Some(ref log_path) = self.log_path else {
            return;
        };
        if let Err(e) = tokio::fs::write(log_path, format!("{message}\n")).await {
            tracing::warn!(
                id = self.id,
                log_path,
                error = %e,
                "failed to append to command log"
            );
        }
    }

    /// Returns the DB status as a typed [`CommandStatus`].
    #[must_use]
    pub fn status(&self) -> CommandStatus {
        self.status.parse().unwrap_or(CommandStatus::Running)
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

    /// Find all runs currently in "running" status for a specific server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database query fails.
    pub async fn find_running_by_server(
        ctx: &AppContext,
        server_id: i64,
    ) -> Result<Vec<Self>, ModelError> {
        Entity::find()
            .filter(Column::Status.eq(CommandStatus::Running.as_str()))
            .filter(Column::ServerId.eq(server_id))
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find all command runs for a specific server (regardless of status).
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database query fails.
    pub async fn find_by_server(ctx: &AppContext, server_id: i64) -> Result<Vec<Self>, ModelError> {
        Entity::find()
            .filter(Column::ServerId.eq(server_id))
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find the most recent command run for a specific server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database query fails.
    pub async fn find_latest_by_server(
        ctx: &AppContext,
        server_id: i64,
    ) -> Result<Option<Self>, ModelError> {
        Entity::find()
            .filter(Column::ServerId.eq(server_id))
            .order_by_desc(Column::CreatedAt)
            .limit(1)
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
            .filter(Column::Status.eq(CommandStatus::Running.as_str()))
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
            .filter(Column::Status.ne(CommandStatus::Running.as_str()))
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
            .filter(Column::Status.ne(CommandStatus::Running.as_str()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_run_view_exposes_is_running_true() {
        let model = Model {
            id: 1,
            command: "test".to_string(),
            args: serde_json::Value::Array(vec![]),
            working_dir: None,
            log_path: None,
            env: None,
            status: CommandStatus::Running.as_str().to_string(),
            exit_code: None,
            started_at: chrono::Utc::now().naive_utc(),
            completed_at: None,
            server_id: None,
            log_removed: false,
            pid: None,
            title: None,
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };
        let view = CommandRunView::new(&model);
        assert!(view.is_running);
    }

    #[test]
    fn command_run_view_exposes_is_running_false() {
        let model = Model {
            id: 2,
            command: "test".to_string(),
            args: serde_json::Value::Array(vec![]),
            working_dir: None,
            log_path: None,
            env: None,
            status: CommandStatus::Completed.as_str().to_string(),
            exit_code: Some(0),
            started_at: chrono::Utc::now().naive_utc(),
            completed_at: Some(chrono::Utc::now().naive_utc()),
            server_id: None,
            log_removed: false,
            pid: None,
            title: None,
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };
        let view = CommandRunView::new(&model);
        assert!(!view.is_running);
    }

    #[test]
    fn command_run_view_flattens_model_fields() {
        let model = Model {
            id: 42,
            command: "steamcmd_install".to_string(),
            args: serde_json::Value::Array(vec![serde_json::Value::String("+quit".to_string())]),
            working_dir: Some("/tmp".to_string()),
            log_path: None,
            env: None,
            status: CommandStatus::Running.as_str().to_string(),
            exit_code: None,
            started_at: chrono::Utc::now().naive_utc(),
            completed_at: None,
            server_id: Some(1),
            log_removed: false,
            pid: Some(1234),
            title: Some("Install".to_string()),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
        };
        let view = CommandRunView::new(&model);
        assert_eq!(view.id, 42);
        assert_eq!(view.command, "steamcmd_install");
        assert_eq!(view.server_id, Some(1));
        assert_eq!(view.pid, Some(1234));
        assert_eq!(view.title, Some("Install".to_string()));
    }
}
