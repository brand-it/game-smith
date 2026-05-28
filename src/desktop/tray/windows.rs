//! Windows event loop for the system tray icon.
//!
//! Runs a Win32 `GetMessageW` message pump to process tray events.

use std::mem::MaybeUninit;

use tray_icon::menu::MenuEvent;
use tray_icon::TrayIcon;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

use super::dispatch_menu;

/// Runs the Win32 event loop on the current thread.
///
/// `GetMessageW` blocks until a window message is posted (e.g., a tray
/// menu click), then we drain the menu event channel and dispatch.
pub fn run_event_loop(server_url: String, tray: TrayIcon) -> ! {
    let rx_menu = MenuEvent::receiver().clone();
    let _keep_alive = tray;

    let mut msg = unsafe { MaybeUninit::<MSG>::zeroed().assume_init() };
    loop {
        while let Ok(event) = rx_menu.try_recv() {
            dispatch_menu(&event, &server_url);
        }
        // Blocks until a window message is posted.
        // Returns `FALSE` only on WM_QUIT — unreachable here.
        let _ = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        let _ = unsafe { TranslateMessage(&msg) };
        unsafe { DispatchMessageW(&msg) };
    }
}
