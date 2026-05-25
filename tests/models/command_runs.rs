use game_smith::app::App;
use game_smith::models::command_runs::{ActiveModel, CommandStatus, Model as CommandRunModel};
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

    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["hello".to_string()],
        None,
        None,
        Some("/tmp/test.log".to_string()),
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    assert_eq!(model.status(), CommandStatus::Running);
    assert_eq!(model.command, "echo");
    assert!(model.log_path.is_some());
    assert!(model.log_path.as_ref().unwrap().ends_with("/test.log"));
    assert!(model.pid.is_none());

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

    let model = ActiveModel::create_run(
        &boot.app_context,
        "sleep".to_string(),
        vec!["10".to_string()],
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = model.id;

    let mut active: ActiveModel = model.into();
    let finished = active
        .finish(&boot.app_context, Some(0), CommandStatus::Completed)
        .await
        .expect("Failed to finish run");

    assert_eq!(finished.status(), CommandStatus::Completed);
    assert_eq!(finished.exit_code, Some(0));
    assert!(finished.completed_at.is_some());

    let found = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run by ID");
    let found = found.expect("Run not found");
    assert_eq!(found.status(), CommandStatus::Completed);
    assert_eq!(found.exit_code, Some(0));
}

#[tokio::test]
#[serial]
async fn test_update_pid() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    let model = ActiveModel::create_run(
        &boot.app_context,
        "ls".to_string(),
        vec![],
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = model.id;

    let mut active: ActiveModel = model.into();
    let updated = active
        .update_pid(&boot.app_context, 12345)
        .await
        .expect("Failed to update PID");

    assert_eq!(updated.pid, Some(12345));

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

    let _running = ActiveModel::create_run(
        &boot.app_context,
        "sleep".to_string(),
        vec!["100".to_string()],
        None,
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
        None,
    )
    .await
    .expect("Failed to create run");

    let mut active: ActiveModel = completed.into();
    let _ = active
        .finish(&boot.app_context, Some(0), CommandStatus::Completed)
        .await;

    let running = CommandRunModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running runs");

    assert!(!running.is_empty());
    assert!(running.iter().all(|r| r.status() == CommandStatus::Running));
}

#[tokio::test]
#[serial]
async fn test_mark_log_removed() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["test".to_string()],
        None,
        None,
        Some("/tmp/test.log".to_string()),
        None,
        None,
    )
    .await
    .expect("Failed to create run");

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

    let model = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["old".to_string()],
        None,
        None,
        Some("/tmp/old.log".to_string()),
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

    let cutoff = chrono::Utc::now() + chrono::Duration::days(1);
    let stale = CommandRunModel::find_stale(&boot.app_context, cutoff)
        .await
        .expect("Failed to find stale runs");

    assert!(!stale.is_empty());
    assert!(stale.iter().all(|r| r.status() != CommandStatus::Running));
}

#[tokio::test]
#[serial]
async fn test_find_nonexistent_by_id() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

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

    let model = ActiveModel::create_run(
        &boot.app_context,
        "sleep".to_string(),
        vec!["100".to_string()],
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    assert!(model.is_running());

    let mut active: ActiveModel = model.into();
    let completed = active
        .finish(&boot.app_context, Some(0), CommandStatus::Completed)
        .await
        .expect("Failed to finish run");

    assert!(!completed.is_running());
}

// ── find_running dedicated tests ────────────────────────────────────────

#[tokio::test]
#[serial]
async fn test_find_running_empty() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    let running = CommandRunModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running runs");

    assert!(running.is_empty());
}

#[tokio::test]
#[serial]
async fn test_find_running_excludes_completed() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // One running run
    let running_run = ActiveModel::create_run(
        &boot.app_context,
        "sleep".to_string(),
        vec!["100".to_string()],
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create running run");

    // One completed run
    let completed_run = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["done".to_string()],
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create completed run");

    let mut active: ActiveModel = completed_run.into();
    active
        .finish(&boot.app_context, Some(0), CommandStatus::Completed)
        .await
        .expect("Failed to finish run");

    let found = CommandRunModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running runs");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, running_run.id);
    assert_eq!(found[0].status(), CommandStatus::Running);
}

#[tokio::test]
#[serial]
async fn test_find_running_multiple() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create 3 running runs
    for i in 0..3_u8 {
        ActiveModel::create_run(
            &boot.app_context,
            format!("sleep-{i}"),
            vec!["100".to_string()],
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("Failed to create run");
    }

    let found = CommandRunModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running runs");

    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|r| r.status() == CommandStatus::Running));
}
