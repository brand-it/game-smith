use std::collections::HashMap;
use std::fmt::{self, Display};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use loco_rs::app::SharedStore;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::models::command_runs::Model as CommandRunModel;
use crate::AppDirs;

// Platform-specific modules
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::{BINARY_NAME, DOWNLOAD_URL, TEMP_FILE_NAME};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::{BINARY_NAME, DOWNLOAD_URL, TEMP_FILE_NAME};

use linux_or_windows::extract;

mod linux_or_windows {
    #[cfg(target_os = "linux")]
    pub use super::linux::extract;
    #[cfg(target_os = "windows")]
    pub use super::windows::extract;
}

/// The HLDS app ID has special handling — `app_update` must be run multiple
/// times to download all engine files.
const HLDS_APP_ID: u32 = 90;
const HLDS_UPDATE_RETRIES: u32 = 5;

/// Name of the steamcmd data directory inside the application data directory.
const STEAMCMD_DIR_NAME: &str = "steamcmd";

/// Health status of the `SteamCMD` installation.
///
/// Stored in `AppContext.shared_store` at boot time and read by the
/// `steamcmd_health()` Tera function to display a banner across all pages.
#[derive(Debug, Clone)]
pub enum SteamCmdHealthStatus {
    /// Health check is still running (boot-time, non-blocking).
    Checking,
    /// `SteamCMD` binary exists and executes successfully.
    Healthy,
    /// `SteamCMD` binary does not exist on disk.
    NotInstalled,
    /// `SteamCMD` binary exists but fails to start (missing deps, permissions).
    Broken(String),
}

impl SteamCmdHealthStatus {
    /// Returns the status as a short string suitable for template conditionals.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Healthy => "healthy",
            Self::NotInstalled => "not_installed",
            Self::Broken(_) => "broken",
        }
    }
}

/// Static reference to the application's shared store.
///
/// Populated by [`set_shared_store`] during initialization so the Tera
/// function can read health status without receiving `AppContext` as an arg.
static SHARED_STORE: OnceLock<Arc<SharedStore>> = OnceLock::new();

/// Store a reference to the application's shared store.
///
/// Called once during boot by the `SteamCmdInstaller` initializer.
pub fn set_shared_store(store: &Arc<SharedStore>) {
    SHARED_STORE.set(store.clone()).ok();
}

/// Returns the current [`SteamCmdHealthStatus`] from the shared store.
///
/// Returns `None` if the store has not been initialized yet.
pub fn health_status() -> Option<SteamCmdHealthStatus> {
    SHARED_STORE
        .get()
        .and_then(|store| store.get::<SteamCmdHealthStatus>())
}

/// Tera function registered as `steamcmd_health()`.
///
/// Returns the current health status as a string: `"healthy"`,
/// `"not_installed"`, `"broken"`, or `"checking"`.
///
/// # Errors
/// Returns a [`::tera::Error`] if the Tera function fails.
#[allow(clippy::implicit_hasher)]
pub fn tera_steamcmd_health(
    _args: &HashMap<String, ::tera::Value>,
) -> ::tera::Result<::tera::Value> {
    let status = health_status().map_or("checking", |s| s.as_str());
    Ok(::tera::Value::String(status.to_owned()))
}

/// Errors that can occur during `SteamCMD` operations.
#[derive(Debug)]
pub enum SteamCmdError {
    /// Failed to download the `SteamCMD` archive.
    Download(Box<dyn std::error::Error + Send + Sync>),
    /// Failed to extract the `SteamCMD` archive.
    Extract(io::Error),
    /// Failed to create a directory during installation.
    CreateDir(io::Error),
    /// Failed to spawn the `SteamCMD` process.
    Spawn(io::Error),
    /// `SteamCMD` process exited with a non-zero status.
    ExitStatus(i32),
    /// Required 32-bit shared libraries are missing on Linux.
    MissingDependencies(String),
    /// A generic I/O error occurred.
    Io(io::Error),
}

