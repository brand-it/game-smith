use std::fs;
use std::path::PathBuf;

use game_smith::app::App;
use game_smith::models::command_runs::{ActiveModel, CommandStatus, Model as CommandRunModel};
use game_smith::workers::log_cleanup::LogCleanupWorker;
use loco_rs::bgworker::BackgroundWorker;
use loco_rs::testing::prelude::*;
use serial_test::serial;

macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        let _guard = settings.bind_to_scope();
    };
}

/// Helper to create a temp log file with a given size.
fn create_temp_log(size: u64) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("game-smith-test-{}", uuid::Uuid::new_v4()));
    let path = dir.join("test.log");
    fs::create_dir_all(&dir).expect("Failed to create temp dir");

    // Write a file of the requested size
    let mut file = fs::File::create(&path).expect("Failed to create temp file");
    for _ in 0..size {
        use std::io::Write;
        file.write_all(b"a").expect("Failed to write to temp file");
    }

    path
}

#[tokio::test]
#[serial]
async fn test_log_cleanup_worker_build() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    let _worker = LogCleanupWorker::build(&boot.app_context);
    // Worker should be buildable without errors
}

#[tokio::test]
#[serial]
async fn test_truncate_oversized_logs() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a temp log file larger than 1KB
    let log_path = create_temp_log(2048);
    let log_path_str = log_path.to_string_lossy().to_string();

    // Create a completed run with this log path
    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["large output".to_string()],
        None,
        None,
        Some(log_path_str.clone()),
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let mut active: ActiveModel = model.into();
    active
        .finish(&boot.app_context, Some(0), CommandStatus::Completed)
        .await
        .expect("Failed to finish run");

    // Create worker with a small max file size (1KB)
    let worker = LogCleanupWorker {
        ctx: boot.app_context.clone(),
    };

    // The truncate_head function should reduce the file to 1024 bytes
    let config = game_smith::workers::log_cleanup::LogCleanupConfig {
        max_file_bytes: 1024,
        ..Default::default()
    };

    worker
        .truncate_oversized_logs(&config)
        .await
        .expect("Failed to truncate logs");

    // Verify the file was truncated
    let metadata = fs::metadata(&log_path).expect("Failed to read file metadata");
    assert!(metadata.len() <= 1024);
    assert!(metadata.len() > 0);

    // Clean up
    let _ = fs::remove_file(&log_path);
    let _ = fs::remove_dir(log_path.parent().unwrap());
}

#[tokio::test]
#[serial]
async fn test_vacuum_missing_logs() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a temp log file
    let log_path = create_temp_log(100);
    let log_path_str = log_path.to_string_lossy().to_string();

    // Create a completed run with this log path
    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["test".to_string()],
        None,
        None,
        Some(log_path_str.clone()),
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = model.id;

    let mut active: ActiveModel = model.into();
    active
        .finish(&boot.app_context, Some(0), CommandStatus::Completed)
        .await
        .expect("Failed to finish run");

    // Delete the log file to simulate it being missing
    fs::remove_file(&log_path).expect("Failed to delete temp file");
    fs::remove_dir(log_path.parent().unwrap()).ok();

    // Run vacuum — should mark the log as removed in DB
    let worker = LogCleanupWorker {
        ctx: boot.app_context.clone(),
    };

    worker
        .vacuum_missing_logs()
        .await
        .expect("Failed to vacuum logs");

    // Verify the record was updated
    let found = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run");

    let found = found.expect("Run not found");
    assert!(found.log_path.is_none());
    assert!(found.log_removed);
}

#[tokio::test]
#[serial]
async fn test_truncate_head_small_file() {
    configure_insta!();

    // Create a small file (100 bytes)
    let log_path = create_temp_log(100);

    // Try to truncate with a larger max size — file should be untouched
    let result = game_smith::workers::log_cleanup::LogCleanupWorker::truncate_head(&log_path, 1024);
    assert!(result.is_ok());

    let metadata = fs::metadata(&log_path).expect("Failed to read metadata");
    assert_eq!(metadata.len(), 100);

    // Clean up
    let _ = fs::remove_file(&log_path);
    let _ = fs::remove_dir(log_path.parent().unwrap());
}

#[tokio::test]
#[serial]
async fn test_remove_stale_logs() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a temp log file
    let log_path = create_temp_log(100);
    let log_path_str = log_path.to_string_lossy().to_string();

    // Create a completed run with this log path
    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["old".to_string()],
        None,
        None,
        Some(log_path_str.clone()),
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = model.id;

    let mut active: ActiveModel = model.into();
    active
        .finish(&boot.app_context, Some(0), CommandStatus::Completed)
        .await
        .expect("Failed to finish run");

    // Create worker
    let worker = LogCleanupWorker {
        ctx: boot.app_context.clone(),
    };

    // Use a cutoff in the future to make all completed runs stale
    let config = game_smith::workers::log_cleanup::LogCleanupConfig {
        retention_days: 0, // 0 days retention = everything is stale
        ..Default::default()
    };

    worker
        .remove_stale_logs(&config)
        .await
        .expect("Failed to remove stale logs");

    // Verify the log file was deleted
    assert!(!log_path.exists());

    // Verify the record was updated
    let found = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run");

    let found = found.expect("Run not found");
    assert!(found.log_path.is_none());
    assert!(found.log_removed);

    // Clean up
    let _ = fs::remove_dir(log_path.parent().unwrap());
}
