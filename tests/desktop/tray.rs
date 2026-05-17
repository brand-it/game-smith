//! Tests for tray icon creation.

/// Verify that the tray icon can be created under a virtual display.
///
/// This test would have caught the GTK initialization bug where the
/// tray icon panicked because GTK wasn't initialized before creating
/// the menu.
#[test]
#[cfg(feature = "desktop")]
fn gtk_initializes_before_tray() {
    // This test verifies that GTK is initialized before creating
    // the tray icon. If GTK isn't initialized first, the tray
    // icon creation will panic.
    use game_smith::desktop::{DesktopConfig, DesktopManager};

    let config = DesktopConfig {
        enabled: true,
        open_browser: false,
        tray: game_smith::desktop::TrayConfig {
            enabled: true,
            tooltip: "test".to_string(),
        },
        port: 5150,
    };
    let manager = DesktopManager::new(config, "http://localhost:5150".to_string());

    // This should not panic with "GTK has not been initialized"
    // Run this test under xvfb-run to provide a virtual display
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        manager.spawn_tray();
    }));

    // If we got a panic, it means GTK wasn't initialized
    assert!(
        result.is_ok(),
        "Tray icon creation panicked. This likely means GTK was not initialized before creating the tray icon."
    );
}