impl Display for SteamCmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Download(src) => write!(f, "failed to download SteamCMD: {src}"),
            Self::Extract(err) => write!(f, "failed to extract SteamCMD archive: {err}"),
            Self::CreateDir(err) => write!(f, "failed to create directory: {err}"),
            Self::Spawn(err) => write!(f, "failed to spawn SteamCMD: {err}"),
            Self::ExitStatus(code) => write!(f, "SteamCMD exited with status {code}"),
            Self::MissingDependencies(details) => {
                write!(
                    f,
                    "SteamCMD failed to start: {details}\n\
                     Install missing dependencies manually: \
                     https://developer.valvesoftware.com/wiki/SteamCMD#Manually"
                )
            }
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for SteamCmdError {}

impl From<io::Error> for SteamCmdError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Manages `SteamCMD` installation and execution.
///
/// Handles platform-aware download, extraction, and invocation of Valve's
/// `SteamCMD` tool. `SteamCMD` self-updates on every invocation, so no version
/// tracking is required.
pub struct SteamCmd {
    /// Base directory for steamcmd files.
    steamcmd_dir: PathBuf,
    /// Path to the steamcmd binary.
    binary_path: PathBuf,
    /// Optional command run model for logging progress to the log file.
    model: Option<CommandRunModel>,
}

impl SteamCmd {
    /// Create a new [`SteamCmd`] instance with paths resolved from the
    /// application's data directories.
    #[must_use]
    pub fn new(dirs: &AppDirs) -> Self {
        let steamcmd_dir = dirs.app_dir.join(STEAMCMD_DIR_NAME);
        let binary_path = steamcmd_dir.join(BINARY_NAME);
        Self {
            steamcmd_dir,
            binary_path,
            model: None,
        }
    }

    /// Attach a command run model so progress is written to the log file.
    #[must_use]
    pub fn with_command_run(mut self, model: CommandRunModel) -> Self {
        self.model = Some(model);
        self
    }

    /// Take ownership of the stored model, leaving `None` in its place.
    ///
    /// Used to reclaim the model for DB status updates after installation.
    pub const fn take_model(&mut self) -> Option<CommandRunModel> {
        self.model.take()
    }

    /// Returns a reference to the attached command run model, if any.
    #[must_use]
    pub const fn model(&self) -> Option<&CommandRunModel> {
        self.model.as_ref()
    }
    /// Returns the path to the steamcmd directory.
    #[must_use]
    pub fn steamcmd_dir(&self) -> &Path {
        &self.steamcmd_dir
    }

    /// Returns the path to the steamcmd binary.
    #[must_use]
    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    /// Checks if the steamcmd binary exists on disk.
    #[must_use]
    pub fn is_installed(&self) -> bool {
        self.binary_path.exists()
    }

