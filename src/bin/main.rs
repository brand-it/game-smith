use game_smith::app::App;
use loco_rs::cli;
use migration::Migrator;
use std::io::Write;

fn main() -> loco_rs::Result<()> {
    game_smith::install_panic_hook();
    ensure_app_data_dirs();
    write_boot_log();

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

fn ensure_app_data_dirs() {
    let data_home = std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| format!("{}/.local/share", std::env::var("HOME").unwrap_or_default()));
    let app_dir = std::path::PathBuf::from(&data_home).join("game-smith");

    // Ensure XDG_DATA_HOME is set so Tera config templates resolve correctly.
    // SAFETY: single-threaded, before tokio runtime.
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("XDG_DATA_HOME", &data_home);
    }

    // Create logs dir so the file appender doesn't emit a warning on first boot.
    let logs_dir = app_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).expect("failed to create logs dir");

    // Generate and persist a JWT secret if one isn't already set.
    if std::env::var("GAME_SMITH_JWT_SECRET").is_ok() {
        return;
    }

    let secret_path = app_dir.join("secret_key");

    let secret = if secret_path.exists() {
        std::fs::read_to_string(&secret_path)
            .expect("failed to read secret_key file")
            .trim()
            .to_string()
    } else {
        let new_secret = uuid::Uuid::new_v4().to_string() + "-" + &uuid::Uuid::new_v4().to_string();
        std::fs::write(&secret_path, &new_secret).expect("failed to write secret_key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600))
                .expect("failed to set secret_key permissions");
        }
        eprintln!(
            "game-smith: generated new JWT secret at {}",
            secret_path.display()
        );
        new_secret
    };

    // SAFETY: called before tokio runtime starts; process is single-threaded here.
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("GAME_SMITH_JWT_SECRET", secret);
    }
}

/// Write a one-line boot record to a plain text file before anything else
/// initializes. This gives us a breadcrumb even when tracing/journald miss it.
fn write_boot_log() {
    let data_home = std::env::var("XDG_DATA_HOME").unwrap_or_default();
    if data_home.is_empty() {
        return;
    }
    let path = std::path::PathBuf::from(&data_home)
        .join("game-smith")
        .join("boot.log");
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
