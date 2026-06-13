use game_smith::app::App;
use game_smith::models::command_runs::ActiveModel as CommandRunActiveModel;
use game_smith::models::game_servers::{
    is_alive, ActiveModel, CreateServerForm, Model as GameServerModel, ServerStatus,
};
use game_smith::models::game_templates;
use loco_rs::model::ModelError;
use loco_rs::testing::prelude::*;
use sea_orm::ActiveModelTrait;
use serial_test::serial;

macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        let _guard = settings.bind_to_scope();
    };
}

/// Create a minimal [`CreateServerForm`] for test setup.
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

#[tokio::test]
#[serial]
async fn test_create_game_server() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let model = ActiveModel::create(&boot.app_context, &make_form(730, "My CS2 Server"), None)
        .await
        .expect("Failed to create game server");

    assert_eq!(model.status(), ServerStatus::Pending);
    assert_eq!(model.app_id, 730);
    assert_eq!(model.name, "My CS2 Server");
    assert_eq!(model.platform, "linux");
    assert!(!model.auto_start);
    assert!(!model.auto_restart);
    assert!(!model.auto_update);
    assert!(!model.update_on_start);
}

#[tokio::test]
#[serial]
async fn test_find_by_id() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let model = ActiveModel::create(&boot.app_context, &make_form(740, "CS:GO Server"), None)
        .await
        .expect("Failed to create game server");

    let found = GameServerModel::find_by_id(&boot.app_context, model.id)
        .await
        .expect("DB error")
        .expect("Record not found");

    assert_eq!(found.id, model.id);
    assert_eq!(found.app_id, 740);
}

#[tokio::test]
#[serial]
async fn test_find_by_id_nonexistent() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let result = GameServerModel::find_by_id(&boot.app_context, 999999)
        .await
        .expect("DB error");

    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn test_update_status() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let model = ActiveModel::create(
        &boot.app_context,
        &make_form(730, "Status Test Server"),
        None,
    )
    .await
    .expect("Failed to create game server");

    assert_eq!(model.status(), ServerStatus::Pending);

    let mut active: ActiveModel = model.into();
    let updated = active
        .update_status(&boot.app_context, ServerStatus::Installing, None)
        .await
        .expect("Failed to update status");

    assert_eq!(updated.status(), ServerStatus::Installing);
}

#[tokio::test]
#[serial]
async fn test_list() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let before = GameServerModel::list(&boot.app_context)
        .await
        .expect("Failed to list")
        .len();

    ActiveModel::create(&boot.app_context, &make_form(252490, "Rust Server"), None)
        .await
        .expect("Failed to create");

    let after = GameServerModel::list(&boot.app_context)
        .await
        .expect("Failed to list")
        .len();

    assert_eq!(after, before + 1);
}

#[tokio::test]
#[serial]
async fn test_app_id_u32() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let model = ActiveModel::create(&boot.app_context, &make_form(730, "App ID Test"), None)
        .await
        .expect("Failed to create");

    assert_eq!(model.app_id_u32(), 730u32);
}

// ── find_running tests ──────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn test_find_running_empty() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let running = GameServerModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running servers");

    assert!(running.is_empty());
}

#[tokio::test]
#[serial]
async fn test_find_running_filters_by_status() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    // Create three servers with different statuses
    let _stopped = ActiveModel::create(&boot.app_context, &make_form(730, "Stopped Server"), None)
        .await
        .expect("Failed to create");
    let mut stopped_active: ActiveModel = _stopped.clone().into();
    stopped_active
        .update_status(&boot.app_context, ServerStatus::Stopped, None)
        .await
        .expect("Failed to update");

    let _installed =
        ActiveModel::create(&boot.app_context, &make_form(740, "Installed Server"), None)
            .await
            .expect("Failed to create");
    let mut installed_active: ActiveModel = _installed.clone().into();
    installed_active
        .update_status(&boot.app_context, ServerStatus::Installed, None)
        .await
        .expect("Failed to update");

    let running = ActiveModel::create(&boot.app_context, &make_form(750, "Running Server"), None)
        .await
        .expect("Failed to create");
    let mut running_active: ActiveModel = running.clone().into();
    running_active
        .update_status(&boot.app_context, ServerStatus::Running, None)
        .await
        .expect("Failed to update");

    // find_running should only return the running server
    let found = GameServerModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running servers");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, running.id);
    assert_eq!(found[0].status(), ServerStatus::Running);
}

