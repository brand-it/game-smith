//! Linux event loop for the system tray icon.
//!
//! Drives the GTK main loop with a glib idle callback to dispatch
//! tray menu events.

use tray_icon::menu::MenuEvent;
use tray_icon::TrayIcon;

use super::dispatch_menu;

/// Runs the GTK event loop on the current thread.
///
/// Registers a glib idle callback to drain and dispatch menu events,
/// then blocks on `gtk::main()`.
pub fn run_event_loop(server_url: String, tray: TrayIcon) -> ! {
    let rx_menu = MenuEvent::receiver().clone();
    let tray_ref = tray.clone();
    // Use `idle_add_local` instead of `idle_add` to avoid the `Send` bound
    // (TrayIcon is !Send due to GTK thread affinity).
    glib::idle_add_local(move || {
        while let Ok(event) = rx_menu.try_recv() {
            dispatch_menu(&event, &server_url, &tray_ref);
        }
        glib::ControlFlow::Continue
    });
    let _keep_alive = tray;
    gtk::main();
    unreachable!("gtk::main() should not return")
}
