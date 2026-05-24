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

/// Verify that command runs render in the list page with success/failure status.
#[tokio::test]
#[serial]
async fn commands_list_renders_with_data() {
    request::<App, _, _>(|request, ctx| async move {
        // Create a completed run with exit code 0
        let run = game_smith::models::command_runs::ActiveModel::create_run(
            &ctx,
            "echo".to_string(),
            vec!["hello".to_string()],
            None,
            None,
            None,
            None,
        )
        .await
        .expect("Failed to create run");

        let mut active: game_smith::models::command_runs::ActiveModel = run.into();
        active
            .finish(&ctx, Some(0), "completed".to_string())
            .await
            .expect("Failed to finish run");

        let response = request.get("/commands").await;
        response.assert_status_success();

        let body = response.text();
        assert!(
            body.contains("Success"),
            "Expected 'Success' status for exit code 0"
        );
        assert!(
            body.contains("text-green-600"),
            "Expected green styling for success"
        );
    })
    .await;
}

/// Verify that non-zero exit codes render as "Failed" in red.
#[tokio::test]
#[serial]
async fn commands_list_shows_failed_for_nonzero_exit() {
    request::<App, _, _>(|request, ctx| async move {
        let run = game_smith::models::command_runs::ActiveModel::create_run(
            &ctx,
            "false_command".to_string(),
            vec![],
            None,
            None,
            None,
            None,
        )
        .await
        .expect("Failed to create run");

        let mut active: game_smith::models::command_runs::ActiveModel = run.into();
        active
            .finish(&ctx, Some(1), "failed".to_string())
            .await
            .expect("Failed to finish run");

        let response = request.get("/commands").await;
        response.assert_status_success();

        let body = response.text();
        // The false_command row should contain both the command name and Failed status
        assert!(
            body.contains("Failed") && body.contains("text-red-600"),
            "Expected Failed status with red styling for non-zero exit code"
        );
    })
    .await;
}

/// Verify that missing exit codes (running commands) render as placeholder.
#[tokio::test]
#[serial]
async fn commands_list_shows_placeholder_for_missing_exit() {
    request::<App, _, _>(|request, ctx| async move {
        let _run = game_smith::models::command_runs::ActiveModel::create_run(
            &ctx,
            "sleep_long".to_string(),
            vec!["100".to_string()],
            None,
            None,
            None,
            None,
        )
        .await
        .expect("Failed to create run");

        // Run is still "running" with no exit_code set
        let response = request.get("/commands").await;
        response.assert_status_success();

        let body = response.text();
        assert!(
            body.contains("—"),
            "Expected placeholder character for missing exit code"
        );
    })
    .await;
}
