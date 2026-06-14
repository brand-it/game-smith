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

/// Attempt to install SteamCMD dependencies on Windows.
impl super::SteamCmd {
    /// Windows does not require additional 32-bit libraries for SteamCMD.
    /// This function exists for API parity with the Linux implementation so
    /// [`super::SteamCmd::install`] can call it without platform-specific branching.
    pub(super) async fn try_install_dependencies(&self) {
        // No dependencies to install on Windows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_result;
    use std::io::Write;

    /// Verify extract() successfully unpacks a zip archive and cleans up.
    #[test]
    fn extract_unpacks_zip() {
        let temp_dir =
            std::env::temp_dir().join(format!("game-smith-steamcmd-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("failed to create temp dir");

        // Create a zip file containing a single file
        let zip_path = temp_dir.join("test.zip");
        {
            let zip_file = std::fs::File::create(&zip_path).expect("failed to create zip file");
            let mut archive = zip::ZipWriter::new(zip_file);
            archive
                .start_file("steamcmd.exe", zip::write::FileOptions::<()>::default())
                .expect("failed to start file");
            archive
                .write_all(b"FAKE\x00")
                .expect("failed to write to archive");
            archive.finish().expect("failed to finish archive");
        }

        // Extract
        let dest_dir = temp_dir.join("steamcmd");
        std::fs::create_dir_all(&dest_dir).expect("failed to create dest dir");
        extract(&dest_dir, &zip_path).expect("extract failed");

        // Verify
        assert!(
            dest_dir.join("steamcmd.exe").exists(),
            "steamcmd.exe should exist"
        );
        assert!(!zip_path.exists(), "temp file should be cleaned up");

        log_result(
            std::fs::remove_dir_all(&temp_dir),
            &format!("cleaned up temp dir {}", temp_dir.display()),
            &format!("failed to clean up temp dir {}", temp_dir.display()),
        );
    }

    /// Verify try_install_dependencies() is callable and returns immediately (no-op on Windows).
    #[tokio::test]
    async fn try_install_dependencies_returns_immediately() {
        let temp_dir = std::env::temp_dir().join(format!("game-smith-test-{}", std::process::id()));
        log_result(
            std::fs::remove_dir_all(&temp_dir),
            &format!("cleaned up pre-existing temp dir {}", temp_dir.display()),
            &format!(
                "failed to clean up pre-existing temp dir {}",
                temp_dir.display()
            ),
        );
        std::fs::create_dir_all(&temp_dir).expect("failed to create temp dir");

        let steamcmd = super::super::SteamCmd {
            steamcmd_dir: temp_dir.clone(),
            binary_path: temp_dir.join(BINARY_NAME),
            model: None,
        };

        steamcmd.try_install_dependencies().await;

        log_result(
            std::fs::remove_dir_all(&temp_dir),
            &format!("cleaned up temp dir {}", temp_dir.display()),
            &format!("failed to clean up temp dir {}", temp_dir.display()),
        );
    }
}
