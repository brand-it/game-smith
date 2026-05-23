use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Local;
use loco_rs::app::AppContext;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::model::ModelError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::command_runs::{ActiveModel, Model as CommandRunModel};
use crate::workers::command_exec::{CommandExecWorker, CommandExecWorkerArgs};

/// Represents a command execution result with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRun {
    pub id: i32,
    pub command: String,
    pub args: Vec<String>,
    pub log_path: Option<String>,
    pub status: String,
    pub exit_code: Option<i32>,
}

impl CommandRun {
    /// Create a [`CommandRun`] from a database [`CommandRunModel`].
    #[must_use]
    pub fn from_model(model: &CommandRunModel) -> Self {
        let args: Vec<String> = model
            .args
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            id: model.id,
            command: model.command.clone(),
            args,
            log_path: model.log_path.clone(),
            status: model.status.clone(),
            exit_code: model.exit_code,
        }
    }
}

/// High-level API for executing external commands with tracking and logging.
pub struct CommandRunner {
    ctx: AppContext,
    logs_dir: PathBuf,
}

impl CommandRunner {
    /// Create a new [`CommandRunner`] with the given application context.
    ///
    /// The log directory is derived from the XDG data home path.
    #[must_use]
    pub fn new(ctx: &AppContext) -> Self {
        let data_home = crate::resolve_data_home();
        let logs_dir = PathBuf::from(&data_home)
            .join("game-smith")
            .join("logs")
            .join("commands");
        Self {
            ctx: ctx.clone(),
            logs_dir,
        }
    }

    /// Execute an external command.
    ///
    /// Creates a log file, inserts a database record, and dispatches a
    /// [`CommandExecWorker`] to run the process asynchronously.
    ///
    /// Returns a [`CommandRun`] immediately (the run is in "running" status).
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the log directory cannot be created or the
    /// database insert fails.
    pub async fn execute(
        &self,
        command: String,
        args: Vec<String>,
        working_dir: Option<String>,
        env: Option<HashMap<String, String>>,
    ) -> Result<CommandRun, ModelError> {
        // Create log file path: logs_dir/YYYY-MM-DD/{uuid}.log
        let date_dir = Local::now().format("%Y-%m-%d").to_string();
        let uuid = Uuid::new_v4().to_string();
        let log_dir = self.logs_dir.join(&date_dir);
        std::fs::create_dir_all(&log_dir).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;
        let log_path = log_dir.join(format!("{uuid}.log"));
        let log_path_str = Some(log_path.to_string_lossy().to_string());

        // Create empty log file
        std::fs::File::create(&log_path).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;

        // Insert database record
        let model =
            ActiveModel::create_run(&self.ctx, command, args, working_dir, env, log_path_str)
                .await?;

        // Dispatch worker
        let worker_args = CommandExecWorkerArgs { run_id: model.id };
        CommandExecWorker::perform_later(&self.ctx, worker_args)
            .await
            .map_err(|e| {
                ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            })?;

        Ok(CommandRun::from_model(&model))
    }

    /// Read the tail of a command's log file.
    ///
    /// If `since_bytes` is provided, only reads content after that byte offset.
    ///
    /// # Errors
    /// Returns [`ModelError::EntityNotFound`] if the run does not exist, or a
    /// generic [`ModelError`] if the log file cannot be read.
    pub async fn tail(&self, id: i32, since_bytes: Option<u64>) -> Result<String, ModelError> {
        use std::io::{Read, Seek, SeekFrom};

        let model = CommandRunModel::find_by_id(&self.ctx, id)
            .await?
            .ok_or(ModelError::EntityNotFound)?;

        let log_path = match &model.log_path {
            Some(path) => PathBuf::from(path),
            None => return Ok(String::new()),
        };

        if !log_path.exists() {
            return Ok(String::new());
        }

        let mut file = std::fs::File::open(&log_path).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;

        if let Some(offset) = since_bytes {
            file.seek(SeekFrom::Start(offset)).map_err(|e| {
                ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            })?;
        }

        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|e| {
            ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })?;

        Ok(content)
    }

    /// Check whether a command run is still executing.
    ///
    /// Returns true only if the DB status is "running" AND the process
    /// (if a PID was recorded) is still alive on the system.
    ///
    /// # Errors
    /// Returns [`ModelError::EntityNotFound`] if the run does not exist.
    pub async fn is_running(&self, id: i32) -> Result<bool, ModelError> {
        let model = CommandRunModel::find_by_id(&self.ctx, id)
            .await?
            .ok_or(ModelError::EntityNotFound)?;

        if !model.is_running() {
            return Ok(false);
        }

        // If we have a PID, verify the process is actually alive
        if let Some(pid) = model.pid {
            // Send signal 0 to check if process exists (no actual signal sent)
            #[allow(clippy::cast_possible_truncation)]
            let alive = unsafe { libc::kill(pid as libc::c_int, 0) == 0 };
            return Ok(alive);
        }

        // No PID recorded — trust the DB status
        Ok(true)
    }

    /// Stop a running command.
    ///
    /// Sends SIGTERM to the child process if a PID is available, then marks
    /// the run as "stopped" in the database.
    ///
    /// # Errors
    /// Returns [`ModelError::EntityNotFound`] if the run does not exist.
    pub async fn stop(&self, id: i32) -> Result<(), ModelError> {
        let model = CommandRunModel::find_by_id(&self.ctx, id)
            .await?
            .ok_or(ModelError::EntityNotFound)?;

        if !model.is_running() {
            return Ok(());
        }

        // Try to terminate the process if we have a PID
        if let Some(pid) = model.pid {
            #[allow(clippy::cast_possible_truncation)]
            let _ = unsafe { libc::kill(pid as libc::c_int, libc::SIGTERM) };
        }

        // Mark as stopped in DB
        let mut active: crate::models::command_runs::ActiveModel = model.into();
        active
            .finish(&self.ctx, Some(-1), "stopped".to_string())
            .await?;

        Ok(())
    }
}
