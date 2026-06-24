use game_smith::app::App;
use game_smith::models::game_servers::{ActiveModel, CreateServerForm};
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
    }
}

/// Creates a fake SteamCMD binary tree in a temp directory.
///
/// Returns the temp directory path. The caller is responsible for cleanup.
///
/// Path chain:
///   XDG_DATA_HOME = tmp
///   resolve_data_home() = "{tmp}/game-smith"
///   AppDirs::new(data_home) -> app_dir = "{tmp}/game-smith/game-smith"
///   SteamCmd::new(dirs) -> binary = app_dir/steamcmd/steamcmd.sh
fn setup_fake_steamcmd() -> std::path::PathBuf {
    let tmp = std::env::temp_dir().join(format!("gs-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let fake_steamcmd_dir = tmp.join("game-smith").join("game-smith").join("steamcmd");
    std::fs::create_dir_all(&fake_steamcmd_dir).expect("failed to create fake steamcmd dir");
    #[cfg(target_os = "windows")]
    let binary_name = "steamcmd.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "steamcmd.sh";
    let fake_binary_path = fake_steamcmd_dir.join(binary_name);
    std::fs::write(&fake_binary_path, b"#!/bin/sh\nexit 0\n").expect("failed to write fake binary");
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake_binary_path)
            .expect("failed to get metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_binary_path, perms).expect("failed to set permissions");
    }
    tmp
}

/// GET /servers renders the game server list page.
#[tokio::test]
#[serial]
async fn servers_list_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/servers").await;
        response.assert_status_success();

        let body = response.text();
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("Game Servers"));
        assert!(body.contains("New Server") || body.contains("Install"));
    })
    .await;
}

/// GET /servers shows empty state when no servers exist.
#[tokio::test]
#[serial]
async fn servers_list_empty_state() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/servers").await;
        response.assert_status_success();

        let body = response.text();
        assert!(body.contains("No game servers found") || body.contains("New Server"));
    })
    .await;
}

/// GET /servers renders existing server rows.
#[tokio::test]
#[serial]
async fn servers_list_with_data() {
    request::<App, _, _>(|request, ctx| async move {
        ActiveModel::create(&ctx, &make_form(730, "My CS2 Server"), None)
            .await
            .expect("Failed to create game server");

        let response = request.get("/servers").await;
        response.assert_status_success();
        let body = response.text();
        assert!(
            body.contains("My CS2 Server"),
            "Server name should appear in list"
        );
        assert!(body.contains("730"), "App ID should appear in list");
    })
    .await;
}

/// GET /servers/new renders the creation landing page.
#[tokio::test]
#[serial]
async fn servers_new_landing_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/servers/new").await;
        response.assert_status_success();

        let body = response.text();
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("Create New Server"));
        // "Create from Template" card should NOT appear when no templates exist
        assert!(!body.contains("servers-create-from-template"));
    })
    .await;
}

/// GET /servers/new/form renders the blank install form.
#[tokio::test]
#[serial]
async fn servers_new_form_page_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/servers/new/form").await;
        response.assert_status_success();

        let body = response.text();
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("Steam App ID"));
        assert!(body.contains("Server Name"));
    })
    .await;
}

/// GET /servers/:id shows 404-style error for missing server.
#[tokio::test]
#[serial]
async fn servers_show_not_found() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/servers/999999").await;
        // Should be an error (500 or 404)
        assert!(
            response.status_code().is_client_error() || response.status_code().is_server_error(),
            "Missing server should return error status"
        );
    })
    .await;
}

/// GET /servers/:id shows game server details.
#[tokio::test]
#[serial]
async fn servers_show_renders() {
    request::<App, _, _>(|request, ctx| async move {
        let model = ActiveModel::create(&ctx, &make_form(740, "CS:GO Detail Server"), None)
            .await
            .expect("Failed to create game server");

        let url = format!("/servers/{}", model.id);
        let response = request.get(&url).await;
        response.assert_status_success();

        let body = response.text();
        assert!(
            body.contains("CS:GO Detail Server"),
            "Server name should appear"
        );
        assert!(body.contains("740"), "App ID should appear");
        assert!(
            body.contains(std::env::consts::OS),
            "Platform should appear"
        );
    })
    .await;
}