#[tokio::test]
#[serial]
async fn test_find_running_returns_zombie_servers() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    // Create a server marked as "running" but with no running command runs
    let server = ActiveModel::create(&boot.app_context, &make_form(730, "Zombie Server"), None)
        .await
        .expect("Failed to create");
    let mut active: ActiveModel = server.clone().into();
    active
        .update_status(&boot.app_context, ServerStatus::Running, None)
        .await
        .expect("Failed to update");

    // find_running should still return it (DB status is authoritative)
    let found = GameServerModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running servers");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, server.id);

    // is_alive checks the actual process — with no real PID set,
    // it returns false even though DB status is Running.
    assert!(!is_alive(&boot.app_context, &found[0]).await);

    // find_alive must exclude zombies (DB "Running" but no alive process)
    let alive = GameServerModel::find_alive(&boot.app_context)
        .await
        .expect("Failed to find alive servers");
    assert!(alive.is_empty(), "find_alive should exclude zombie servers");
}

/// `find_alive` includes servers with a live PID and excludes zombies.
///
/// PID 1 (init/systemd) is guaranteed to be alive on Linux. A stale
/// PID that doesn't exist on the system must be filtered out.
#[tokio::test]
#[serial]
#[cfg(target_os = "linux")]
async fn test_find_alive_excludes_zombies() {
    // Helper: insert a command_run for a given server_id and pid.
    async fn insert_run(ctx: &loco_rs::app::AppContext, server_id: i32, pid: i64) {
        use sea_orm::ActiveValue;
        let run = CommandRunActiveModel {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::Set(chrono::Utc::now().into()),
            updated_at: ActiveValue::Set(chrono::Utc::now().into()),
            command: ActiveValue::Set("test".to_string()),
            args: ActiveValue::Set(serde_json::json!([])),
            working_dir: ActiveValue::NotSet,
            log_path: ActiveValue::NotSet,
            env: ActiveValue::NotSet,
            status: ActiveValue::Set("running".to_string()),
            exit_code: ActiveValue::NotSet,
            started_at: ActiveValue::Set(chrono::Utc::now().naive_utc()),
            completed_at: ActiveValue::NotSet,
            server_id: ActiveValue::Set(Some(i64::from(server_id))),
            log_removed: ActiveValue::Set(false),
            pid: ActiveValue::Set(Some(pid)),
            title: ActiveValue::NotSet,
        };
        run.insert(&ctx.db).await.unwrap();
    }

    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    // Create a zombie server: "Running" status with a command run having
    // a PID that almost certainly doesn't exist.
    let zombie = ActiveModel::create(
        &boot.app_context,
        &make_form(730, "Zombie Alive Test"),
        None,
    )
    .await
    .expect("Failed to create zombie");
    let mut zombie_active: ActiveModel = zombie.clone().into();
    zombie_active.status = sea_orm::ActiveValue::Set(ServerStatus::Running.as_str().to_string());
    zombie_active
        .clone()
        .update(&boot.app_context.db)
        .await
        .map_err(ModelError::from)
        .expect("Failed to update zombie");
    insert_run(&boot.app_context, zombie.id, 99999).await;

    // Create an "alive" server: "Running" status with a command run having
    // the PID of this process (always alive).
    let own_pid = std::process::id() as i64;
    let alive_server = ActiveModel::create(&boot.app_context, &make_form(740, "Alive Test"), None)
        .await
        .expect("Failed to create alive server");
    let mut alive_active: ActiveModel = alive_server.clone().into();
    alive_active.status = sea_orm::ActiveValue::Set(ServerStatus::Running.as_str().to_string());
    alive_active
        .clone()
        .update(&boot.app_context.db)
        .await
        .map_err(ModelError::from)
        .expect("Failed to update alive server");
    insert_run(&boot.app_context, alive_server.id, own_pid).await;

    // find_running returns both (DB status is "Running" for both)
    let running = GameServerModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running servers");
    assert_eq!(running.len(), 2);

    // find_alive queries ALL servers regardless of DB status —
    // but only the server with a live PID in its command runs is included.
    let alive = GameServerModel::find_alive(&boot.app_context)
        .await
        .expect("Failed to find alive servers");
    assert_eq!(alive.len(), 1);
    assert_eq!(alive[0].id, alive_server.id);
}

