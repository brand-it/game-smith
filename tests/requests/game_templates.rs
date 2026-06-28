use game_smith::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// GET /templates renders the template list page.
#[tokio::test]
#[serial]
async fn templates_list_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/templates").await;
        response.assert_status_success();
    })
    .await;
}

/// GET /templates/new renders the create template form.
#[tokio::test]
#[serial]
async fn templates_new_form_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/templates/new").await;
        response.assert_status_success();
    })
    .await;
}

/// POST /templates with a duplicate name re-renders the form with an error message
/// and the submitted data preserved.
#[tokio::test]
#[serial]
async fn templates_create_duplicate_name_renders_form_with_error() {
    use game_smith::models::game_templates::ActiveModel;

    request::<App, _, _>(|request, ctx| async move {
        // Create an initial template
        ActiveModel::create(
            &ctx,
            "Duplicate Test Template".to_string(),
            Some("Test description".to_string()),
            730,
            None,
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            None,
        )
        .await
        .expect("Failed to create initial template");

        // Try to create a template with the same name
        let response = request
            .post("/templates")
            .form(&[
                ("app_id", "730"),
                ("name", "Duplicate Test Template"),
                ("description", "Test description"),
                ("server_mod", ""),
                ("beta_branch", ""),
                ("boot_script", ""),
                ("use_steam_login", "false"),
                ("auto_start", "false"),
                ("auto_restart", "false"),
                ("auto_update", "false"),
                ("update_on_start", "false"),
                ("restart_schedule", ""),
            ])
            .await;

        // Should return 200 with form re-render (not redirect)
        response.assert_status_success();
        let body = response.text();

        // Error message should be present
        assert!(
            body.contains("already exists"),
            "Form should show duplicate error message, got: {}",
            body
        );

        // Form data should be preserved
        assert!(
            body.contains("Duplicate Test Template"),
            "Form should preserve submitted name, got: {}",
            body
        );
        assert!(
            body.contains("730"),
            "Form should preserve submitted app_id, got: {}",
            body
        );
    })
    .await;
}
