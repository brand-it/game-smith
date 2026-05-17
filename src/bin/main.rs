use game_smith::app::App;
use loco_rs::cli;
use migration::Migrator;

#[tokio::main]
async fn main() -> loco_rs::Result<()> {
    // Initialize desktop integration before starting the server.
    #[cfg(feature = "desktop")]
    {
        let config = load_desktop_config();
        if config.enabled {
            let server_url = format!("http://localhost:{}", config.port);
            let manager = game_smith::desktop::DesktopManager::new(config, server_url.clone());
            manager.spawn_tray();
            manager.open_browser();
        }
    }

    cli::main::<App, Migrator>().await
}

#[cfg(feature = "desktop")]
fn load_desktop_config() -> game_smith::desktop::DesktopConfig {
    game_smith::desktop::DesktopConfig::from_env()
}
