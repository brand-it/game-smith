#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use std::io::Write;

use game_smith::app::clean_stale_migrations;
use game_smith::{app::App, create_data_dirs, resolve_data_home, AppDirs};
use loco_rs::app::Hooks;
use loco_rs::boot::{create_app, start, ServeParams, StartMode};
use loco_rs::cli;
use loco_rs::Result;
use migration::Migrator;
use tracing::{error, info};

/// Determine whether the current invocation should boot the server.
/// Defaults to `start` when no subcommand is given.
fn should_start_server() -> bool {
    matches!(std::env::args().nth(1).as_deref(), Some("start") | None)
}

/// Handle autostart subcommands early (before server or CLI bootstrap).
fn handle_autostart_subcommand() -> bool {
    match std::env::args().nth(1).as_deref() {
        Some("autostart-enable") => {
            if let Err(e) = game_smith::desktop::autostart::enable() {
                error!("game-smith: failed to enable autostart: {e}");
                return true;
            }
            info!("game-smith: autostart enabled");
            true
        }
        Some("autostart-disable") => {
            if let Err(e) = game_smith::desktop::autostart::disable() {
                error!("game-smith: failed to disable autostart: {e}");
                return true;
            }
            info!("game-smith: autostart disabled");
            true
        }
        Some("autostart-is-enabled") => {
            let enabled = match game_smith::desktop::autostart::is_enabled() {
                Ok(v) => v,
                Err(e) => {
                    error!("game-smith: failed to check autostart status: {e}");
                    return true;
                }
            };
            info!(
                "game-smith: autostart is {}",
                if enabled { "enabled" } else { "disabled" }
            );
            true
        }
        _ => false,
    }
}

fn main() -> Result<()> {
    game_smith::install_panic_hook();

    // Handle autostart subcommands before any server or CLI bootstrap.
    if handle_autostart_subcommand() {
        return Ok(());
    }

    // Resolve app data directories once; reuse for dirs and boot log.
    let dirs = AppDirs::new(resolve_data_home());
    create_data_dirs(&dirs);
    write_boot_log(&dirs);

    if should_start_server() {
        run_server()
    } else {
        run_cli()
    }
}

/// Synchronous entry point for server mode.
/// Runs on thread 0 so that GTK / tray initialization happens before
/// the tokio multi-thread runtime moves execution elsewhere.
fn run_server() -> Result<()> {
    // Single-instance guard.
    let config = load_desktop_config();
    let port = config.port;
    if std::net::TcpListener::bind(format!("127.0.0.1:{port}")).is_err() {
        eprintln!("game-smith: already running on port {port}, opening browser");
        game_smith::desktop::open_url(&format!("http://127.0.0.1:{port}"));
        return Ok(());
    }

    // GTK and the tray icon MUST be initialized on thread 0, before the tokio
    // multi-thread runtime starts (which moves execution off the OS main thread).
    let _desktop_handle = if config.enabled {
        let server_url = format!("http://127.0.0.1:{}", config.port);
        let manager = game_smith::desktop::DesktopManager::new(config, server_url);
        manager.open_browser();
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
    };
    // Start the tokio runtime only after GTK/tray setup is complete.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(boot_server())
}

/// Synchronous entry point for CLI subcommands (db, task, etc.).
fn run_cli() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async { cli::main::<App, Migrator>().await })
}

/// Boot the app with the server, workers, and scheduler all running in-process.
/// The scheduler config is embedded in code via `App::load_config`.
#[allow(clippy::too_many_lines)]
async fn boot_server() -> Result<()> {
    use loco_rs::boot::create_context;

    let environment =
        loco_rs::environment::Environment::from(loco_rs::environment::resolve_from_env());
    let config = App::load_config(&environment).await?;

    clean_stale_migrations(&config.database.uri).await;

    let app_context = create_context::<App>(&environment, config).await?;

    if !App::init_logger(&app_context)? {
        loco_rs::logger::init::<App>(&app_context.config.logger)?;
    }

    let boot =
        create_app::<App, Migrator>(StartMode::ServerAndWorker, &environment, app_context.config)
            .await?;

    // Auto-start servers that have auto_start = true.
    let ctx_for_autostart = boot.app_context.clone();
    tokio::spawn(async move {
        match game_smith::models::game_servers::Model::find_auto_start(&ctx_for_autostart).await {
            Ok(servers) => {
                if servers.is_empty() {
                    return;
                }
                let mut started = 0;
                let total = servers.len();
                for server in &servers {
                    match server.start(&ctx_for_autostart).await {
                        Ok(true) => {
                            started += 1;
                            tracing::info!(server_id = server.id, server_name = %server.name, "auto-started server");
                        }
                        Ok(false) => {
                            tracing::info!(server_id = server.id, server_name = %server.name, "auto-start skipped (already running or no executable)");
                        }
                        Err(e) => {
                            tracing::error!(server_id = server.id, server_name = %server.name, err = %e, "auto-start failed");
                        }
                    }
                }
                tracing::info!(started, total, "auto-start complete");
            }
            Err(e) => {
                tracing::error!(err = %e, "failed to query auto-start servers");
            }
        }
    });

    // Spawn custom in-process scheduler.
    let ctx_for_scheduler = boot.app_context.clone();
    tokio::spawn(async move {
        if let Err(e) = game_smith::scheduler::run_scheduler(&ctx_for_scheduler).await {
            tracing::error!(err = %e, "scheduler exited with error");
        }
    });

    let serve_params = ServeParams {
        port: boot.app_context.config.server.port,
        binding: boot.app_context.config.server.binding.clone(),
    };
    let shutdown_ctx = boot.app_context.clone();

    let result = start::<App>(boot, serve_params, false).await;

    // Graceful shutdown: stop running game servers and show status page.
    let running_servers = game_smith::models::game_servers::Model::find_running(&shutdown_ctx)
        .await
        .unwrap_or_default();

    for server in &running_servers {
        if let Err(e) = server.stop(&shutdown_ctx).await {
            tracing::error!(
                server_id = server.id,
                server_name = %server.name,
                err = %e,
                "failed to stop server during shutdown"
            );
        } else {
            tracing::info!(
                server_id = server.id,
                server_name = %server.name,
                "stopped server during shutdown"
            );
        }
    }
    tracing::info!("shutdown complete");
    result
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
