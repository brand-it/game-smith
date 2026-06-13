use game_smith::app::App;
use game_smith::models::command_runs::{ActiveModel, CommandStatus, Model as CommandRunModel};
use game_smith::models::game_servers::{
    ActiveModel as GameServerActiveModel, CreateServerForm, Model as GameServerModel, ServerStatus,
};
use loco_rs::boot::run_task;
use loco_rs::{task, testing::prelude::*};
use sea_orm::{ActiveModelTrait, ActiveValue};
use serial_test::serial;

macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        let _guard = settings.bind_to_scope();
    };
}

fn make_form(app_id: u32, name: &str) -> CreateServerForm {
    CreateServerForm {
        app_id: app_id.to_string(),
        name: name.to_string(),
        server_mod: None,
        beta_branch: None,
        use_steam_login: false,
        steam_username: None,
        steam_password: None,
        template_id: None,
    }
}

/// Verify the task runs successfully with no running command runs.
#[tokio::test]
#[serial]
async fn test_can_run_pid_liveness_empty() {
    let boot = boot_test::<App>().await.unwrap();

    assert!(run_task::<App>(
        &boot.app_context,
        Some(&"pid_liveness".to_string()),
        &task::Vars::default()
    )
    .await
    .is_ok());
}

/// Verify that a command run with a dead PID is marked as Failed.
#[tokio::test]
#[serial]
async fn test_pid_liveness_marks_dead_run_as_failed() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a running command run with a PID that doesn't exist
    let run = ActiveModel::create_run(
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

    let run_id = run.id;

    // Set a dead PID
    let mut active: ActiveModel = run.into();
    active
        .update_pid(&boot.app_context, 99999999)
        .await
        .expect("Failed to update PID");

    // Verify it's running with the dead PID
    let before = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run")
        .expect("Run not found");
    assert_eq!(before.status(), CommandStatus::Running);
    assert_eq!(before.pid, Some(99999999));

    // Run the task
    assert!(run_task::<App>(
        &boot.app_context,
        Some(&"pid_liveness".to_string()),
        &task::Vars::default()
    )
    .await
    .is_ok());

    // Verify the run is now marked as Failed
    let after = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run")
        .expect("Run not found");
    assert_eq!(after.status(), CommandStatus::Failed);
    assert!(after.completed_at.is_some());
}

/// Verify that a command run with no PID is marked as Failed.
#[tokio::test]
#[serial]
async fn test_pid_liveness_marks_null_pid_as_failed() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a running command run (create_run sets PID to current process)
    let run = ActiveModel::create_run(
        &boot.app_context,
        "echo".to_string(),
        vec!["hello".to_string()],
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = run.id;

    // Manually set PID to NULL to simulate a run created without process association
    let mut active: ActiveModel = run.into();
    active.pid = ActiveValue::Set(None);
    active
        .update(&boot.app_context.db)
        .await
        .expect("Failed to nullify PID");

    // Verify the run has no PID
    let before = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run")
        .expect("Run not found");
    assert_eq!(before.pid, None);
    assert_eq!(before.status(), CommandStatus::Running);

    // Run the task
    assert!(run_task::<App>(
        &boot.app_context,
        Some(&"pid_liveness".to_string()),
        &task::Vars::default()
    )
    .await
    .is_ok());

    // Verify the run is now marked as Failed
    let after = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run")
        .expect("Run not found");
    assert_eq!(after.status(), CommandStatus::Failed);
    assert!(after.completed_at.is_some());
}

/// Verify that a command run with a live PID is not touched.
#[tokio::test]
#[serial]
async fn test_pid_liveness_preserves_alive_runs() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Use our own PID — always alive
    let own_pid = std::process::id() as i64;

    // Create a running command run with our own PID (always alive)
    let run = ActiveModel::create_run(
        &boot.app_context,
        "self".to_string(),
        vec![],
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("Failed to create run");

    let run_id = run.id;

    let mut active: ActiveModel = run.into();
    active
        .update_pid(&boot.app_context, own_pid)
        .await
        .expect("Failed to update PID");

    // Run the task
    assert!(run_task::<App>(
        &boot.app_context,
        Some(&"pid_liveness".to_string()),
        &task::Vars::default()
    )
    .await
    .is_ok());

    // Verify the run is still Running
    let after = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run")
        .expect("Run not found");
    assert_eq!(after.status(), CommandStatus::Running);
}

/// Verify that when a dead command run has a server_id, the command run is
/// marked as Failed but the server status is untouched.
#[tokio::test]
#[serial]
async fn test_pid_liveness_preserves_server_status() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Create a game server
    let server =
        GameServerActiveModel::create(&boot.app_context, &make_form(730, "Test Server"), None)
            .await
            .expect("Failed to create server");

    let server_id = server.id as i64;
    let server_id_i32 = server.id;
    // Set server to Running
    let mut server_active: GameServerActiveModel = server.into();
    server_active
        .update_status(&boot.app_context, ServerStatus::Running, None)
        .await
        .expect("Failed to update server status");

    // Create a running command run associated with this server, with a dead PID
    let run = ActiveModel::create_run(
        &boot.app_context,
        "srcds_linux".to_string(),
        vec!["-game".to_string(), "csgo".to_string()],
        None,
        None,
        None,
        None,
        Some(server_id),
    )
    .await
    .expect("Failed to create run");

    let run_id = run.id;

    let mut active: ActiveModel = run.into();
    active
        .update_pid(&boot.app_context, 99999997)
        .await
        .expect("Failed to update run PID");

    // Run the task
    assert!(run_task::<App>(
        &boot.app_context,
        Some(&"pid_liveness".to_string()),
        &task::Vars::default()
    )
    .await
    .is_ok());

    // Verify the command run is marked as Failed
    let run_after = CommandRunModel::find_by_id(&boot.app_context, run_id)
        .await
        .expect("Failed to find run")
        .expect("Run not found");
    assert_eq!(run_after.status(), CommandStatus::Failed);

    // Verify the server status is unchanged (still Running)
    let server_after = GameServerModel::find_by_id(&boot.app_context, server_id_i32)
        .await
        .expect("Failed to find server")
        .expect("Server not found");
    assert_eq!(server_after.status(), ServerStatus::Running);
}
