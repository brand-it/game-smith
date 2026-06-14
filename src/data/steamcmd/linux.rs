use std::path::Path;

use flate2::read::GzDecoder;
use tracing::{info, warn};

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

/// Attempt to install the 32-bit shared libraries required by `SteamCMD` on Linux.
impl super::SteamCmd {
    /// Detects the available package manager (`apt-get` or `yum`) and runs
    /// the install command without sudo. This is best-effort: the function
    /// always returns `()` regardless of outcome. Progress is logged via
    /// `tracing` and written to the command run log file via `model.log_write()`.
    ///
    /// The definitive check for whether `SteamCMD` can run is
    /// [`super::SteamCmd::is_installed_with_deps`], which runs the binary
    /// and checks the exit code.
    pub(super) async fn try_install_dependencies(&self) {
        const MANUAL_URL: &str = "https://developer.valvesoftware.com/wiki/SteamCMD#Manually";

        let result: Result<String, String> = async {
            // Try apt-get first (Debian/Ubuntu)
            {
                let cmd_str = "apt-get install -y lib32gcc-s1";
                info!(
                    cmd = cmd_str,
                    "Attempting to install SteamCMD dependencies via apt-get..."
                );
                if let Some(ref m) = self.model {
                    m.log_write(&format!("Attempting: {cmd_str}")).await;
                }
                match run_command("apt-get", &["install", "-y", "lib32gcc-s1"]).await {
                    Ok(()) => {
                        if let Some(ref m) = self.model {
                            m.log_write("Dependencies installed successfully via apt-get")
                                .await;
                        }
                        return Ok("apt-get".to_string());
                    }
                    Err(DepsError::NotFound { .. }) => {
                        // apt-get isn't installed — fall through to yum.
                    }
                    Err(e) => {
                        // apt-get exists but couldn't install — no point trying yum.
                        return Err(e.to_string());
                    }
                }
            }

            // Try yum (Enterprise Linux / RHEL / CentOS / Fedora)
            {
                let cmd_str = "yum install -y glibc.i686 libstdc++.i686";
                info!(
                    cmd = cmd_str,
                    "Attempting to install SteamCMD dependencies via yum..."
                );
                if let Some(ref m) = self.model {
                    m.log_write(&format!("Attempting: {cmd_str}")).await;
                }
                match run_command("yum", &["install", "-y", "glibc.i686", "libstdc++.i686"]).await {
                    Ok(()) => {
                        if let Some(ref m) = self.model {
                            m.log_write("Dependencies installed successfully via yum")
                                .await;
                        }
                        Ok("yum".to_string())
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
        }
        .await;

        match result {
            Ok(pkg_manager) => {
                info!(
                    package_manager = %pkg_manager,
                    "SteamCMD dependencies installed successfully"
                );
            }
            Err(error_msg) => {
                warn!(
                    error = %error_msg,
                    manual_url = MANUAL_URL,
                    "Failed to install SteamCMD dependencies. \
                     This is not fatal — SteamCMD may still work. \
                     If the binary fails to start, install the dependencies manually."
                );
                if let Some(ref m) = self.model {
                    m.log_write(&format!(
                        "Dependency install failed: {error_msg}. \
                         See {MANUAL_URL} for manual instructions."
                    ))
                    .await;
                }
            }
        }
    }
}

/// Run a package manager command and return Ok(()) on success.
///
/// On non-zero exit, returns an error containing the stdout/stderr output.
async fn run_command(cmd: &str, args: &[&str]) -> Result<(), DepsError> {
    let mut child_cmd = tokio::process::Command::new(cmd);
    child_cmd.args(args);
    child_cmd.env("DEBIAN_FRONTEND", "noninteractive");

    let output = match child_cmd.output().await {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DepsError::NotFound {
                cmd: cmd.to_string(),
            })
        }
        Err(e) => return Err(DepsError::from(e)),
    };

    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let truncated_stdout = truncate(&stdout, 512);
        let truncated_stderr = truncate(&stderr, 512);
        Err(DepsError::InstallFailed {
            cmd: cmd.to_string(),
            stdout: truncated_stdout.into_owned(),
            stderr: truncated_stderr.into_owned(),
        })
    }
}

fn truncate(s: &str, max_chars: usize) -> std::borrow::Cow<'_, str> {
    if s.chars().count() <= max_chars {
        s.into()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...").into()
    }
}

/// Error type for dependency installation attempts.
#[derive(Debug)]
enum DepsError {
    /// The package manager command was not found on the system.
    NotFound { cmd: String },
    /// The package manager command failed.
    InstallFailed {
        cmd: String,
        stdout: String,
        stderr: String,
    },
}

impl std::fmt::Display for DepsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { cmd } => {
                write!(f, "Command \"{cmd}\" not found on this system")
            }
            Self::InstallFailed {
                cmd,
                stdout,
                stderr,
            } => {
                write!(f, "Command \"{cmd}\" failed.")?;
                if !stdout.is_empty() {
                    write!(f, "\nstdout: {stdout}")?;
                }
                if !stderr.is_empty() {
                    write!(f, "\nstderr: {stderr}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for DepsError {}

impl From<std::io::Error> for DepsError {
    fn from(err: std::io::Error) -> Self {
        Self::InstallFailed {
            cmd: "unknown".to_string(),
            stdout: err.to_string(),
            stderr: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_result;

    /// Verify extract() successfully unpacks a tar.gz archive and cleans up.
    #[test]
    fn extract_unpacks_tar_gz() {
        let temp_dir =
            std::env::temp_dir().join(format!("game-smith-steamcmd-test-{}", std::process::id()));
        log_result(
            std::fs::remove_dir_all(&temp_dir),
            &format!("cleaned up pre-existing temp dir {}", temp_dir.display()),
            &format!("failed to clean up pre-existing temp dir {}", temp_dir.display()),
        );
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

        log_result(
            std::fs::remove_dir_all(&temp_dir),
            &format!("cleaned up temp dir {}", temp_dir.display()),
            &format!("failed to clean up temp dir {}", temp_dir.display()),
        );
    }

    /// Verify try_install_dependencies() is callable and does not panic.
    /// Outcome depends on test environment (apt-get/yum availability, permissions).
    #[tokio::test]
    async fn try_install_dependencies_does_not_panic() {
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
