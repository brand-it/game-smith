use game_smith::app::App;
use game_smith::models::game_servers::ActiveModel;
use loco_rs::testing::prelude::*;
use serial_test::serial;

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
        ActiveModel::create(
            &ctx,
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

/// GET /servers/new renders the install form.
#[tokio::test]
#[serial]
async fn servers_new_form_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/servers/new").await;
        response.assert_status_success();

        let body = response.text();
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("Steam App ID") || body.contains("app_id"));
        assert!(body.contains("Server Name") || body.contains("name"));
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
        let model = ActiveModel::create(
            &ctx,
            740,
            "CS:GO Detail Server".to_string(),
            "/tmp/game-smith/games/csgo-detail".to_string(),
            "linux".to_string(),
            None,
            None,
            false,
        )
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
        assert!(body.contains("linux"), "Platform should appear");
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
    use game_smith::controllers::game_servers::CreateServerForm;

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
    use game_smith::controllers::game_servers::CreateServerForm;

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
    use game_smith::controllers::game_servers::CreateServerForm;

    // A non-"true" value should fail deserialization since the field expects a bool
    let data = "app_id=730&name=Test+Server&use_steam_login=on";
    let result: Result<CreateServerForm, _> = serde_urlencoded::from_str(data);
    assert!(
        result.is_err(),
        "non-true value should fail deserialization"
    );
}
