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

    // But is_alive should return false because no running command runs exist
    assert!(!is_alive(&boot.app_context, &found[0]).await);
}
