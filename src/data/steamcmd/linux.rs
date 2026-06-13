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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify extract() successfully unpacks a tar.gz archive and cleans up.
    #[test]
    fn extract_unpacks_tar_gz() {
        let temp_dir =
            std::env::temp_dir().join(format!("game-smith-steamcmd-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("failed to create temp dir");

        // Create a tar.gz file containing a single file
        let tar_path = temp_dir.join("test.tar.gz");
        {
            let tar_file = std::fs::File::create(&tar_path).expect("failed to create tar file");
            let gz_encoder = flate2::write::GzEncoder::new(tar_file, flate2::Compression::fast());
            let mut archive = tar::Builder::new(gz_encoder);

            let mut headers = tar::Header::new_gnu();
            headers.set_size(5);
            headers.set_mode(0o644);
            headers.set_cksum();
            archive
                .append_data(&mut headers, "steamcmd.sh", b"FAKE\x00".as_slice())
                .expect("failed to append to archive");
            archive.finish().expect("failed to finish archive");
        }

        // Extract
        let dest_dir = temp_dir.join("steamcmd");
        std::fs::create_dir_all(&dest_dir).expect("failed to create dest dir");
        extract(&dest_dir, &tar_path).expect("extract failed");

        // Verify
        assert!(
            dest_dir.join("steamcmd.sh").exists(),
            "steamcmd.sh should exist"
        );
        assert!(!tar_path.exists(), "temp file should be cleaned up");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
