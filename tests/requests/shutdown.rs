use game_smith::app::App;
use game_smith::initializers::embedded_i18n::EmbeddedViews;
use game_smith::models::game_servers::{ActiveModel, CreateServerForm};
use game_smith::views::shutdown::show as render_shutdown;
use loco_rs::testing::prelude::*;
use serial_test::serial;

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
        boot_script: None,
        auto_start: false,
        auto_restart: false,
        auto_update: false,
        update_on_start: false,
        restart_schedule: None,
    }
}

/// Test that the shutdown view renders successfully with servers.
///
/// This is the regression test for the missing `app_id` in `ShutdownServerView`:
/// the view struct had `id` and `name` but the template needed `server.app_id`
/// for the `steam_library` image macro. Omitting `app_id` caused a Tera
/// render error (HTTP 500) with no useful line number in the logs.
#[tokio::test]
#[serial]
async fn shutdown_view_renders_with_servers() {
    let boot = boot_test::<App>().await.expect("Failed to boot test app");
    let ctx = &boot.app_context;

    // Create a server
    let server = ActiveModel::create(ctx, &make_form(730, "My CS2 Server"), None)
        .await
        .expect("Failed to create game server");

    let servers = vec![server];
    let views = EmbeddedViews::build().expect("Failed to create embedded views");

    // This would return Err if template rendering fails (e.g. missing app_id)
    let result = render_shutdown(&views, &servers);
    assert!(result.is_ok(), "Shutdown view should render without error");
}

/// Test that the shutdown view renders successfully with no servers.
#[tokio::test]
#[serial]
async fn shutdown_view_renders_empty_state() {
    let views = EmbeddedViews::build().expect("Failed to create embedded views");

    let result = render_shutdown(&views, &[]);
    assert!(
        result.is_ok(),
        "Shutdown view should render empty state without error"
    );
}

/// GET /shutdown/status returns JSON with server list.
#[tokio::test]
#[serial]
async fn shutdown_status_api_returns_json() {
    use serde_json::Value;

    request::<App, _, _>(|request, ctx| async move {
        ActiveModel::create(&ctx, &make_form(740, "Status Test Server"), None)
            .await
            .expect("Failed to create game server");

        let response = request.get("/shutdown/status").await;
        response.assert_status_success();

        let body: Value = response.json();
        assert!(body.is_object());
        assert!(body.get("servers").is_some());
    })
    .await;
}

/// Verify ShutdownServerView contains all fields the template expects.
///
/// The shutdown.html template iterates over `servers` and accesses:
/// - `server.id` for DOM element IDs
/// - `server.name` for display
/// - `server.app_id` for the steam_library image macro
///
/// If any field is missing from the view struct, the template render will fail.
#[tokio::test]
#[serial]
async fn shutdown_server_view_has_app_id() {
    use game_smith::views::shutdown::ShutdownServerView;

    let boot = boot_test::<App>().await.expect("Failed to boot test app");
    let ctx = &boot.app_context;

    let server = ActiveModel::create(ctx, &make_form(730, "View Test Server"), None)
        .await
        .expect("Failed to create game server");

    let view = ShutdownServerView::new(&server);
    assert_eq!(view.app_id, 730, "app_id should be populated from server");
    assert_eq!(view.name, "View Test Server");
    assert!(view.id > 0, "id should be a valid database id");
}
