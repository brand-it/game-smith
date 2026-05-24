use game_smith::app::App;
use game_smith::models::command_runs::{ActiveModel, Model as CommandRunModel};
use loco_rs::testing::prelude::*;
use serial_test::serial;

macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        let _guard = settings.bind_to_scope();
    };
}

#[tokio::test]
#[serial]
async fn test_create_run() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a new command run
    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["hello".to_string()],
        None,
        None,
        Some("/tmp/test.log".to_string()),
        None,
    )
    .await
    .expect("Failed to create run");

    // Verify the run was created with correct status
    assert_eq!(model.status, "running");
    assert_eq!(model.command, "echo");
    assert!(model.log_path.is_some());
    assert!(model.log_path.as_ref().unwrap().ends_with("/test.log"));
    assert!(model.pid.is_none());

    // Verify we can find it by ID
    let found = CommandRunModel::find_by_id(&boot.app_context, model.id)
        .await
        .expect("Failed to find run by ID");
    assert!(found.is_some());
}

#[tokio::test]
#[serial]
async fn test_finish_run() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a new command run
    let model = ActiveModel::create_run(
        &boot.app_context,
        "sleep".to_string(),
        vec!["10".to_string()],
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = model.id;

    // Finish the run
    let mut active: ActiveModel = model.into();
    let finished = active
        .finish(&boot.app_context, Some(0), "completed".to_string())
        .await
        .expect("Failed to finish run");

    // Verify the run is finished
    assert_eq!(finished.status, "completed");
    assert_eq!(finished.exit_code, Some(0));
    assert!(finished.completed_at.is_some());

    // Verify we can find the updated record
    let found = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run by ID");
    let found = found.expect("Run not found");
    assert_eq!(found.status, "completed");
    assert_eq!(found.exit_code, Some(0));
}

#[tokio::test]
#[serial]
async fn test_update_pid() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a new command run
    let model = ActiveModel::create_run(
        &boot.app_context,
        "ls".to_string(),
        vec![],
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = model.id;

    // Update the PID
    let mut active: ActiveModel = model.into();
    let updated = active
        .update_pid(&boot.app_context, 12345)
        .await
        .expect("Failed to update PID");

    assert_eq!(updated.pid, Some(12345));

    // Verify we can find it by PID
    let found = CommandRunModel::find_by_pid(&boot.app_context, 12345)
        .await
        .expect("Failed to find run by PID");
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, run_id);
}

#[tokio::test]
#[serial]
async fn test_find_running() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create multiple runs
    let _running = ActiveModel::create_run(
        &boot.app_context,
        "sleep".to_string(),
        vec!["100".to_string()],
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let completed = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["done".to_string()],
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    // Mark the second one as completed
    let mut active: ActiveModel = completed.into();
    let _ = active
        .finish(&boot.app_context, Some(0), "completed".to_string())
        .await;

    // Find running runs
    let running = CommandRunModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running runs");

    // At least one should be running (the first one we created)
    assert!(!running.is_empty());
    assert!(running.iter().all(|r| r.status == "running"));
}

#[tokio::test]
#[serial]
async fn test_mark_log_removed() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a run with a log path
    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["test".to_string()],
        None,
        None,
        Some("/tmp/test.log".to_string()),
        None,
    )
    .await
    .expect("Failed to create run");

    // Mark log as removed
    let mut active: ActiveModel = model.into();
    let updated = active
        .mark_log_removed(&boot.app_context)
        .await
        .expect("Failed to mark log removed");

    assert!(updated.log_path.is_none());
    assert!(updated.log_removed);
}

#[tokio::test]
#[serial]
async fn test_find_stale() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a completed run
    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["old".to_string()],
        None,
        None,
        Some("/tmp/old.log".to_string()),
        None,
    )
    .await
    .expect("Failed to create run");

    let mut active: ActiveModel = model.into();
    active
        .finish(&boot.app_context, Some(0), "completed".to_string())
        .await
        .expect("Failed to finish run");

    // Find stale runs (using a cutoff in the future, so all completed runs should be stale)
    let cutoff = chrono::Utc::now() + chrono::Duration::days(1);
    let stale = CommandRunModel::find_stale(&boot.app_context, cutoff)
        .await
        .expect("Failed to find stale runs");

    // Should include our completed run
    assert!(!stale.is_empty());
    assert!(stale.iter().all(|r| r.status == "completed"));
}

#[tokio::test]
#[serial]
async fn test_find_nonexistent_by_id() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Try to find a run that doesn't exist
    let found = CommandRunModel::find_by_id(&boot.app_context, 99999)
        .await
        .expect("Failed to find run by ID");

    assert!(found.is_none());
}

#[tokio::test]
#[serial]
async fn test_find_nonexistent_by_pid() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Try to find a run by PID that doesn't exist
    let found = CommandRunModel::find_by_pid(&boot.app_context, 99999)
        .await
        .expect("Failed to find run by PID");

    assert!(found.is_none());
}

#[tokio::test]
#[serial]
async fn test_is_running_helper() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a running run
    let model = ActiveModel::create_run(
        &boot.app_context,
        "sleep".to_string(),
        vec!["100".to_string()],
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    assert!(model.is_running());

    // Mark it as completed
    let mut active: ActiveModel = model.into();
    let completed = active
        .finish(&boot.app_context, Some(0), "completed".to_string())
        .await
        .expect("Failed to finish run");

    assert!(!completed.is_running());
}
