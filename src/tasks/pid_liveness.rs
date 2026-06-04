use loco_rs::prelude::*;
use tracing::{error, info};

use crate::models::command_runs;
use crate::models::command_runs::CommandStatus;
use crate::models::game_servers;

pub struct PidLiveness;

#[async_trait]
impl Task for PidLiveness {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "pid_liveness".to_string(),
            detail: "Check command run PID liveness and correct stale statuses".to_string(),
        }
    }

    async fn run(&self, app_context: &AppContext, _vars: &task::Vars) -> Result<()> {
        info!("pid_liveness: checking running command runs");
        let running = command_runs::Model::find_running(app_context)
            .await
            .map_err(|e| {
                loco_rs::Error::string(&format!("failed to query running command runs: {e}"))
            })?;
        info!(
            count = running.len(),
            "pid_liveness: found running command runs, checking PIDs"
        );

        for run in running {
            let should_mark_failed = run
                .pid
                .is_none_or(|pid| !game_servers::check_pid_alive(pid));

            if should_mark_failed {
                mark_dead(app_context, run).await;
            }
        }

        Ok(())
    }
}

/// Mark a command run as failed because the process is no longer alive.
async fn mark_dead(app_context: &AppContext, run: command_runs::Model) {
    let pid_label = run
        .pid
        .map_or_else(|| "no PID".to_string(), |pid| format!("dead PID {pid}"));
    info!(run_id = run.id, %pid_label, "command run process is dead, marking as failed");
    let mut active: command_runs::ActiveModel = run.into();
    match active
        .finish(app_context, None, CommandStatus::Failed)
        .await
    {
        Ok(_) => {
            info!(run_id = active.id.unwrap(), "marked command run as failed");
        }
        Err(e) => error!(run_id = active.id.unwrap(), error = %e,
            "failed to update command run status to failed"),
    }
}
