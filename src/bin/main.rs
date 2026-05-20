use game_smith::{app::App, create_data_dirs, resolve_data_home, AppDirs};
use loco_rs::cli;
use migration::Migrator;
use std::io::Write;

fn main() -> loco_rs::Result<()> {
    game_smith::install_panic_hook();

    // Resolve app data directories once; reuse for dirs and boot log.
    let dirs = AppDirs::new(resolve_data_home());
    create_data_dirs(&dirs);
    write_boot_log(&dirs);

    // Single-instance guard — must run before GTK and before tokio.
    let config = load_desktop_config();
    let port = config.port;
    if std::net::TcpListener::bind(format!("127.0.0.1:{port}")).is_err() {
        eprintln!("game-smith: already running on port {port}, opening browser");
        let _ = open::that(format!("http://127.0.0.1:{port}"));
        return Ok(());
    }

    // GTK and the tray icon MUST be initialized on thread 0, before the tokio
    // multi-thread runtime starts (which moves execution off the OS main thread).
    let _desktop_handle = {
        let config = load_desktop_config();
        if config.enabled {
            let server_url = format!("http://127.0.0.1:{}", config.port);
            let manager = game_smith::desktop::DesktopManager::new(config, server_url);
            // Only auto-open the browser when actually starting the server.
            let is_start = std::env::args().nth(1).as_deref() == Some("start");
            if is_start {
                manager.open_browser();
            }
            let handle = manager.spawn_tray();
            eprintln!(
                "game-smith: tray handle = {}",
                if handle.is_some() {
                    "Some (created)"
                } else {
                    "None (failed)"
                }
            );
            handle
        } else {
            eprintln!("game-smith: desktop disabled");
            None
        }
    };

    // Start the tokio runtime only after GTK/tray setup is complete.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async { cli::main::<App, Migrator>().await })
}

/// Write a one-line boot record to a plain text file before anything else
/// initializes. This gives us a breadcrumb even when tracing/journald miss it.
fn write_boot_log(dirs: &AppDirs) {
    let path = dirs.app_dir.join("boot.log");
    let line = format!(
        "{} PID={} DISPLAY={} WAYLAND={} LOCO_ENV={}\n",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        std::process::id(),
        std::env::var("DISPLAY").unwrap_or_else(|_| "(unset)".into()),
        std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "(unset)".into()),
        std::env::var("LOCO_ENV").unwrap_or_else(|_| "(unset)".into()),
    );
    // Append so we keep a history of boots.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

fn load_desktop_config() -> game_smith::desktop::DesktopConfig {
    game_smith::desktop::DesktopConfig::from_env()
}
