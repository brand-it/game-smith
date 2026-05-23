//! Tests for tray icon creation.
//!
//! Linux-only — requires GTK and a virtual display (xvfb-run).

/// Verify that the tray icon can be created under a virtual display.
///
/// This test would have caught the GTK initialization bug where the
/// tray icon panicked because GTK wasn't initialized before creating
/// the menu.
#[test]
#[cfg(target_os = "linux")]
fn gtk_initializes_before_tray() {
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
        let _ = manager.spawn_tray();
    }));

    assert!(
        result.is_ok(),
        "Tray icon creation panicked. This likely means GTK was not initialized before creating the tray icon."
    );
}
