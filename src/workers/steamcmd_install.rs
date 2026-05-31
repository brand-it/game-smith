use async_trait::async_trait;
use loco_rs::app::AppContext;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::Result;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::data::steamcmd::{set_shared_store, SteamCmd, SteamCmdHealthStatus};
use crate::models::command_runs::{CommandStatus, Model as CommandRunModel};
use crate::{resolve_data_home, AppDirs};

/// Worker that performs the `SteamCMD` installation (download + extract).
///
/// This is Rust-based logic, not a shell command, so it cannot be handled
/// by [`CommandExecWorker`].
pub struct SteamCmdInstallWorker {
    pub ctx: AppContext,
}

/// Arguments for [`SteamCmdInstallWorker`].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SteamCmdInstallWorkerArgs {
    pub run_id: i32,
}

impl SteamCmdInstallWorker {
    /// Run the installation and update the database record with the result.
    async fn run_install(&self, run_id: i32) {
        let data_home = resolve_data_home();
        let dirs = AppDirs::new(data_home);
        let steamcmd = SteamCmd::new(&dirs);

        // Write progress to log file if available
        if let Ok(Some(model)) = CommandRunModel::find_by_id(&self.ctx, run_id).await {
            if let Some(ref log_path) = model.log_path {
                if let Err(e) =
                    tokio::fs::write(log_path, "Starting SteamCMD installation...\n").await
                {
                    tracing::warn!(run_id, log_path, error = %e, "failed to write install start marker to log");
                }
            }
        }

        let result = steamcmd.install().await;

        match result {
            Ok(()) => {
                info!("SteamCMD installed successfully");

                // Update health status in shared store
                set_shared_store(&self.ctx.shared_store);
                self.ctx.shared_store.insert(SteamCmdHealthStatus::Healthy);

                // Mark run as completed
                if let Ok(Some(m)) = CommandRunModel::find_by_id(&self.ctx, run_id).await {
                    let mut active: crate::models::command_runs::ActiveModel = m.into();
                    if let Err(e) = active
                        .finish(&self.ctx, Some(0), CommandStatus::Completed)
                        .await
                    {
                        tracing::warn!(run_id, error = %e, "failed to mark install run as completed in DB");
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "SteamCMD installation failed");

                // Write error to log file if available
                if let Ok(Some(model)) = CommandRunModel::find_by_id(&self.ctx, run_id).await {
                    if let Some(ref log_path) = model.log_path {
                        if let Err(e) =
                            tokio::fs::write(log_path, format!("Installation failed: {e}\n")).await
                        {
                            tracing::warn!(run_id, log_path, error = %e, "failed to write install failure to log");
                        }
                    }
                }

                // Mark run as failed
                if let Ok(Some(m)) = CommandRunModel::find_by_id(&self.ctx, run_id).await {
                    let mut active: crate::models::command_runs::ActiveModel = m.into();
                    if let Err(e) = active
                        .finish(&self.ctx, Some(1), CommandStatus::Failed)
                        .await
                    {
                        tracing::warn!(run_id, error = %e, "failed to mark install run as failed in DB");
                    }
                }
            }
        }
    }
}

#[async_trait]
impl BackgroundWorker<SteamCmdInstallWorkerArgs> for SteamCmdInstallWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn perform(&self, args: SteamCmdInstallWorkerArgs) -> Result<()> {
        let SteamCmdInstallWorkerArgs { run_id } = args;
        self.run_install(run_id).await;
        Ok(())
    }
}
