use std::collections::HashMap;
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

/// Result of distro detection for manual install instructions.
#[derive(Debug, Clone)]
pub struct DistroInfo {
    /// Human-readable label (e.g., "Arch Linux / `CachyOS`").
    pub label: String,
    /// The `sudo` command to install 32-bit `SteamCMD` dependencies.
    pub install_command: String,
}

/// Read `/etc/os-release` and return the package manager install command
/// that matches the running distribution.
///
/// Returns `None` if the file cannot be read or the distro is not recognized.
#[must_use]
pub fn detect_package_manager() -> Option<DistroInfo> {
    let content = std::fs::read_to_string("/etc/os-release").ok()?;

    let mut fields = HashMap::new();
    for line in content.lines() {
        let mut parts = line.splitn(2, '=');
        if let Some(key) = parts.next() {
            let value = parts.next().unwrap_or("").trim_matches('"');
            fields.insert(key, value);
        }
    }
    let id = fields.get("ID").copied().unwrap_or("");
    let id_like = fields.get("ID_LIKE").copied().unwrap_or("");

    let all_ids = format!("{id} {id_like}");

    if all_ids.contains("arch") || all_ids.contains("cachyos") {
        return Some(DistroInfo {
            label: "Arch Linux / CachyOS".to_string(),
            install_command: concat!(
                "sudo pacman -S lib32-glibc lib32-ncurses5-compat-libs ",
                "lib32-keyutils lib32-gcc-libs"
            )
            .to_string(),
        });
    }

    if all_ids.contains("debian") || all_ids.contains("ubuntu") || all_ids.contains("pop") {
        return Some(DistroInfo {
            label: "Debian / Ubuntu".to_string(),
            install_command: concat!(
                "sudo dpkg --add-architecture i386 && sudo apt-get update ",
                "&& sudo apt-get install -y lib32gcc-s1"
            )
            .to_string(),
        });
    }

    if all_ids.contains("fedora")
        || all_ids.contains("rhel")
        || all_ids.contains("centos")
        || all_ids.contains("amzn")
    {
        return Some(DistroInfo {
            label: "Fedora / RHEL".to_string(),
            install_command: "sudo dnf install -y glibc.i686 libstdc++.i686".to_string(),
        });
    }

    None
}

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
    /// Check for missing 32-bit shared library dependencies using `ldd`.
    ///
    /// Runs `ldd` against the 32-bit `steamclient.so` and collects any
    /// libraries reported as "not found". This is a proactive check that
    /// avoids the segfault that occurs when trying to run the binary
    /// without the required 32-bit runtime libraries.
    pub(super) fn check_dependencies(&self) -> Result<(), super::SteamCmdError> {
        let steamclient_path = self.steamcmd_dir.join("linux32").join("steamclient.so");

        if !steamclient_path.exists() {
            // File not extracted yet or non-standard layout — skip ldd check.
            // The binary test (+quit) will catch the problem.
            return Ok(());
        }

        let output = match std::process::Command::new("ldd")
            .arg(&steamclient_path)
            .output()
        {
            Ok(output) => output,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // `ldd` not available — unlikely, skip check.
                return Ok(());
            }
            Err(e) => {
                warn!(error = %e, "failed to run ldd for dependency check");
                return Ok(());
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Collect all "not found" library names
        let mut missing: Vec<String> = Vec::new();
        for line in stdout.lines() {
            if line.contains("not found") {
                // Format: "  libxxx.so.1 => not found"
                // or:     "  libxxx.so.1 => /some/path/libxxx.so.1 (0x...)"
                let lib_name = line
                    .split_whitespace()
                    .next()
                    .map_or_else(|| line.trim().to_string(), str::to_string);
                missing.push(lib_name);
            }
        }

        if missing.is_empty() {
            return Ok(());
        }

        let details = format!(
            "{} 32-bit shared library(ies) not found: {}\n{}",
            missing.len(),
            missing.join(", "),
            stderr.trim()
        );
        Err(super::SteamCmdError::MissingDependencies(details))
    }

    /// Detects the available package manager and runs the install command
    /// without sudo. This is best-effort: the function always returns `()`
    /// regardless of outcome. Progress is logged via `tracing` and written
    /// to the command run log file via `model.log_write()`.
    ///
    /// The definitive check for whether `SteamCMD` can run is
    /// [`super::SteamCmd::is_installed_with_deps`], which runs the binary
    /// and checks the exit code.
    pub(super) async fn try_install_dependencies(&self) {
        const MANUAL_URL: &str = "https://developer.valvesoftware.com/wiki/SteamCMD#Manually";

        struct PkgManager<'a> {
            name: &'a str,
            cmd: &'a str,
            args: &'a [&'a str],
        }

        const PACKAGE_MANAGERS: &[PkgManager<'_>] = &[
            PkgManager {
                name: "pacman",
                cmd: "pacman",
                args: &[
                    "-S",
                    "--noconfirm",
                    "lib32-glibc",
                    "lib32-ncurses5-compat-libs",
                    "lib32-keyutils",
                    "lib32-gcc-libs",
                ],
            },
            PkgManager {
                name: "apt-get",
                cmd: "apt-get",
                args: &["install", "-y", "lib32gcc-s1"],
            },
            PkgManager {
                name: "dnf",
                cmd: "dnf",
                args: &["install", "-y", "glibc.i686", "libstdc++.i686"],
            },
            PkgManager {
                name: "yum",
                cmd: "yum",
                args: &["install", "-y", "glibc.i686", "libstdc++.i686"],
            },
        ];

        let result: Result<String, String> = async {
            for pkg in PACKAGE_MANAGERS {
                let cmd_str = format!("{} {}", pkg.cmd, pkg.args.join(" "));
                info!(
                    cmd = cmd_str,
                    "Attempting to install SteamCMD dependencies via {}...", pkg.name
                );
                if let Some(ref m) = self.model {
                    m.log_write(&format!("Attempting: {cmd_str}")).await;
                }
                match run_command(pkg.cmd, pkg.args).await {
                    Ok(()) => {
                        if let Some(ref m) = self.model {
                            m.log_write(&format!(
                                "Dependencies installed successfully via {}",
                                pkg.name
                            ))
                            .await;
                        }
                        return Ok(pkg.name.to_string());
                    }
                    Err(DepsError::NotFound { .. }) => {
                        // Package manager not available — fall through.
                    }
                    Err(e) => {
                        // Package manager exists but couldn't install.
                        return Err(e.to_string());
                    }
                }
            }
            Err("No package manager found or all failed".to_string())
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

    /// After install, smoke-test the binary. If it segfaults (SIGSEGV),
    /// discover the 32-bit `libgcc_s.so.1` path via `ldconfig -p` and
    /// persist it for future invocations.
    ///
    /// The discovered path is written to:
    /// - `<steamcmd_dir>/.ld_preload` — read by `SteamCmd::new()` on every construction.
    /// - `~/.bashrc` or `~/.profile` — so terminal usage also benefits.
    ///
    /// All failures are non-fatal (logged via `warn!()` and returned).
    #[allow(clippy::too_many_lines)]
    pub(super) async fn configure_shell_env(&self) {
        // Step 1: Check glibc version. The smoke test (+quit) exits too early
        // to trigger the crash — it only happens during real network operations.
        // glibc >= 2.38 is the known affected range.
        let glibc_output = match std::process::Command::new("ldd").arg("--version").output() {
            Ok(output) => output,
            Err(e) => {
                warn!(error = %e, "failed to run ldd --version");
                return;
            }
        };

        let stdout = String::from_utf8_lossy(&glibc_output.stdout);
        let glibc_version = parse_glibc_version(&stdout);

        let Some(glibc_version) = glibc_version else {
            info!("Could not parse glibc version — skipping LD_PRELOAD workaround");
            return;
        };

        // Compare version as (major, minor). glibc >= 2.38 is affected.
        if glibc_version < (2, 38) {
            info!(version = ?glibc_version, "glibc version below 2.38 — no LD_PRELOAD workaround needed");
            return;
        }

        info!(version = ?glibc_version, "glibc >= 2.38 detected — configuring LD_PRELOAD workaround");
        if let Some(ref m) = self.model {
            m.log_write(&format!(
                "glibc {glibc_version:?} detected (>= 2.38). \
                     Configuring LD_PRELOAD workaround for SteamCMD...",
            ))
            .await;
        }

        // Step 2: Discover 32-bit libgcc_s.so.1 via ldconfig -p
        let ldconfig_output = match std::process::Command::new("ldconfig").arg("-p").output() {
            Ok(output) if output.status.success() => output,
            Ok(_) => {
                warn!("ldconfig -p exited with non-zero status");
                return;
            }
            Err(e) => {
                warn!(error = %e, "failed to run ldconfig -p");
                return;
            }
        };

        let stdout = String::from_utf8_lossy(&ldconfig_output.stdout);
        let lib_path = stdout
            .lines()
            .find(|line| line.contains("libgcc_s.so.1") && !line.contains("x86-64"))
            .and_then(|line| {
                line.split("=>")
                    .last()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            })
            .map(String::from);

        let Some(lib_path) = lib_path else {
            warn!(
                "No 32-bit libgcc_s.so.1 found in ldconfig -p output. \
                 Run `ldconfig -p | grep libgcc_s` to find the path manually \
                 and set LD_PRELOAD."
            );
            if let Some(ref m) = self.model {
                m.log_write(
                    "Could not find 32-bit libgcc_s.so.1 via ldconfig -p. \
                     SteamCMD may still segfault on launch.",
                )
                .await;
            }
            return;
        };

        info!(path = %lib_path, "Found 32-bit libgcc_s.so.1");
        if let Some(ref m) = self.model {
            m.log_write(&format!("Found 32-bit libgcc_s.so.1 at {lib_path}"))
                .await;
        }

        // Step 3: Persist to hint file
        let hint_path = self.steamcmd_dir.join(super::LD_PRELOAD_HINT);
        if let Err(e) = std::fs::write(&hint_path, &lib_path) {
            warn!(
                error = %e,
                path = %hint_path.display(),
                "failed to write LD_PRELOAD hint file"
            );
        } else {
            info!(path = %hint_path.display(), "Persisted LD_PRELOAD hint");
            if let Some(ref m) = self.model {
                m.log_write(&format!(
                    "Persisted LD_PRELOAD hint to {}",
                    hint_path.display()
                ))
                .await;
            }
        }

        // Step 4: Write to shell profile
        let export_line = format!("export LD_PRELOAD={lib_path}");
        let Some(home) = std::env::var_os("HOME") else {
            warn!("HOME not set — cannot write LD_PRELOAD to shell profile");
            return;
        };

        let home_path = std::path::Path::new(&home);
        let profile_path = if home_path.join(".bashrc").exists() {
            home_path.join(".bashrc")
        } else if home_path.join(".profile").exists() {
            home_path.join(".profile")
        } else {
            warn!(
                "Neither ~/.bashrc nor ~/.profile found — \
                 LD_PRELOAD will only apply to app-launched SteamCMD"
            );
            return;
        };

        // Skip if already present
        if let Ok(existing) = std::fs::read_to_string(&profile_path) {
            if existing.contains(&export_line) {
                info!(
                    path = %profile_path.display(),
                    "LD_PRELOAD export already in shell profile"
                );
                return;
            }
        }

        // Append
        let append_text = format!("\n# SteamCMD glibc workaround\n{export_line}\n");
        match std::fs::OpenOptions::new().append(true).open(&profile_path) {
            Ok(mut file) => {
                if std::io::Write::write_all(&mut file, append_text.as_bytes()).is_ok() {
                    info!(
                        path = %profile_path.display(),
                        "Added LD_PRELOAD to shell profile"
                    );
                    if let Some(ref m) = self.model {
                        m.log_write(&format!(
                            "Added LD_PRELOAD to {} — restart your shell for it to take effect",
                            profile_path.display()
                        ))
                        .await;
                    }
                } else {
                    warn!(
                        path = %profile_path.display(),
                        "failed to write LD_PRELOAD to shell profile"
                    );
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    path = %profile_path.display(),
                    "failed to open shell profile for writing"
                );
            }
        }
    }
}

/// Parse the glibc version from `ldd --version` output.
///
/// Expects a line like "ldd (GNU libc) 2.43" and returns `(major, minor)`.
fn parse_glibc_version(output: &str) -> Option<(u32, u32)> {
    // The version is typically on the first line, last token
    let first_line = output.lines().next()?;
    let token = first_line.split_whitespace().last()?;
    let mut parts = token.split('.');
    let major = parts.next().and_then(|s| s.parse::<u32>().ok())?;
    let minor = parts.next().and_then(|s| s.parse::<u32>().ok())?;
    Some((major, minor))
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
            &format!(
                "failed to clean up pre-existing temp dir {}",
                temp_dir.display()
            ),
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
            ld_preload_path: None,
        };

        steamcmd.try_install_dependencies().await;

        log_result(
            std::fs::remove_dir_all(&temp_dir),
            &format!("cleaned up temp dir {}", temp_dir.display()),
            &format!("failed to clean up temp dir {}", temp_dir.display()),
        );
    }
}