/// POST /servers with invalid app_id returns error.
#[tokio::test]
#[serial]
async fn servers_create_invalid_app_id() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/servers")
            .add_header(
                axum::http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .bytes(b"app_id=notanumber&name=Test+Server".as_ref().into())
            .await;
        // Should be an error
        assert!(
            response.status_code().is_client_error() || response.status_code().is_server_error(),
            "Invalid app_id should return error"
        );
    })
    .await;
}

/// Verify CreateServerForm deserializes use_steam_login as false when absent from form.
#[tokio::test]
#[serial]
async fn servers_create_checkbox_unchecked_defaults_false() {
    use game_smith::models::game_servers::CreateServerForm;

    // Unchecked checkbox sends nothing for use_steam_login
    let data = "app_id=730&name=Test+Server";
    let form: CreateServerForm =
        serde_urlencoded::from_str(data).expect("should parse without use_steam_login");
    assert!(!form.use_steam_login, "unchecked should default to false");
}

/// Verify CreateServerForm deserializes use_steam_login as true when checkbox checked.
#[tokio::test]
#[serial]
async fn servers_create_checkbox_checked_parses_true() {
    use game_smith::models::game_servers::CreateServerForm;

    // Checked checkbox sends use_steam_login=true
    let data = "app_id=730&name=Test+Server&use_steam_login=true";
    let form: CreateServerForm =
        serde_urlencoded::from_str(data).expect("should parse with use_steam_login=true");
    assert!(form.use_steam_login, "checked should be true");
}