    /// Checks if steamcmd is installed and verifies the binary can actually
    /// run by executing a live test.
    ///
    /// This spawns `steamcmd.sh +quit` and checks the exit code. If the binary
    /// exists but fails to start (missing shared libraries, permission issues,
    /// etc.), the captured stderr is returned in the error for diagnostics.
    ///
    /// # Errors
    /// Returns [`SteamCmdError::Io`] if the steamcmd binary is not found.
    /// Returns [`SteamCmdError::Spawn`] if the process cannot be started.
    /// Returns [`SteamCmdError::MissingDependencies`] with the captured stderr
    /// if the binary exits with a non-zero status.
    pub fn is_installed_with_deps(&self) -> Result<(), SteamCmdError> {
        if !self.is_installed() {
            return Err(SteamCmdError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "steamcmd binary not found",
            )));
        }

        let output = std::process::Command::new(&self.binary_path)
            .arg("+quit")
            .output()
            .map_err(SteamCmdError::Spawn)?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(SteamCmdError::MissingDependencies(stderr))
        }
    }

    /// Ensures `SteamCMD` is installed on the system.
    ///
    /// This method is idempotent:
    /// - If the binary already exists, it returns immediately.
    /// - If the binary is missing, it downloads and extracts the platform
    ///   archive.
    ///
    /// `SteamCMD` self-updates on every invocation, so a pre-existing binary
    /// does not need to be re-downloaded.
    ///
    /// # Errors
    /// Returns a [`SteamCmdError`] if the directory cannot be created, the
    /// download fails, or extraction fails.
    pub async fn ensure_installed(&self) -> Result<(), SteamCmdError> {
        if self.is_installed() {
            debug!(path = %self.binary_path.display(), "SteamCMD binary already installed");
            return Ok(());
        }

        info!(path = %self.binary_path.display(), "SteamCMD binary not found, installing...");
        self.install().await
    }

    /// Downloads and extracts the `SteamCMD` archive for the current platform.
    ///
    /// # Errors
    /// Returns a [`SteamCmdError`] if the download or extraction fails.
    pub async fn install(&self) -> Result<(), SteamCmdError> {
        std::fs::create_dir_all(&self.steamcmd_dir).map_err(SteamCmdError::CreateDir)?;

        let temp_path = self.download_to_temp(DOWNLOAD_URL).await?;
        extract(&self.steamcmd_dir, &temp_path)?;

        // Attempt to install platform-specific dependencies (best-effort, no-op on Windows).
        // Failure is logged but does not block installation — the binary may still work.
        self.try_install_dependencies().await;

        if self.is_installed() {
            info!(path = %self.binary_path.display(), "SteamCMD installed successfully");
            Ok(())
        } else {
            error!("SteamCMD installation failed: binary not found after extraction");
            Err(SteamCmdError::Extract(io::Error::new(
                io::ErrorKind::NotFound,
                "binary not found after extraction",
            )))
        }
    }

    /// Downloads a file from a URL and writes it to a temporary path.
    async fn download_to_temp(&self, url: &str) -> Result<PathBuf, SteamCmdError> {
        let temp_dir = self.steamcmd_dir.join("temp");
        std::fs::create_dir_all(&temp_dir).map_err(SteamCmdError::CreateDir)?;

        let temp_path = temp_dir.join(TEMP_FILE_NAME);

        info!(url = url, "Downloading SteamCMD archive...");
        let response = reqwest::get(url)
            .await
            .map_err(|e| SteamCmdError::Download(Box::new(e)))?;

        if !response.status().is_success() {
            return Err(SteamCmdError::Download(Box::new(io::Error::other(
                format!("HTTP {} from {url}", response.status()),
            ))));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| SteamCmdError::Download(Box::new(e)))?;
        tokio::fs::write(&temp_path, bytes.as_ref())
            .await
            .map_err(SteamCmdError::Io)?;

        debug!(path = %temp_path.display(), "Download complete");
        Ok(temp_path)
    }

    /// Runs steamcmd with the given arguments and waits for it to complete.
    ///
    /// The process inherits stdin/stdout/stderr. Use this for interactive or
    /// verbose operations.
    ///
    /// # Arguments
    /// * `args` - Command-line arguments to pass to steamcmd (does not include
    ///   the `+quit` command; callers should include it if needed).
    ///
    /// # Errors
    /// Returns [`SteamCmdError::Spawn`] if the process cannot be started, or
    /// [`SteamCmdError::ExitStatus`] if steamcmd exits with a non-zero code.
    pub async fn run_command(&self, args: &[&str]) -> Result<(), SteamCmdError> {
        if !self.is_installed() {
            return Err(SteamCmdError::Spawn(io::Error::new(
                io::ErrorKind::NotFound,
                "steamcmd is not installed",
            )));
        }

        info!(
            binary = %self.binary_path.display(),
            args = ?args,
            "Running SteamCMD"
        );

        let output = Command::new(&self.binary_path)
            .args(args)
            .output()
            .await
            .map_err(SteamCmdError::Spawn)?;

        if output.status.success() {
            debug!("SteamCMD command completed successfully");
            Ok(())
        } else {
            let code = output.status.code().unwrap_or(-1);
            warn!(
                exit_code = code,
                stderr = %String::from_utf8_lossy(&output.stderr),
                "SteamCMD command failed"
            );
            Err(SteamCmdError::ExitStatus(code))
        }
    }

    /// Runs steamcmd with the given arguments and captures stdout.
    ///
    /// # Arguments
    /// * `args` - Command-line arguments to pass to steamcmd.
    ///
    /// # Returns
    /// The stdout output as a string.
    ///
    /// # Errors
    /// Returns [`SteamCmdError::Spawn`] if the process cannot be started, or
    /// [`SteamCmdError::ExitStatus`] if steamcmd exits with a non-zero code.
    pub async fn run_command_capture(&self, args: &[&str]) -> Result<String, SteamCmdError> {
        if !self.is_installed() {
            return Err(SteamCmdError::Spawn(io::Error::new(
                io::ErrorKind::NotFound,
                "steamcmd is not installed",
            )));
        }

        let output = Command::new(&self.binary_path)
            .args(args)
            .output()
            .await
            .map_err(SteamCmdError::Spawn)?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let code = output.status.code().unwrap_or(-1);
            Err(SteamCmdError::ExitStatus(code))
        }
    }

    /// Installs or updates a game server application using `SteamCMD`.
    ///
    /// This is a high-level convenience method that runs the `app_update`
    /// command with the `validate` flag.
    ///
    /// # Arguments
    /// * `app_id` - The Steam App ID of the game server.
    /// * `install_dir` - The directory where the game server will be installed.
    ///
    /// # Edge cases
    /// For HLDS (`app_id` 90), the `app_update` command is run multiple times
    /// as required by the Half-Life Dedicated Server installation process.
    ///
    /// # Errors
    /// Returns a [`SteamCmdError`] if the command fails.
    pub async fn update_app(&self, app_id: u32, install_dir: &Path) -> Result<(), SteamCmdError> {
        let install_dir_str = install_dir.to_string_lossy().to_string();
        let retries = if app_id == HLDS_APP_ID {
            HLDS_UPDATE_RETRIES
        } else {
            1
        };

        info!(
            app_id = app_id,
            install_dir = %install_dir.display(),
            retries = retries,
            "Updating game server application"
        );

        let app_id_str = app_id.to_string();
        for attempt in 1..=retries {
            if app_id == HLDS_APP_ID && retries > 1 {
                info!(
                    app_id = app_id,
                    attempt = attempt,
                    total = retries,
                    "Running SteamCMD app_update"
                );
            }

            let args: Vec<&str> = vec![
                "+force_install_dir",
                &install_dir_str,
                "+app_update",
                &app_id_str,
                "+validate",
                "+quit",
            ];

            self.run_command(&args).await?;
        }

        Ok(())
    }

    /// Builds the command-line arguments for an `app_update` call without
    /// executing it. Useful for testing and preview.
    #[must_use]
    pub fn build_update_app_args(app_id: u32, install_dir: &Path) -> Vec<String> {
        vec![
            "+force_install_dir".to_string(),
            install_dir.to_string_lossy().to_string(),
            "+app_update".to_string(),
            app_id.to_string(),
            "+validate".to_string(),
            "+quit".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steamcmd_paths() {
        let data_home = std::env::temp_dir();
        let dirs = AppDirs::new(data_home.to_string_lossy().to_string());
        let steamcmd = SteamCmd::new(&dirs);
        assert!(
            !steamcmd.is_installed(),
            "is_installed should return false for non-existent binary"
        );
    }

    #[test]
    fn test_build_update_app_args() {
        let install_dir = PathBuf::from("/opt/games/counter-strike");
        let args = SteamCmd::build_update_app_args(740, &install_dir);

        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "+force_install_dir");
        assert_eq!(args[1], "/opt/games/counter-strike");
        assert_eq!(args[2], "+app_update");
        assert_eq!(args[3], "740");
        assert_eq!(args[4], "+validate");
        assert_eq!(args[5], "+quit");
    }

    #[test]
    fn test_build_update_app_args_hlds() {
        let install_dir = PathBuf::from("/opt/games/hlds");
        let args = SteamCmd::build_update_app_args(HLDS_APP_ID, &install_dir);

        assert_eq!(args[3], "90");
    }

    #[test]
    fn test_missing_deps_error_display() {
        let err = SteamCmdError::MissingDependencies(
            "error while loading shared libraries: libstdc++.so.6".to_string(),
        );
        let msg = err.to_string();
        assert!(msg.contains("failed to start"));
        assert!(msg.contains("libstdc++"));
    }
}
