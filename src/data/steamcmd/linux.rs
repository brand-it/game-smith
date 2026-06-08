use std::path::Path;

use flate2::read::GzDecoder;
use tracing::info;

/// Name of the Linux steamcmd binary.
pub const BINARY_NAME: &str = "steamcmd.sh";

/// Download URL for the Linux steamcmd archive.
pub const DOWNLOAD_URL: &str =
    "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz";

/// Temporary file name for the Linux steamcmd download.
pub const TEMP_FILE_NAME: &str = "steamcmd.tar.gz";

/// Extract a downloaded tar.gz archive into the steamcmd directory.
pub fn extract(steamcmd_dir: &Path, temp_path: &Path) -> Result<(), super::SteamCmdError> {
    info!(path = %temp_path.display(), "Extracting SteamCMD tar archive...");
    let bytes = std::fs::read(temp_path).map_err(super::SteamCmdError::Io)?;

    let decoder = GzDecoder::new(bytes.as_slice());
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(steamcmd_dir)
        .map_err(super::SteamCmdError::Extract)?;

    // Clean up temp file
    let _ = std::fs::remove_file(temp_path);
    let _ = std::fs::remove_dir(steamcmd_dir.join("temp"));

    Ok(())
}
