use loco_rs::prelude::*;
use tracing::info;

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
            let Some(pid) = run.pid else {
                continue;
            };

            if !game_servers::check_pid_alive(pid) {
                info!(
                    run_id = run.id,
                    pid = pid,
                    "command run process is dead, marking as failed"
                );

                // Mark the command run as failed
                let mut active: command_runs::ActiveModel = run.into();
                match active
                    .finish(app_context, None, CommandStatus::Failed)
                    .await
                {
                    Ok(_) => info!(run_id = active.id.unwrap(), "marked command run as failed"),
                    Err(e) => info!(run_id = active.id.unwrap(), error = %e,
                        "failed to update command run status to failed"),
                }
            }
        }

        Ok(())
    }
}
