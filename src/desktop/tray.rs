//! Tray icon implementation.
//!
//! Encapsulates menu constants, icon creation, menu building, and
//! platform-specific event loops. This module is an internal implementation
//! detail of the desktop integration layer.

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon,
};

/// Menu item identifier for the "Open Dashboard" action.
const MENU_OPEN: &str = "open";
/// Menu item identifier for the "Quit" action.
const MENU_QUIT: &str = "quit";

/// Tray icon state and configuration.
///
/// Owns the tooltip text and server URL needed for menu event handling.
/// Provides methods to build the icon, menu, and run the event loop.
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
                let in_circle = dist_sq <= radius_sq;

                let idx = (y * size + x) as usize * 4;
                if in_circle {
                    pixels[idx] = 220;
                    pixels[idx + 1] = 225;
                    pixels[idx + 2] = 255;
                } else {
                    pixels[idx] = 30;
                    pixels[idx + 1] = 30;
                    pixels[idx + 2] = 40;
                }
                pixels[idx + 3] = 255;
            }
        }

        Icon::from_rgba(pixels, size, size).expect("failed to create procedural icon")
    }

    /// Builds the context menu with "Open Dashboard" and "Quit" items.
    #[must_use]
    pub fn menu() -> Menu {
        let open_item = MenuItem::with_id(MENU_OPEN, "Open Dashboard", true, None);
        let quit_item = MenuItem::with_id(MENU_QUIT, "Quit", true, None);

        let menu = Menu::new();
        let _ = menu.append_items(&[&open_item, &quit_item]);

        menu
    }

    /// Runs the platform-specific event loop on the current thread.
    ///
    /// On Linux this drives the GTK main loop; on Windows this runs a
    /// Win32 message pump. Menu events are drained from the channel and
    /// dispatched to [`Self::dispatch`].
    ///
    /// This method diverges and never returns.
    #[cfg(target_os = "linux")]
    pub fn run_event_loop(self, tray: TrayIcon) -> ! {
        let rx_menu = MenuEvent::receiver().clone();
        let server_url = self.server_url;
        glib::idle_add(move || {
            while let Ok(event) = rx_menu.try_recv() {
                Self::dispatch(&event, &server_url);
            }
            glib::ControlFlow::Continue
        });
        let _keep_alive = tray;
        gtk::main();
        unreachable!("gtk::main() should not return")
    }

    #[cfg(target_os = "windows")]
    pub fn run_event_loop(self, tray: TrayIcon) -> ! {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
        };

        let rx_menu = MenuEvent::receiver().clone();
        let server_url = self.server_url;
        let _keep_alive = tray;
        let mut msg = unsafe { std::mem::zeroed::<MSG>() };
        loop {
            while let Ok(event) = rx_menu.try_recv() {
                Self::dispatch(&event, &server_url);
            }
            if unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() } {
                let _ = unsafe { TranslateMessage(&msg) };
                unsafe { DispatchMessageW(&msg) };
            } else {
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    }

    /// Dispatch a menu event using the given server URL.
    ///
    /// Used inside event-loop closures where `self` has been moved.
    fn dispatch(event: &MenuEvent, server_url: &str) {
        use std::process;

        match event.id.as_ref() {
            MENU_OPEN => {
                let _ = open::that(server_url);
            }
            MENU_QUIT => {
                process::exit(0);
            }
            _ => {}
        }
    }
}
