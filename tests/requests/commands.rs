use game_smith::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// Verify that `GET /` renders the commands list page without template errors.
/// This exercises the full rendering pipeline: base layout inheritance, macro
/// imports, and the `t()` Fluent translation function.
#[tokio::test]
#[serial]
async fn root_renders_commands_list() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/").await;
        response.assert_status_success();

        let body = response.text();
        assert!(body.contains("Command Runs"));
    })
    .await;
}

/// Verify that `GET /commands` renders the commands list page.
#[tokio::test]
#[serial]
async fn commands_list_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/commands").await;
        response.assert_status_success();

        let body = response.text();

        // Verify base layout is applied
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("<title>Command Runs</title>"));
        assert!(body.contains("bg-gray-50 min-h-screen"));

        // Verify i18n strings are translated in the page title
        assert!(body.contains("Command Runs"));

        // Verify empty state renders when no runs exist
        assert!(body.contains("No command runs found."));
    })
    .await;
}
