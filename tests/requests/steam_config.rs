use game_smith::app::App;
use game_smith::initializers::embedded_i18n::EmbeddedViews;
use game_smith::views::steam_config::config as render_steam_config;
use loco_rs::testing::prelude::*;
use serial_test::serial;
/// GET /steam-config should return 200.
#[tokio::test]
#[serial]
async fn steam_config_show_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/steam-config").await;
        response.assert_status_success();
    })
    .await;
}

/// POST /steam-config with valid username/password should return 200 or redirect.
#[tokio::test]
#[serial]
async fn steam_config_save_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/steam-config")
            .form(&[
                ("steam_username", "test_user"),
                ("steam_password", "test_pass"),
            ])
            .await;
        response.assert_status_success();
    })
    .await;
}

/// POST /steam-config/clear should return 200 or redirect.
#[tokio::test]
#[serial]
async fn steam_config_clear_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.post("/steam-config/clear").await;
        response.assert_status_success();
    })
    .await;
}

/// Verify the view struct passes all fields the template expects.
///
/// The steam_config/config.html template accesses:
/// - `username` for the form input value
/// - `error` for validation error messages
/// - `success` for success messages
///
/// If any field is missing from the `data!` map, the template render will fail.
#[tokio::test]
#[serial]
async fn steam_config_view_struct_regression() {
    let boot = boot_test::<App>().await.expect("Failed to boot test app");
    let ctx = &boot.app_context;

    let views = EmbeddedViews::build().expect("Failed to create embedded views");

    let result = render_steam_config(&ctx, &views, Some("test_user"), None, None);
    assert!(result.is_ok(), "Steam config view should render");

    // Also test with error message
    let result = render_steam_config(&ctx, &views, None, Some("test error"), None);
    assert!(result.is_ok(), "Steam config view should render with error");

    // Also test with success message
    let result = render_steam_config(&ctx, &views, None, None, Some("credentials cleared"));
    assert!(
        result.is_ok(),
        "Steam config view should render with success"
    );

    // Also test with all None (fresh page)
    let result = render_steam_config(&ctx, &views, None, None, None);
    assert!(
        result.is_ok(),
        "Steam config view should render empty state"
    );
}
