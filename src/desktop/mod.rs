//! Desktop integration module.
//!
//! Provides system tray icon, desktop notifications, and browser auto-open.
//! Conditionally compiled behind the `desktop` Cargo feature.

mod notifications;

use std::process;

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

/// Menu item identifier for the "Open Dashboard" action.
const MENU_OPEN: &str = "open";
/// Menu item identifier for the "Quit" action.
const MENU_QUIT: &str = "quit";

/// Configuration for the desktop integration layer.
#[derive(Debug, Clone)]
pub struct DesktopConfig {
    /// Whether desktop features are enabled.
    pub enabled: bool,
    /// Whether to auto-open the browser on startup.
    pub open_browser: bool,
    /// Tray icon configuration.
    pub tray: TrayConfig,
    /// Server port (used to construct the local URL).
    pub port: u16,
}

/// Tray icon settings.
#[derive(Debug, Clone)]
pub struct TrayConfig {
    /// Whether the tray icon is enabled.
    pub enabled: bool,
    /// Tooltip text shown when hovering over the tray icon.
    pub tooltip: String,
}

impl DesktopConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        use std::env;

        let enabled = env::var("GAME_SMITH_DESKTOP_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let open_browser = env::var("GAME_SMITH_DESKTOP_OPEN_BROWSER")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let tray_enabled = env::var("GAME_SMITH_DESKTOP_TRAY_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let tooltip = env::var("GAME_SMITH_DESKTOP_TRAY_TOOLTIP")
            .unwrap_or_else(|_| "game-smith".to_string());

        let port: u16 = env::var("GAME_SMITH_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5150);

        Self {
            enabled,
            open_browser,
            tray: TrayConfig {
                enabled: tray_enabled,
                tooltip,
            },
            port,
        }
    }
}

/// Handle returned when spawning the tray icon.
///
/// The tray icon remains alive as long as this handle is held.
/// Dropping this handle removes the tray icon from the system tray.
pub struct DesktopHandle {
    _tray_icon: TrayIcon,
}

/// Manages desktop integration: tray icon, notifications, browser auto-open.
pub struct DesktopManager {
    config: DesktopConfig,
    server_url: String,
}

impl DesktopManager {
    /// Returns the configured server URL.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Creates a new desktop manager with the given configuration and server URL.
    pub fn new(config: DesktopConfig, server_url: String) -> Self {
        Self { config, server_url }
    }

    /// Spawns the tray icon on a dedicated thread.
    ///
    /// The tray icon runs its own event loop on a background thread.
    /// Menu events are handled inline on that thread.
    ///
    /// Returns `None` if tray is disabled or creation fails.
    pub fn spawn_tray(&self) -> Option<DesktopHandle> {
        if !self.config.tray.enabled {
            return None;
        }

        // GTK must be initialized on the main thread before creating UI elements
        gtk::init().expect("failed to initialize GTK");

        let tooltip = self.config.tray.tooltip.clone();
        let server_url = self.server_url.clone();

        let icon = create_icon();
        let menu = build_menu();

        let tray = TrayIconBuilder::new()
            .with_tooltip(&tooltip)
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .build()
            .inspect_err(|e| tracing::error!(error = %e, "failed to create tray icon"))
            .ok()?;

        // Spawn a thread to poll menu events.
        std::thread::spawn(move || {
            let rx = MenuEvent::receiver();
            for event in rx.iter() {
                match event.id.as_ref() {
                    MENU_OPEN => {
                        let _ = open::that(&server_url);
                    }
                    MENU_QUIT => {
                        process::exit(0);
                    }
                    _ => {}
                }
            }
        });

        Some(DesktopHandle {
            _tray_icon: tray,
        })
    }

    /// Opens the default browser to the server URL.
    ///
    /// This spawns a background thread that delays opening the browser
    /// by 2 seconds to allow the server time to bind its TCP listener.
    pub fn open_browser(&self) {
        if !self.config.open_browser {
            return;
        }

        let url = self.server_url.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Err(e) = open::that(&url) {
                tracing::warn!(error = %e, url = %url, "failed to open browser");
            }
        });
    }

    /// Shows a desktop notification.
    pub fn notify(&self, title: &str, message: &str) {
        notifications::notify(title, message);
    }
}

/// Builds the tray context menu with "Open Dashboard" and "Quit" items.
fn build_menu() -> Menu {
    let open_item = MenuItem::with_id(
        MENU_OPEN,
        "Open Dashboard",
        true,
        None,
    );
    let quit_item = MenuItem::with_id(
        MENU_QUIT,
        "Quit",
        true,
        None,
    );

    let menu = Menu::new();
    let _ = menu.append_items(&[&open_item, &quit_item]);

    menu
}

/// Creates a procedural 32x32 tray icon (simple white circle on dark background).
fn create_icon() -> Icon {
    let size = 32;
    let mut pixels = vec![0u8; (size * size * 4) as usize];

    let center = size / 2;
    let radius = size / 2 - 2;

    for y in 0..size {
        for x in 0..size {
            let dx = x as i32 - center as i32;
            let dy = y as i32 - center as i32;
            let dist_sq = dx * dx + dy * dy;
            let in_circle = dist_sq <= (radius as i32) * (radius as i32);

            let idx = (y * size + x) as usize * 4;
            if in_circle {
                pixels[idx] = 220;     // R
                pixels[idx + 1] = 225; // G
                pixels[idx + 2] = 255; // B
                pixels[idx + 3] = 255; // A
            } else {
                pixels[idx] = 30;      // R
                pixels[idx + 1] = 30;  // G
                pixels[idx + 2] = 40;  // B
                pixels[idx + 3] = 255; // A
            }
        }
    }

    Icon::from_rgba(pixels, size, size).expect("failed to create procedural icon")
}
