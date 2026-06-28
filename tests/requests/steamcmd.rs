use game_smith::app::App;
use game_smith::data::steamcmd::DistroInfo;
use game_smith::initializers::embedded_i18n::EmbeddedViews;
use game_smith::views::steamcmd::status as render_steamcmd_status;
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

/// Verify the steamcmd status view struct has all template fields.
///
/// This is a regression test: if any field the template expects is removed
/// from the `data!` call in `views::steamcmd::status()`, Tera render fails
/// with an HTTP 500 and no useful line number.
#[tokio::test]
#[serial]
async fn steamcmd_status_view_struct_regression() {
    let _boot = boot_test::<App>().await.expect("Failed to boot test app");
    let views = EmbeddedViews::build().expect("Failed to create embedded views");

    let result = render_steamcmd_status(
        &views,
        "/home/user/.local/share/steamcmd/steamcmd", // binary_path
        true,                                        // installed
        "healthy",                                   // health_status
        None,                                        // broken_message
        None,                                        // last_check_id
        None,                                        // last_check_status
        "linux",                                     // platform
        None,                                        // distro
    );
    assert!(result.is_ok(), "Steamcmd status view should render");
}

/// Verify the steamcmd status view renders when broken_message is set.
#[tokio::test]
#[serial]
async fn steamcmd_status_view_with_broken() {
    let _boot = boot_test::<App>().await.expect("Failed to boot test app");
    let views = EmbeddedViews::build().expect("Failed to create embedded views");

    let result = render_steamcmd_status(
        &views,
        "/home/user/.local/share/steamcmd/steamcmd",
        true,
        "broken",
        Some("test error"),
        Some(1),
        Some("failed"),
        "linux",
        Some(&DistroInfo {
            label: "Arch Linux".to_string(),
            install_command: "sudo pacman -S lib32-glibc".to_string(),
        }),
    );
    assert!(
        result.is_ok(),
        "Steamcmd status view should render with broken state"
    );
}

/// Verify the steamcmd status view renders when steamcmd is not installed.
#[tokio::test]
#[serial]
async fn steamcmd_status_view_not_installed() {
    let _boot = boot_test::<App>().await.expect("Failed to boot test app");
    let views = EmbeddedViews::build().expect("Failed to create embedded views");

    let result = render_steamcmd_status(
        &views,
        "/home/user/.local/share/steamcmd/steamcmd",
        false,
        "not_installed",
        None,
        None,
        None,
        "linux",
        None,
    );
    assert!(
        result.is_ok(),
        "Steamcmd status view should render when not installed"
    );
}
