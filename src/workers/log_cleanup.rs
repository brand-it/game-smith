use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};

use async_trait::async_trait;
use loco_rs::app::AppContext;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::Result;
use serde::{Deserialize, Serialize};

use crate::models::command_runs::Model as CommandRunModel;

pub struct LogCleanupWorker {
    pub ctx: AppContext,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogCleanupWorkerArgs;

/// Configuration for log cleanup behavior.
#[derive(Debug, Clone)]
pub struct LogCleanupConfig {
    /// Maximum number of days to retain log files.
    pub retention_days: u64,
    /// Maximum size of a single log file in bytes (default 10MB).
    pub max_file_bytes: u64,
}

impl Default for LogCleanupConfig {
    fn default() -> Self {
        Self {
            retention_days: 30,
            max_file_bytes: 10 * 1024 * 1024, // 10MB
        }
    }
}

#[async_trait]
impl BackgroundWorker<LogCleanupWorkerArgs> for LogCleanupWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn perform(&self, _args: LogCleanupWorkerArgs) -> Result<()> {
        let config = LogCleanupConfig::default();

        // 1. Truncate oversized log files
        self.truncate_oversized_logs(&config).await?;

        // 2. Remove stale logs beyond retention period
        self.remove_stale_logs(&config).await?;

        // 3. Vacuum records where log files no longer exist
        self.vacuum_missing_logs().await?;

        Ok(())
    }
}

impl LogCleanupWorker {
    /// Find and truncate log files that exceed the maximum size.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn truncate_oversized_logs(&self, config: &LogCleanupConfig) -> Result<()> {
        let runs = CommandRunModel::find_with_log(&self.ctx)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to find runs with logs: {e}")))?;

        for run in runs {
            if let Some(ref log_path) = run.log_path {
                let path = std::path::Path::new(log_path);
                if let Ok(metadata) = fs::metadata(path) {
                    if metadata.len() > config.max_file_bytes {
                        if let Err(e) = Self::truncate_head(path, config.max_file_bytes) {
                            tracing::warn!(
                                run_id = run.id,
                                error = %e,
                                "failed to truncate oversized log"
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Remove log files that exceed the retention period.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn remove_stale_logs(&self, config: &LogCleanupConfig) -> Result<()> {
        let days = i64::try_from(config.retention_days).unwrap_or(0);
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let runs = CommandRunModel::find_stale(&self.ctx, cutoff)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to find stale runs: {e}")))?;

        for run in runs {
            let run_id = run.id;
            if let Some(ref log_path) = run.log_path {
                let path = std::path::Path::new(log_path);
                if let Err(e) = fs::remove_file(path) {
                    tracing::warn!(
                        run_id = run_id,
                        error = %e,
                        "failed to remove stale log file"
                    );
                }

                // Mark as removed in DB
                let mut active: crate::models::command_runs::ActiveModel = run.into();
                if let Err(e) = active.mark_log_removed(&self.ctx).await {
                    tracing::warn!(
                        run_id = run_id,
                        error = %e,
                        "failed to mark log as removed"
                    );
                }
            }
        }

        Ok(())
    }

    /// Find records where `log_path` is set but the file doesn't exist on disk.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn vacuum_missing_logs(&self) -> Result<()> {
        let runs = CommandRunModel::find_with_log(&self.ctx)
            .await
            .map_err(|e| loco_rs::Error::string(&format!("failed to find runs with logs: {e}")))?;

        for run in runs {
            let run_id = run.id;
            if let Some(ref log_path) = run.log_path {
                let path = std::path::Path::new(log_path);
                if !path.exists() {
                    let mut active: crate::models::command_runs::ActiveModel = run.into();
                    if let Err(e) = active.mark_log_removed(&self.ctx).await {
                        tracing::warn!(
                            run_id = run_id,
                            error = %e,
                            "failed to vacuum missing log record"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Truncate the head of a log file, keeping only the last `max_bytes`.
    ///
    /// # Errors
    /// Returns an I/O error if the file cannot be read or written.
    pub fn truncate_head(path: &std::path::Path, max_bytes: u64) -> std::io::Result<()> {
        let metadata = fs::metadata(path)?;
        let file_len = metadata.len();

        if file_len <= max_bytes {
            return Ok(());
        }

        // Read the last max_bytes of the file
        let skip = file_len - max_bytes;
        let buf_size = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        let mut buffer = vec![0u8; buf_size];
        let mut file = fs::File::open(path)?;
        file.seek(SeekFrom::Start(skip))?;
        file.read_exact(&mut buffer)?;

        // Write back only the tail
        let mut file = fs::File::create(path)?;
        file.write_all(&buffer)?;

        Ok(())
    }
}
