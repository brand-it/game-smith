//! Tests for desktop configuration loading.

use std::env;

use game_smith::desktop::DesktopConfig;
use serial_test::serial;

/// Helper to set env var, run a test, and restore the original value.
fn with_env<F: FnOnce()>(key: &str, value: &str, f: F) {
    let old = env::var(key).ok();
    env::set_var(key, value);
    f();
    match old {
        Some(v) => env::set_var(key, &v),
        None => env::remove_var(key),
    }
}

/// Test that config loads defaults when no env vars are set.
#[test]
#[serial]
fn loads_defaults_when_no_env_vars() {
    // Clear all relevant env vars
    for key in [
        "GAME_SMITH_DESKTOP_ENABLED",
        "GAME_SMITH_DESKTOP_OPEN_BROWSER",
        "GAME_SMITH_DESKTOP_TRAY_ENABLED",
        "GAME_SMITH_DESKTOP_TRAY_TOOLTIP",
        "GAME_SMITH_PORT",
    ] {
        env::remove_var(key);
    }

    let config = DesktopConfig::from_env();

    assert!(config.enabled, "enabled should default to true");
    assert!(config.open_browser, "open_browser should default to true");
    assert!(config.tray.enabled, "tray.enabled should default to true");
    assert_eq!(
        config.tray.tooltip, "game-smith",
        "tooltip should default to 'game-smith'"
    );
    assert_eq!(config.port, 5150, "port should default to 5150");
}

/// Test that config respects explicit disabled values.
#[test]
#[serial]
fn respects_disabled_flag() {
    with_env("GAME_SMITH_DESKTOP_ENABLED", "false", || {
        let config = DesktopConfig::from_env();
        assert!(
            !config.enabled,
            "enabled should be false when set to 'false'"
        );
    });
}

/// Test that config respects port override.
#[test]
#[serial]
fn respects_port_override() {
    with_env("GAME_SMITH_PORT", "8080", || {
        let config = DesktopConfig::from_env();
        assert_eq!(config.port, 8080, "port should be overridden to 8080");
    });
}

/// Test that invalid port falls back to default.
#[test]
#[serial]
fn invalid_port_falls_back_to_default() {
    with_env("GAME_SMITH_PORT", "not-a-number", || {
        let config = DesktopConfig::from_env();
        assert_eq!(
            config.port, 5150,
            "invalid port should fall back to default"
        );
    });
}

/// Test that tray can be independently disabled.
#[test]
#[serial]
fn tray_can_be_disabled() {
    with_env("GAME_SMITH_DESKTOP_TRAY_ENABLED", "0", || {
        let config = DesktopConfig::from_env();
        assert!(
            !config.tray.enabled,
            "tray should be disabled when set to '0'"
        );
    });
}

/// Test that browser open can be independently disabled.
#[test]
#[serial]
fn browser_open_can_be_disabled() {
    with_env("GAME_SMITH_DESKTOP_OPEN_BROWSER", "false", || {
        let config = DesktopConfig::from_env();
        assert!(
            !config.open_browser,
            "open_browser should be false when set to 'false'"
        );
    });
}

/// Test that custom tooltip is respected.
#[test]
#[serial]
fn custom_tooltip_is_respected() {
    with_env("GAME_SMITH_DESKTOP_TRAY_TOOLTIP", "My App", || {
        let config = DesktopConfig::from_env();
        assert_eq!(config.tray.tooltip, "My App");
    });
}
