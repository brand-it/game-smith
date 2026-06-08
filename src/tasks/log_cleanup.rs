use loco_rs::prelude::*;
use tracing::info;

use crate::workers::log_cleanup::{LogCleanupWorker, LogCleanupWorkerArgs};

pub struct LogCleanup;

#[async_trait]
impl Task for LogCleanup {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "log_cleanup".to_string(),
            detail: "Truncate oversized logs, remove stale logs, vacuum missing log records"
                .to_string(),
        }
    }

    async fn run(&self, app_context: &AppContext, _vars: &task::Vars) -> Result<()> {
        info!("log_cleanup: starting log cleanup");
        let worker = LogCleanupWorker::build(app_context);
        worker.perform(LogCleanupWorkerArgs).await?;
        info!("log_cleanup: finished");
        Ok(())
    }
}
