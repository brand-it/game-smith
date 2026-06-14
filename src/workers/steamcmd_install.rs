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

        // Fetch the model for logging and DB updates
        let model = match CommandRunModel::find_by_id(&self.ctx, run_id).await {
            Ok(Some(m)) => m,
            Ok(None) => {
                error!(run_id, "SteamCMD install run not found in DB");
                return;
            }
            Err(e) => {
                error!(run_id, error = %e, "failed to fetch SteamCMD install run from DB");
                return;
            }
        };

        // Write progress to log file
        model.log_write("Starting SteamCMD installation...").await;

        let mut steamcmd = SteamCmd::new(&dirs).with_command_run(model);
        let result = steamcmd.install().await;

        match result {
            Ok(()) => {
                info!("SteamCMD installed successfully");

                // Update health status in shared store
                set_shared_store(&self.ctx.shared_store);
                self.ctx.shared_store.insert(SteamCmdHealthStatus::Healthy);

                // Reclaim model from steamcmd to mark run as completed
                if let Some(m) = steamcmd.take_model() {
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

                if let Some(m) = steamcmd.model() {
                    m.log_write(&format!("Installation failed: {e}")).await;
                }

                // Reclaim model from steamcmd to mark run as failed
                if let Some(m) = steamcmd.take_model() {
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
