use game_smith::app::App;
use game_smith::models::game_servers::{
    is_alive, ActiveModel, Model as GameServerModel, ServerStatus,
};
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
async fn test_create_game_server() {
    configure_insta!();
    let boot = boot_test::<App>().await.unwrap();

    let model = ActiveModel::create(
        &boot.app_context,
        730,
        "My CS2 Server".to_string(),
        "/tmp/game-smith/games/my-cs2-server".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
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

    let model = ActiveModel::create(
        &boot.app_context,
        740,
        "CS:GO Server".to_string(),
        "/tmp/game-smith/games/csgo-server".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
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
        730,
        "Status Test Server".to_string(),
        "/tmp/game-smith/games/status-test".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
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

    ActiveModel::create(
        &boot.app_context,
        252490,
        "Rust Server".to_string(),
        "/tmp/game-smith/games/rust-server".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
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

    let model = ActiveModel::create(
        &boot.app_context,
        730,
        "App ID Test".to_string(),
        "/tmp/game-smith/games/appid-test".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
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
    let _stopped = ActiveModel::create(
        &boot.app_context,
        730,
        "Stopped Server".to_string(),
        "/tmp/game-smith/games/stopped".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
    .await
    .expect("Failed to create");
    let mut stopped_active: ActiveModel = _stopped.clone().into();
    stopped_active
        .update_status(&boot.app_context, ServerStatus::Stopped, None)
        .await
        .expect("Failed to update");

    let _installed = ActiveModel::create(
        &boot.app_context,
        740,
        "Installed Server".to_string(),
        "/tmp/game-smith/games/installed".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
    .await
    .expect("Failed to create");
    let mut installed_active: ActiveModel = _installed.clone().into();
    installed_active
        .update_status(&boot.app_context, ServerStatus::Installed, None)
        .await
        .expect("Failed to update");

    let running = ActiveModel::create(
        &boot.app_context,
        750,
        "Running Server".to_string(),
        "/tmp/game-smith/games/running".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
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
    let server = ActiveModel::create(
        &boot.app_context,
        730,
        "Zombie Server".to_string(),
        "/tmp/game-smith/games/zombie".to_string(),
        "linux".to_string(),
        None,
        None,
        false,
    )
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

    // is_alive trusts the DB status when no command runs exist
    // (could be a fresh start or zombie — pid_liveness handles cleanup)
    assert!(is_alive(&boot.app_context, &found[0]).await);
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
        740,
        "Update Regression Server".to_string(),
        install_dir,
        "linux".to_string(),
        None,
        None,
        false,
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
