use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Initializer},
    Result,
};
use tracing::info;

use crate::{data::steamcmd::SteamCmd, resolve_data_home, AppDirs};

/// Initializer that checks if `SteamCMD` is installed at application boot.
///
/// Logs a warning if the binary is missing so that the UI can prompt the
/// user to install it. Installation is triggered through the web interface,
/// not automatically at boot.
#[allow(clippy::module_name_repetitions)]
pub struct SteamCmdInstaller;

#[async_trait]
impl Initializer for SteamCmdInstaller {
    fn name(&self) -> String {
        "steamcmd-check".to_string()
    }

    async fn before_run(&self, _ctx: &AppContext) -> Result<()> {
        let data_home = resolve_data_home();
        let dirs = AppDirs::new(data_home);
        let steamcmd = SteamCmd::new(&dirs);

        if steamcmd.is_installed() {
            info!(path = %steamcmd.binary_path().display(), "SteamCMD binary found");
        } else {
            tracing::warn!(path = %steamcmd.binary_path().display(), "SteamCMD binary not found — visit /steamcmd to install");
        }

        Ok(())
    }
}
