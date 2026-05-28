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

/// Verify the checkbox deserializer treats unchecked form (single "false") as false.
#[tokio::test]
#[serial]
async fn servers_create_checkbox_unchecked_deserializes_false() {
    use game_smith::controllers::game_servers::deserialize_checkbox;

    #[derive(Debug, serde::Deserialize)]
    struct CheckboxTest {
        #[serde(deserialize_with = "deserialize_checkbox")]
        value: bool,
    }

    // Single value "false" (unchecked: only hidden field submitted)
    let json = serde_json::json!({"value": "false"});
    let parsed: CheckboxTest = serde_json::from_value(json).expect("should parse single false");
    assert!(!parsed.value);
}

/// Verify the checkbox deserializer treats checked form (["false","true"]) as true.
#[tokio::test]
#[serial]
async fn servers_create_checkbox_checked_deserializes_true() {
    use game_smith::controllers::game_servers::deserialize_checkbox;

    #[derive(Debug, serde::Deserialize)]
    struct CheckboxTest {
        #[serde(deserialize_with = "deserialize_checkbox")]
        value: bool,
    }

    // Duplicate keys from hidden+checkbox become array: last value "true" wins
    let json = serde_json::json!({"value": ["false", "true"]});
    let parsed: CheckboxTest = serde_json::from_value(json).expect("should parse checked form");
    assert!(parsed.value);
}

/// Verify the checkbox deserializer treats "on" as false (not "true").
#[tokio::test]
#[serial]
async fn servers_create_checkbox_on_value_treated_as_false() {
    use game_smith::controllers::game_servers::deserialize_checkbox;

    #[derive(Debug, serde::Deserialize)]
    struct CheckboxTest {
        #[serde(deserialize_with = "deserialize_checkbox")]
        value: bool,
    }

    // Legacy checkbox sends "on"; should be treated as false since it's not "true"
    let json = serde_json::json!({"value": ["false", "on"]});
    let parsed: CheckboxTest =
        serde_json::from_value(json).expect("should parse but treat 'on' as false");
    assert!(!parsed.value);
}
