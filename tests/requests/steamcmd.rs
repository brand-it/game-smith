use game_smith::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// Verify that `GET /steamcmd` renders the status page without errors.
#[tokio::test]
#[serial]
async fn steamcmd_status_renders() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/steamcmd").await;
        response.assert_status_success();

        let body = response.text();

        // Verify base layout is applied
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("SteamCMD"));

        // The page should show one of the two states
        let shows_installed = body.contains("is installed");
        let shows_not_installed = body.contains("is not installed");

        assert!(
            shows_installed || shows_not_installed,
            "Page should show either installed or not installed state, but showed neither.\nBody preview: {}",
            &body[..body.len().min(500)]
        );
    })
    .await;
}

/// Verify that `POST /steamcmd/install` returns a redirect on success or
/// an error on failure. The redirect code is 303 (See Other) for POST
/// redirects per the HTTP spec.
#[tokio::test]
#[serial]
async fn steamcmd_install_endpoint_exists() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.post("/steamcmd/install").await;

        let status = response.status_code();
        let body = response.text();

        // The endpoint should exist (not 404). It may return 303 (redirect on
        // success) or 500 (installation failed due to network/permissions).
        assert!(
            status == 303 || status == 500,
            "Expected redirect (303) or error (500), got {status}. Body: {body}"
        );
    })
    .await;
}
