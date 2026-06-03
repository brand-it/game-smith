//! Cross-platform "start on boot" autostart management.
//!
//! Uses the [`auto-launch`] crate to handle platform-specific details:
//! - **Linux**: XDG autostart `.desktop` file in `~/.config/autostart/`.
//! - **Windows**: Registry entry under `HKCU\...\Run`.

use auto_launch::{AutoLaunchBuilder, LinuxLaunchMode, WindowsEnableMode};

/// Returns an [`AutoLaunch`] instance configured for game-smith.
fn launcher() -> Result<auto_launch::AutoLaunch, Box<dyn std::error::Error>> {
    let binary_path = std::env::current_exe()?;
    let path_str = binary_path.to_string_lossy();

    AutoLaunchBuilder::new()
        .set_app_name("Game Smith")
        .set_app_path(&path_str)
        .set_args(&["start"])
        .set_linux_launch_mode(LinuxLaunchMode::XdgAutostart)
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .build()
        .map_err(Into::into)
}

/// Enable the autostart entry so game-smith launches on boot.
///
/// Resolves the binary path via [`std::env::current_exe`] so the entry works
/// regardless of install location. The `start` argument is passed so the
/// autostarted instance boots the server (tray icon, etc.).
///
/// # Errors
/// Returns an error if the binary path cannot be resolved or the platform
/// fails to write the autostart entry.
pub fn enable() -> Result<(), Box<dyn std::error::Error>> {
    let l = launcher()?;
    l.enable().map_err(Into::into)
}

/// Disable (remove) the autostart entry.
///
/// # Errors
/// Returns an error if the autostart entry cannot be located or removed.
pub fn disable() -> Result<(), Box<dyn std::error::Error>> {
    let l = launcher()?;
    l.disable().map_err(Into::into)
}

/// Check whether the autostart entry is currently enabled.
///
/// # Errors
/// Returns an error if the autostart entry cannot be inspected.
pub fn is_enabled() -> Result<bool, Box<dyn std::error::Error>> {
    launcher()?.is_enabled().map_err(Into::into)
}
