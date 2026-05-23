use async_trait::async_trait;
use loco_rs::app::AppContext;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::process::Command;

use crate::models::command_runs::Model as CommandRunModel;

pub struct CommandExecWorker {
    pub ctx: AppContext,
}

impl CommandExecWorker {
    /// Load the run record and verify it is still in "running" state.
    async fn fetch_and_validate(&self, run_id: i32) -> Result<CommandRunModel> {
        let model = CommandRunModel::find_by_id(&self.ctx, run_id)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to find run: {e}")))?
            .ok_or_else(|| loco_rs::Error::string("run not found"))?;

        if !model.is_running() {
            tracing::info!(run_id, "run already stopped, skipping execution");
            return Err(loco_rs::Error::string("run already stopped"));
        }

        Ok(model)
    }

    /// Open the log file for writing (or /dev/null if no log path).
    fn open_log_file(log_path: Option<&str>) -> Result<std::fs::File> {
        let path = log_path.unwrap_or("/dev/null");
        std::fs::File::create(path)
            .map_err(|e| loco_rs::Error::string(&format!("failed to open log file: {e}")))
    }

    /// Build a tokio `Command` from the persisted run model.
    fn build_command(
        model: &CommandRunModel,
        stdout_file: std::fs::File,
        stderr_file: std::fs::File,
    ) -> Command {
        let cmd_args: Vec<String> = model
            .args
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let mut cmd = Command::new(&model.command);
        cmd.args(&cmd_args);
        cmd.stdout(std::process::Stdio::from(stdout_file));
        cmd.stderr(std::process::Stdio::from(stderr_file));

        if let Some(dir) = &model.working_dir {
            cmd.current_dir(dir);
        }

        if let Some(env_json) = &model.env {
            if let Ok(variables) =
                serde_json::from_value::<HashMap<String, String>>(env_json.clone())
            {
                for (key, value) in variables {
                    cmd.env(key, value);
                }
            }
        }

        cmd
    }

    /// Mark a run as failed in the database (best-effort, fire-and-forget).
    async fn mark_spawn_failed(ctx: &AppContext, run_id: i32) {
        if let Ok(Some(m)) = CommandRunModel::find_by_id(ctx, run_id).await {
            let mut active: crate::models::command_runs::ActiveModel = m.into();
            let _ = active.finish(ctx, Some(-1), "failed".to_string()).await;
        }
    }

    /// Store the child process PID in the database (best-effort).
    async fn store_pid(&self, run_id: i32, pid: u32) {
        if let Ok(Some(m)) = CommandRunModel::find_by_id(&self.ctx, run_id).await {
            let mut active: crate::models::command_runs::ActiveModel = m.into();
            let _ = active.update_pid(&self.ctx, i64::from(pid)).await;
        }
    }

    /// Determine the final status and exit code from a process exit status.
    fn determine_status(status: std::process::ExitStatus) -> (String, Option<i32>) {
        if status.success() {
            ("completed".to_string(), Some(0))
        } else if let Some(code) = status.code() {
            ("failed".to_string(), Some(code))
        } else {
            ("failed".to_string(), None)
        }
    }

    /// Write the final status back to the database.
    async fn update_final_status(
        &self,
        run_id: i32,
        exit_code: Option<i32>,
        final_status: String,
    ) -> Result<()> {
        let model = CommandRunModel::find_by_id(&self.ctx, run_id)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to find run: {e}")))?
            .ok_or_else(|| loco_rs::Error::string("run not found"))?;
        let mut active: crate::models::command_runs::ActiveModel = model.into();
        active
            .finish(&self.ctx, exit_code, final_status)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to update run status: {e}")))?;
        Ok(())
    }
}

/// Arguments for executing a command via [`CommandExecWorker`].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandExecWorkerArgs {
    pub run_id: i32,
}

#[async_trait]
impl BackgroundWorker<CommandExecWorkerArgs> for CommandExecWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    #[allow(clippy::let_underscore_future)]
    async fn perform(&self, args: CommandExecWorkerArgs) -> Result<()> {
        let CommandExecWorkerArgs { run_id } = args;

        // 1. Validate run exists and is still active
        let Ok(model) = self.fetch_and_validate(run_id).await else {
            return Ok(());
        };

        // 2. Open log file for stdout/stderr
        let log_path = model.log_path.as_deref();
        let stdout_file = Self::open_log_file(log_path)?;
        let stderr_file = stdout_file
            .try_clone()
            .map_err(|e| loco_rs::Error::string(&format!("failed to clone log file: {e}")))?;

        // 3. Build the command
        let mut cmd = Self::build_command(&model, stdout_file, stderr_file);

        // 4. Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            let ctx = self.ctx.clone();
            let rid = run_id;
            let _ = async move { Self::mark_spawn_failed(&ctx, rid).await };
            loco_rs::Error::string(&format!("failed to spawn process: {e}"))
        })?;

        // 5. Store PID in database
        if let Some(pid) = child.id() {
            self.store_pid(run_id, pid).await;
        }

        // 6. Wait for completion
        let status = child
            .wait()
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to wait for process: {e}")))?;

        // 7. Determine and persist final status
        let (final_status, exit_code) = Self::determine_status(status);
        self.update_final_status(run_id, exit_code, final_status)
            .await?;

        Ok(())
    }
}
