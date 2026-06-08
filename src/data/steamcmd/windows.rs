use std::path::Path;

use tracing::info;

/// Name of the Windows steamcmd binary.
pub const BINARY_NAME: &str = "steamcmd.exe";

/// Download URL for the Windows steamcmd archive.
pub const DOWNLOAD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";

/// Temporary file name for the Windows steamcmd download.
pub const TEMP_FILE_NAME: &str = "steamcmd.zip";

/// Extract a downloaded zip archive into the steamcmd directory.
pub fn extract(steamcmd_dir: &Path, temp_path: &Path) -> Result<(), super::SteamCmdError> {
    info!(path = %temp_path.display(), "Extracting SteamCMD zip archive...");
    let bytes = std::fs::read(temp_path).map_err(super::SteamCmdError::Io)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| {
        super::SteamCmdError::Extract(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    archive.extract(steamcmd_dir).map_err(|e| {
        super::SteamCmdError::Extract(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    // Clean up temp file
    let _ = std::fs::remove_file(temp_path);
    let _ = std::fs::remove_dir(steamcmd_dir.join("temp"));

    Ok(())
}