/// A server marked "Stopped" in the DB but with a live PID in its command
/// runs should still be included by `find_alive`. During shutdown, DB status
/// is intent, not ground truth — the process is the source of truth.
#[tokio::test]
#[serial]
#[cfg(target_os = "linux")]
async fn test_find_alive_includes_stopped_but_alive() {
    // Helper: insert a command_run for a given server_id and pid.
    async fn insert_run(ctx: &loco_rs::app::AppContext, server_id: i32, pid: i64) {
        use sea_orm::ActiveValue;
        let run = CommandRunActiveModel {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::Set(chrono::Utc::now().into()),
            updated_at: ActiveValue::Set(chrono::Utc::now().into()),
            command: ActiveValue::Set("test".to_string()),
            args: ActiveValue::Set(serde_json::json!([])),
            working_dir: ActiveValue::NotSet,
            log_path: ActiveValue::NotSet,
            env: ActiveValue::NotSet,
            status: ActiveValue::Set("running".to_string()),
            exit_code: ActiveValue::NotSet,
            started_at: ActiveValue::Set(chrono::Utc::now().naive_utc()),
            completed_at: ActiveValue::NotSet,
            server_id: ActiveValue::Set(Some(i64::from(server_id))),
            log_removed: ActiveValue::Set(false),
            pid: ActiveValue::Set(Some(pid)),
            title: ActiveValue::NotSet,
        };
        run.insert(&ctx.db).await.unwrap();
    }

    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    // Create a server with status "Stopped" but a command run with a live PID.
    let own_pid = std::process::id() as i64;
    let server = ActiveModel::create(
        &boot.app_context,
        &make_form(750, "Stopped But Alive"),
        None,
    )
    .await
    .expect("Failed to create server");
    let mut active: ActiveModel = server.clone().into();
    active.status = sea_orm::ActiveValue::Set(ServerStatus::Stopped.as_str().to_string());
    active
        .clone()
        .update(&boot.app_context.db)
        .await
        .map_err(ModelError::from)
        .expect("Failed to update server");
    insert_run(&boot.app_context, server.id, own_pid).await;

    // find_running should NOT return it (DB status is Stopped)
    let running = GameServerModel::find_running(&boot.app_context)
        .await
        .expect("Failed to find running servers");
    assert!(
        !running.iter().any(|s| s.id == server.id),
        "find_running should exclude stopped server"
    );

    // find_alive MUST include it (process is actually alive)
    let alive = GameServerModel::find_alive(&boot.app_context)
        .await
        .expect("Failed to find alive servers");
    assert!(
        alive.iter().any(|s| s.id == server.id),
        "find_alive should include a stopped-but-alive server"
    );
}
/// update() must not return a WriteScript error when install_dir does not exist.
///
/// Regression test: `GameServerInstaller::update()` called `fs::write(install_dir/update_NNN.txt)`
/// without first calling `create_dir_all`, so any server whose `install_dir` was absent on disk
/// produced `GameServerError::WriteScript` → HTTP 500 in the `POST /servers/:id/update` handler.
///
/// The fix must add `create_dir_all(install_dir)` before `fs::write` in `update()`, mirroring
/// what `install()` already does.
#[tokio::test]
#[serial]
async fn test_update_creates_install_dir_before_writing_script() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();

    // Build a fake SteamCMD binary so is_installed() returns true and
    // update() can reach the fs::write call.
    //
    // Path chain:
    //   XDG_DATA_HOME = tmp
    //   resolve_data_home() = "{tmp}/game-smith"
    //   AppDirs::new(data_home) → app_dir = "{tmp}/game-smith/game-smith"
    //   SteamCmd::new(dirs) → binary = app_dir/steamcmd/steamcmd.sh
    let tmp = std::env::temp_dir().join(format!("gs-test-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let fake_steamcmd_dir = tmp.join("game-smith").join("game-smith").join("steamcmd");
    std::fs::create_dir_all(&fake_steamcmd_dir).expect("failed to create fake steamcmd dir");
    #[cfg(target_os = "windows")]
    let binary_name = "steamcmd.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "steamcmd.sh";
    std::fs::write(fake_steamcmd_dir.join(binary_name), b"#!/bin/sh\n")
        .expect("failed to write fake binary");

    // Point resolve_data_home() at our temp tree.
    #[cfg(target_os = "windows")]
    std::env::set_var("APPDATA", &tmp);
    #[cfg(not(target_os = "windows"))]
    std::env::set_var("XDG_DATA_HOME", &tmp);

    // install_dir is a subdirectory that does NOT exist on disk.
    let install_dir = tmp
        .join("nonexistent-install-dir")
        .to_string_lossy()
        .to_string();

    // Create a real DB record so the HTTP handler can load it.
    let server = ActiveModel::create(
        &boot.app_context,
        &make_form(740, "Update Regression Server"),
        None,
    )
    .await
    .expect("failed to create server record");

    let installer =
        game_smith::data::game_server_installer::GameServerInstaller::new(&boot.app_context);
    let result = installer.update(&server).await;

    // Always clean up env and temp files before asserting.
    #[cfg(target_os = "windows")]
    let _ = std::env::remove_var("APPDATA");
    #[cfg(not(target_os = "windows"))]
    let _ = std::env::remove_var("XDG_DATA_HOME");
    let _ = std::fs::remove_dir_all(&tmp);

    // Before the fix: WriteScript(Os { code: 2, kind: NotFound }) because
    // install_dir was never created before fs::write.
    // After the fix: install_dir is created, WriteScript must not occur.
    //
    // SteamCmdNotInstalled means the fake binary path was wrong in the test
    // setup — treat it as a test-setup failure so the test is never vacuous.
    match result {
        Err(game_smith::data::game_server_installer::GameServerError::WriteScript(e)) => {
            panic!("update() returned WriteScript — install_dir not created before fs::write: {e}");
        }
        Err(game_smith::data::game_server_installer::GameServerError::SteamCmdNotInstalled) => {
            panic!(
                "test setup error: fake SteamCMD binary was not found — \
                 check XDG_DATA_HOME path construction"
            );
        }
        // Execute error (fake binary ran and failed) or Ok — both are fine.
        _ => {}
    }
}
#[tokio::test]
#[serial]
async fn test_create_game_server_from_template() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    // Create a template with specific settings
    let template = game_templates::ActiveModel::create(
        &boot.app_context,
        "Test Template".to_string(),
        Some("Template for testing".to_string()),
        730,
        Some("my_mod".to_string()),
        Some("dev".to_string()),
        Some("echo hello".to_string()),
        true,  // use_steam_login
        true,  // auto_start
        true,  // auto_restart
        false, // auto_update
        true,  // update_on_start
        Some("0 3 * * *".to_string()),
    )
    .await
    .expect("Failed to create template");

    // Create a server from the template
    let form = CreateServerForm {
        app_id: "730".to_string(),
        name: "Template Server".to_string(),
        server_mod: None,
        beta_branch: None,
        use_steam_login: false,
        steam_username: None,
        steam_password: None,
        template_id: Some(template.id),
    };
    let server = ActiveModel::create(&boot.app_context, &form, Some(&template))
        .await
        .expect("Failed to create game server from template");

    // Verify template settings were applied
    assert_eq!(server.template_id, Some(template.id));
    assert_eq!(server.boot_script, Some("echo hello".to_string()));
    assert!(server.auto_start);
    assert!(server.auto_restart);
    assert!(!server.auto_update);
    assert!(server.update_on_start);
    assert_eq!(server.restart_schedule, Some("0 3 * * *".to_string()));

    // Verify form values take precedence over template for use_steam_login
    assert!(!server.use_steam_login);
}
