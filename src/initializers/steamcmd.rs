use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Initializer},
    bgworker::BackgroundWorker,
    environment::Environment,
    Result,
};
use tracing::{info, warn};

use crate::data::steamcmd::{set_shared_store, SteamCmd, SteamCmdHealthStatus};
use crate::models::command_runs::ActiveModel as CommandRunActiveModel;
use crate::workers::command_exec::{CommandExecWorker, CommandExecWorkerArgs};
use crate::{resolve_data_home, AppDirs};

/// Initializer that checks `SteamCMD` health at application boot.
///
/// Runs a non-blocking health check that verifies the binary exists AND can
/// execute. Stores the result in the shared store so controllers and templates
/// can read the status. Also creates a `CommandRun` record for auditability.
#[allow(clippy::module_name_repetitions)]
pub struct SteamCmdInstaller;

#[async_trait]
impl Initializer for SteamCmdInstaller {
    fn name(&self) -> String {
        "steamcmd-check".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        // Skip health check in test mode to avoid polluting test databases
        if matches!(ctx.environment, Environment::Test) {
            return Ok(());
        }
        set_shared_store(&ctx.shared_store);
        ctx.shared_store.insert(SteamCmdHealthStatus::Checking);

        let ctx = ctx.clone();

        tokio::spawn(async move {
            let data_home = resolve_data_home();
            let dirs = AppDirs::new(data_home);
            let steamcmd = SteamCmd::new(&dirs);

            // Create log file for the health check
            let log_path = steamcmd.steamcmd_dir().join("health_check.log");
            let _ = std::fs::create_dir_all(steamcmd.steamcmd_dir());
            let _ = std::fs::File::create(&log_path);
            let log_path_str = Some(log_path.to_string_lossy().to_string());

            // Create a CommandRun with the steamcmd binary as the command
            let run = CommandRunActiveModel::create_run(
                &ctx,
                steamcmd.binary_path().to_string_lossy().to_string(),
                vec!["+quit".to_string()],
                Some(steamcmd.steamcmd_dir().to_string_lossy().to_string()),
                None,
                log_path_str,
                Some("SteamCMD Health Check".to_string()),
            )
            .await;

            match run {
                Ok(model) => {
                    info!(
                        run_id = model.id,
                        path = %steamcmd.binary_path().display(),
                        "dispatching SteamCMD health check"
                    );

                    // Dispatch to CommandExecWorker
                    let args = CommandExecWorkerArgs { run_id: model.id };
                    if let Err(e) = CommandExecWorker::perform_later(&ctx, args).await {
                        warn!(error = %e, "failed to dispatch health check worker");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "failed to create health check record");
                }
            }
        });

        Ok(())
    }
}