/// Verify CreateServerForm rejects non-true values for use_steam_login.
#[tokio::test]
#[serial]
async fn servers_create_checkbox_non_true_value_fails() {
    use game_smith::models::game_servers::CreateServerForm;

    // A non-"true" value should fail deserialization since the field expects a bool
    let data = "app_id=730&name=Test+Server&use_steam_login=on";
    let result: Result<CreateServerForm, _> = serde_urlencoded::from_str(data);
    assert!(
        result.is_err(),
        "non-true value should fail deserialization"
    );
}
/// POST /servers/:id/update must not return 500 due to missing install_dir.
///
/// Regression test: `update()` wrote `install_dir/update_{app_id}.txt` without first
/// calling `create_dir_all`, so a server whose install_dir was absent returned HTTP 500
/// with error "failed to update server: No such file or directory".
///
/// Uses a no-op fake SteamCMD so the worker completes immediately without network access.
/// Overrides both HOME and XDG_DATA_HOME so the server's computed install_dir points
/// into the temp tree (where the target subdirectory doesn't exist).
#[tokio::test]
#[serial]
async fn servers_update_does_not_500_when_install_dir_missing() {
    use game_smith::models::game_servers::ActiveModel;

    let tmp = setup_fake_steamcmd();

    // Override HOME so default_install_dir() computes a path under tmp
    // (which doesn't exist yet), and XDG_DATA_HOME for SteamCMD resolution.
    let original_home = std::env::var("HOME").ok();
    let original_xdg = std::env::var("XDG_DATA_HOME").ok();
    std::env::set_var("HOME", &tmp);
    std::env::set_var("XDG_DATA_HOME", &tmp);

    request::<App, _, _>(|request, ctx| async move {
        let model = ActiveModel::create(&ctx, &make_form(740, "Update Test Server"), None)
            .await
            .expect("failed to create server record");

        let url = format!("/servers/{}/update", model.id);
        let response = request.post(&url).await;

        // Before the fix: 500 WriteScript (install_dir not created before fs::write).
        // After the fix: create_dir_all runs first; response is redirect or other non-500.
        assert_ne!(
            response.status_code().as_u16(),
            500,
            "POST /servers/:id/update must not 500 when install_dir is missing (got WriteScript)"
        );
    })
    .await;

    // Restore environment
    match original_home {
        Some(val) => std::env::set_var("HOME", val),
        None => std::env::remove_var("HOME"),
    }
    match original_xdg {
        Some(val) => std::env::set_var("XDG_DATA_HOME", val),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
/// GET /servers/new must show the "Create from Template" card when templates exist,
/// and /servers/new/form?template_id=X must render with pre-filled data.
#[tokio::test]
#[serial]
async fn servers_new_form_renders_with_templates() {
    request::<App, _, _>(|request, ctx| async move {
        // Create a template with all fields populated
        let active = game_smith::models::game_templates::ActiveModel::create(
            &ctx,
            "Test Template".to_string(),
            Some("Test description".to_string()),
            730,
            Some("csgo".to_string()),
            Some("dev".to_string()),
            Some("run.sh".to_string()),
            true,
            true,
            true,
            true,
            true,
            Some("0 4 * * *".to_string()),
        )
        .await
        .expect("Failed to create test template");

        let template_id = active.id;

        // Landing page should show "Create from Template" card
        let landing_response = request.get("/servers/new").await;
        landing_response.assert_status_success();
        let landing_body = landing_response.text();
        assert!(
            landing_body.contains("Create from Template"),
            "Landing page should show template option when templates exist"
        );

        // Form page with template_id should render and pre-fill app_id
        let form_response = request
            .get(&format!("/servers/new/form?template_id={}", template_id))
            .await;
        form_response.assert_status_success();
        let form_body = form_response.text();
        assert!(form_body.contains("<!DOCTYPE html>"));
        assert!(
            form_body.contains("value=\"730\""),
            "Form should pre-fill app_id from template"
        );
    })
    .await;
}

/// GET /servers/new/select-template renders the template selection page.
#[tokio::test]
#[serial]
async fn servers_select_template_renders() {
    request::<App, _, _>(|request, ctx| async move {
        // With no templates, shows empty state
        let response = request.get("/servers/new/select-template").await;
        response.assert_status_success();
        let body = response.text();
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("Choose a Template"));
        assert!(body.contains("No templates available"));

        // Create a template
        let active = game_smith::models::game_templates::ActiveModel::create(
            &ctx,
            "Test Template".to_string(),
            Some("Test description".to_string()),
            730,
            Some("csgo".to_string()),
            Some("dev".to_string()),
            Some("run.sh".to_string()),
            true,
            true,
            true,
            true,
            true,
            Some("0 4 * * *".to_string()),
        )
        .await
        .expect("Failed to create test template");

        let template_id = active.id;

        // With templates, shows card with template info
        let response = request.get("/servers/new/select-template").await;
        response.assert_status_success();
        let body = response.text();
        assert!(body.contains("Test Template"));
        assert!(body.contains("Test description"));
        assert!(body.contains("730"));
        assert!(body.contains("csgo"));
        assert!(body.contains("dev"));
        assert!(body.contains("Use this template"));
        assert!(
            body.contains(&format!("/servers/new/form?template_id={}", template_id)),
            "Card button should link to form with template_id"
        );
    })
    .await;
}

/// POST /servers with a duplicate name re-renders the form with an error message
/// and the submitted data preserved.
#[tokio::test]
#[serial]
async fn servers_create_duplicate_name_renders_form_with_error() {
    request::<App, _, _>(|request, ctx| async move {
        // Create an initial server
        ActiveModel::create(&ctx, &make_form(730, "Duplicate Test Server"), None)
            .await
            .expect("Failed to create initial game server");

        // Try to create a server with the same name
        let response = request
            .post("/servers")
            .form(&[
                ("app_id", "730"),
                ("name", "Duplicate Test Server"),
                ("server_mod", ""),
                ("beta_branch", ""),
                ("use_steam_login", "false"),
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
            body.contains("Duplicate Test Server"),
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
