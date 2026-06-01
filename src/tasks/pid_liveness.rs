use loco_rs::prelude::*;
use tracing::info;

use crate::data::game_server_installer::GameServerInstaller;
use crate::models::command_runs;
use crate::models::command_runs::CommandStatus;
use crate::models::game_servers;
use crate::models::game_servers::ServerStatus;

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
                mark_failed(app_context, run).await;
                continue;
            };

            if !game_servers::check_pid_alive(pid) {
                handle_dead_pid(app_context, run, pid).await;
            }
        }

        Ok(())
    }
}

/// Mark a command run with no PID as failed.
async fn mark_failed(app_context: &AppContext, run: command_runs::Model) {
    info!(run_id = run.id, "command run has no PID, marking as failed");
    let mut active: command_runs::ActiveModel = run.into();
    match active
        .finish(app_context, None, CommandStatus::Failed)
        .await
    {
        Ok(_) => {
            info!(
                run_id = active.id.unwrap(),
                "marked null-pid command run as failed"
            );
        }
        Err(e) => info!(run_id = active.id.unwrap(), error = %e,
            "failed to update null-pid command run status to failed"),
    }
}

/// Handle a dead PID: mark as failed or auto-restart if configured.
async fn handle_dead_pid(app_context: &AppContext, run: command_runs::Model, pid: i64) {
    let should_restart = should_auto_restart(app_context, &run).await;

    if should_restart {
        let server_id = run.server_id.unwrap();
        info!(
            run_id = run.id,
            pid = pid,
            server_id = server_id,
            "command run process is dead, auto-restarting server"
        );

        let mut active: command_runs::ActiveModel = run.into();
        let _ = active
            .finish(app_context, None, CommandStatus::Failed)
            .await;

        restart_server(app_context, server_id).await;
    } else {
        info!(
            run_id = run.id,
            pid = pid,
            "command run process is dead, marking as failed"
        );
        let mut active: command_runs::ActiveModel = run.into();
        let _ = active
            .finish(app_context, None, CommandStatus::Failed)
            .await;
    }
}

/// Check if a dead command run should trigger auto-restart.
async fn should_auto_restart(app_context: &AppContext, run: &command_runs::Model) -> bool {
    let Some(server_id) = run.server_id else {
        return false;
    };
    match game_servers::Model::find_by_id(app_context, i32::try_from(server_id).unwrap_or(i32::MAX))
        .await
    {
        Ok(Some(server)) => server.auto_restart && server.status() == ServerStatus::Running,
        Ok(None) => false,
        Err(e) => {
            info!(run_id = run.id, error = %e, "failed to look up server for auto-restart check");
            false
        }
    }
}

/// Attempt to restart a server after a crash.
async fn restart_server(app_context: &AppContext, server_id: i64) {
    let Ok(Some(server)) =
        game_servers::Model::find_by_id(app_context, i32::try_from(server_id).unwrap_or(i32::MAX))
            .await
    else {
        return;
    };

    let installer = GameServerInstaller::new(app_context);
    match installer.start(&server).await {
        Ok(Some(new_run)) => {
            info!(
                run_id = new_run.id,
                server_id = server.id,
                "auto-restarted server successfully"
            );
        }
        Ok(None) => {
            info!(
                server_id = server.id,
                "auto-restart: no server executable found"
            );
        }
        Err(e) => {
            info!(server_id = server.id, error = %e, "failed to auto-restart server");
        }
    }
}
