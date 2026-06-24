//! Desktop integration module.
//!
//! Provides system tray icon, desktop notifications, autostart, and browser auto-open.
//! Supports Linux (GTK) and Windows (Win32).

pub mod autostart;
mod notifications;
mod tray;
use tray_icon::TrayIconBuilder;
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
    #[must_use]
    pub fn from_env() -> Self {
        use std::env;

        let enabled =
            env::var("GAME_SMITH_DESKTOP_ENABLED").map_or(true, |v| v != "false" && v != "0");

        let open_browser =
            env::var("GAME_SMITH_DESKTOP_OPEN_BROWSER").map_or(true, |v| v != "false" && v != "0");

        let tray_enabled =
            env::var("GAME_SMITH_DESKTOP_TRAY_ENABLED").map_or(true, |v| v != "false" && v != "0");

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
pub struct DesktopHandle {}

/// Manages desktop integration: tray icon, notifications, browser auto-open.
pub struct DesktopManager {
    config: DesktopConfig,
    server_url: String,
}

impl DesktopManager {
    /// Returns the configured server URL.
    #[must_use]
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Creates a new desktop manager with the given configuration and server URL.
    #[must_use]
    pub const fn new(config: DesktopConfig, server_url: String) -> Self {
        Self { config, server_url }
    }
    /// Spawns the tray icon on a dedicated thread.
    ///
    /// The tray icon runs its own event loop on a background thread.
    /// Menu events are handled inline on that thread.
    ///
    /// Returns `None` if tray is disabled or creation fails.
    #[must_use]
    pub fn spawn_tray(&self) -> Option<DesktopHandle> {
        if !self.config.tray.enabled {
            return None;
        }

        let tooltip = self.config.tray.tooltip.clone();
        let server_url = self.server_url.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel::<Option<String>>(0);

        std::thread::spawn(move || {
            #[cfg(target_os = "linux")]
            {
                // Pre-check: ensure libayatana-appindicator can be loaded.
                // If LD_LIBRARY_PATH is missing (e.g. Homebrew on Linux), the
                // tray will fail silently; catch it early with a clear message.
                if let Err(e) = check_appindicator_lib() {
                    eprintln!("game-smith: {e}");
                    let _ = tx.send(Some(format!("appindicator library check failed: {e}")));
                    return;
                }

                if let Err(e) = gtk::init() {
                    eprintln!("game-smith: GTK init failed ({e}) — tray disabled");
                    let _ = tx.send(Some(format!("gtk init failed: {e}")));
                    return;
                }
            }

            let tray_state = tray::Tray::new(server_url);
            let icon = tray::Tray::icon();
            let menu = tray::Tray::menu(autostart::is_enabled().unwrap_or(false));

            let system_tray = match TrayIconBuilder::new()
                .with_id("game-smith")
                .with_tooltip(&tooltip)
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .build()
            {
                Ok(system_tray) => system_tray,
                Err(e) => {
                    eprintln!("game-smith: failed to create tray icon: {e}");
                    let _ = tx.send(Some(format!("tray build failed: {e}")));
                    return;
                }
            };

            let _ = tx.send(None);

            tray_state.run_event_loop(system_tray);
        });

        match rx.recv() {
            Ok(None) => Some(DesktopHandle {}),
            Ok(Some(e)) => {
                eprintln!("game-smith: tray failed: {e}");
                None
            }
            Err(_) => {
                eprintln!("game-smith: tray thread died unexpectedly");
                None
            }
        }
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

/// Checks if the libayatana-appindicator shared library can be loaded at runtime.
///
/// On Linux with Homebrew, the library may be installed but not in
/// `LD_LIBRARY_PATH` or the `ldconfig` cache, causing the tray to fail
/// silently. This function detects that case and returns a clear error
/// with remediation steps.
#[cfg(target_os = "linux")]
fn check_appindicator_lib() -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let lib_name = OsStr::from_bytes(b"libayatana-appindicator3.so.1");

    // Check if LD_LIBRARY_PATH contains the library
    let ld_path = std::env::var_os("LD_LIBRARY_PATH");
    let ld_dirs: Vec<_> = ld_path
        .as_deref()
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();

    for dir in ld_dirs {
        if dir.join(lib_name).exists() {
            return Ok(());
        }
    }

    // Library not in LD_LIBRARY_PATH. Check if it exists in common locations.
    let common_paths = [
        "/home/linuxbrew/.linuxbrew/lib",
        "/home/linuxbrew/.linuxbrew/opt/libayatana-appindicator/lib",
        "/usr/lib",
        "/usr/lib/x86_64-linux-gnu",
    ];

    for path in common_paths {
        let lib_path = format!("{}/{}", path, lib_name.to_string_lossy());
        if std::path::Path::new(&lib_path).exists() {
            return Err(format!(
                "libayatana-appindicator3.so.1 is installed at `{lib_path}` but not in LD_LIBRARY_PATH.\n\
                 The tray icon requires this library at runtime.\n\
                 Fix: export LD_LIBRARY_PATH={path}:$LD_LIBRARY_PATH\n\
                 Or: use 'make dev' instead of 'cargo run' (automatically sets LD_LIBRARY_PATH)."
            ));
        }
    }

    // Library not found anywhere
    Err("libayatana-appindicator3 shared library not found.\n\
         Install it via your package manager or Homebrew.\n\
         Fix: make setup"
        .to_string())
}
