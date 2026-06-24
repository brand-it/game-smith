//! Tray icon implementation.
//!
//! Encapsulates menu constants, icon creation, menu building,
//! and platform-specific event loops.

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::run_event_loop;
#[cfg(target_os = "windows")]
pub use windows::run_event_loop;

/// Menu item identifier for the "Open Dashboard" action.
const MENU_OPEN: &str = "open";
/// Menu item identifier for toggling autostart.
const MENU_AUTOSTART: &str = "autostart";
/// Menu item identifier for the "Quit" action.
const MENU_QUIT: &str = "quit";

/// Tray icon state and configuration.
///
/// Owns the tooltip text and server URL needed
/// for menu event handling.
pub struct Tray {
    server_url: String,
}

impl Tray {
    /// Creates a new tray with the given server URL.
    #[must_use]
    pub const fn new(server_url: String) -> Self {
        Self { server_url }
    }
    /// Creates a procedural 32x32 icon (white circle on dark background).
    #[must_use]
    pub fn icon() -> Icon {
        let size = 32u32;
        let mut pixels = vec![0u8; (size * size * 4) as usize];

        let center = size / 2;
        let radius = size / 2 - 2;
        let radius_sq = radius * radius;

        for y in 0..size {
            for x in 0..size {
                let dx = x.abs_diff(center);
                let dy = y.abs_diff(center);
                let dist_sq = dx * dx + dy * dy;

                let idx = (y * size + x) as usize * 4;
                if dist_sq <= radius_sq {
                    pixels[idx] = 255;
                    pixels[idx + 1] = 255;
                    pixels[idx + 2] = 255;
                    pixels[idx + 3] = 255;
                } else {
                    pixels[idx] = 0;
                    pixels[idx + 1] = 0;
                    pixels[idx + 2] = 0;
                    pixels[idx + 3] = 0;
                }
            }
        }

        Icon::from_rgba(pixels, size, size).expect("failed to create procedural icon")
    }

    /// Builds the context menu with "Open Dashboard" and "Quit" items.
    ///
    /// The `autostart_enabled` parameter controls the label shown for the
    /// autostart toggle so the user can see the current state at a glance.
    #[must_use]
    pub fn menu(autostart_enabled: bool) -> Menu {
        let open_item = MenuItem::with_id(MENU_OPEN, "Open Dashboard", true, None);
        let autostart_label = if autostart_enabled {
            "Autostart: Enabled"
        } else {
            "Autostart: Disabled"
        };
        let autostart_item = MenuItem::with_id(MENU_AUTOSTART, autostart_label, true, None);
        let quit_item = MenuItem::with_id(MENU_QUIT, "Quit", true, None);

        let menu = Menu::new();
        let _ = menu.append_items(&[&open_item, &autostart_item, &quit_item]);

        menu
    }

    /// Runs the platform-specific event loop on the current thread.
    ///
    /// On Linux this drives the GTK main loop; on Windows this runs a
    /// Win32 message pump. Menu events are drained from the channel and
    /// dispatched to [`Self::dispatch`].
    ///
    /// This method diverges and never returns.
    pub fn run_event_loop(self, tray: TrayIcon) -> ! {
        run_event_loop(self.server_url, tray)
    }
}

/// Dispatch a menu event using the given server URL and tray icon.
///
/// Used inside event-loop closures where `self` has been moved.
fn dispatch_menu(event: &MenuEvent, server_url: &str, tray: &TrayIcon) {
    match event.id.as_ref() {
        MENU_OPEN => {
            let _ = open::that(server_url);
        }
        MENU_QUIT => {
            // Open the /shutdown URL directly — the controller handles
            // stopping game servers and exiting the process.
            let shutdown_url = format!("{server_url}/shutdown");
            let _ = open::that(&shutdown_url);
        }
        MENU_AUTOSTART => {
            let was_enabled = super::autostart::is_enabled().unwrap_or(false);
            let result = if was_enabled {
                super::autostart::disable()
            } else {
                super::autostart::enable()
            };
            // Re-query the actual state after the operation to avoid
            // assuming the toggle succeeded.
            let now_enabled = super::autostart::is_enabled().unwrap_or(was_enabled);
            match result {
                Ok(()) => {
                    eprintln!(
                        "game-smith: autostart {}",
                        if now_enabled { "enabled" } else { "disabled" }
                    );
                }
                Err(e) => {
                    eprintln!("game-smith: failed to toggle autostart: {e}");
                }
            }
            // Rebuild the menu so the autostart label reflects the new state.
            tray.set_menu(Some(Box::new(Tray::menu(now_enabled))));
        }
        _ => {}
    }
}
