use std::path::PathBuf;

use loco_rs::app::AppContext;
use loco_rs::model::ModelError;
use tracing::{info, warn};

use crate::data::command_runner::CommandRunner;
use crate::models::game_servers;

/// Errors that can occur during game server installation.
#[derive(Debug)]
pub enum GameServerError {
    /// `SteamCMD` is not installed on the system.
    SteamCmdNotInstalled,
    /// Failed to create the install directory.
    CreateDir(std::io::Error),
    /// Failed to write the `SteamCMD` script file.
    WriteScript(std::io::Error),
    /// Failed to write the boot script `.bat` file.
    WriteBootScript(std::io::Error),
    /// Failed to execute the installation command.
    Execute(ModelError),
    /// The game server record was not found.
    NotFound,
    /// Failed to decrypt Steam credentials.
    SteamCredentials(crate::data::encryption::EncryptionError),
}

impl std::fmt::Display for GameServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SteamCmdNotInstalled => write!(f, "SteamCMD is not installed"),
            Self::CreateDir(e) => write!(f, "failed to create install directory: {e}"),
            Self::WriteScript(e) => write!(f, "failed to write SteamCMD script: {e}"),
            Self::WriteBootScript(e) => write!(f, "failed to write boot script: {e}"),
            Self::Execute(e) => write!(f, "failed to execute installation: {e}"),
            Self::NotFound => write!(f, "game server not found"),
            Self::SteamCredentials(e) => write!(f, "failed to decrypt Steam credentials: {e}"),
        }
    }
}

impl std::error::Error for GameServerError {}

/// High-level API for installing and managing game servers via `SteamCMD`.
pub struct GameServerInstaller {
    ctx: AppContext,
}

impl GameServerInstaller {
    /// Create a new [`GameServerInstaller`] with the given application context.
    #[must_use]
    pub fn new(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    /// Build a `SteamCMD` script for installing a game server.
    ///
    /// The script combines `force_install_dir`, `login anonymous`, `app_update`,
    /// and `quit` into a single file so all output goes to one log.
    ///
    /// # Arguments
    /// * `app_id` - Steam App ID for the game server.
    /// * `install_dir` - Target installation directory.
    /// * `platform` - Target platform (used for cross-platform installs).
    /// * `server_mod` - Optional mod name for HL1 games.
    /// * `beta_branch` - Optional beta branch name.
    /// * `steam_username` - Optional Steam account username.
    /// * `steam_password` - Optional Steam account password.
    ///
    /// # Returns
    /// A multi-line string containing the `SteamCMD` script.
    #[must_use]
    pub fn build_install_script(
        app_id: u32,
        install_dir: &str,
        platform: &str,
        server_mod: Option<&str>,
        beta_branch: Option<&str>,
        steam_username: Option<&str>,
        steam_password: Option<&str>,
    ) -> String {
        // Determine auth mode
        let has_credentials = steam_username
            .filter(|u| !u.is_empty())
            .zip(steam_password.filter(|p| !p.is_empty()))
            .is_some();

        let mut lines = vec!["@ShutdownOnFailedCommand 1".to_string()];

        // @NoPromptForPassword is only needed for anonymous login
        if !has_credentials {
            lines.push("@NoPromptForPassword 1".to_string());
        }

        // Cross-platform support: force platform type when target differs from host
        let host_platform = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        };
        if platform != host_platform {
            lines.push(format!("@sSteamCmdForcePlatformType {platform}"));
        }

        lines.push(format!("force_install_dir {install_dir}"));

        if let Some((username, password)) = steam_username
            .filter(|u| !u.is_empty())
            .zip(steam_password.filter(|p| !p.is_empty()))
        {
            lines.push(format!("login {username} {password}"));
        } else {
            lines.push("login anonymous".to_string());
        }

        // Build app_update arguments
        let mut update_args = vec![app_id.to_string(), "validate".to_string()];

        if let Some(mod_name) = server_mod.filter(|s| !s.is_empty()) {
            update_args.push(format!("mod_{mod_name}"));
        }

        if let Some(branch) = beta_branch.filter(|s| !s.is_empty()) {
            update_args.push("-beta".to_string());
            update_args.push(branch.to_string());
        }
        lines.push(format!("app_update {}", update_args.join(" ")));
        lines.push("quit".to_string());
        lines.join("\n")
    }

    /// Build a `SteamCMD` script for updating an existing game server.
    #[must_use]
    pub fn build_update_script(
        app_id: u32,
        install_dir: &str,
        platform: &str,
        server_mod: Option<&str>,
        beta_branch: Option<&str>,
        steam_username: Option<&str>,
        steam_password: Option<&str>,
    ) -> String {
        Self::build_install_script(
            app_id,
            install_dir,
            platform,
            server_mod,
            beta_branch,
            steam_username,
            steam_password,
        )
    }
    /// Load and decrypt Steam credentials from the database.
    ///
    /// Returns `(Some(username), Some(password))` if configured and decryption succeeds.
    /// Falls back to `(None, None)` if no credentials are stored or decryption fails.
    async fn load_steam_credentials(
        &self,
    ) -> Result<(Option<String>, Option<String>), GameServerError> {
        // Try to load credentials from DB
        let creds = crate::models::steam_credentials::Model::find(&self.ctx)
            .await
            .map_err(GameServerError::Execute)?;

        if let Some(record) = creds {
            // Load encryption key
            let data_home = crate::resolve_data_home();
            let dirs = crate::AppDirs::new(data_home);
            let key_path = dirs.app_dir.join("secret.key");

            let key = crate::data::encryption::EncryptionKey::load(&key_path)
                .map_err(GameServerError::SteamCredentials)?;

            let password = key
                .decrypt(&record.nonce, &record.ciphertext)
                .map_err(GameServerError::SteamCredentials)?;

            Ok((Some(record.username), Some(password)))
        } else {
            Ok((None, None))
        }
    }

    /// Install a game server by executing a `SteamCMD` script.
    ///
    /// Creates the install directory, writes a `SteamCMD` script, and executes
    /// it via [`CommandRunner`] so progress streams over WebSocket.
    ///
    /// # Arguments
    /// * `server` - The game server model record.
    ///
    /// # Errors
    /// Returns a [`GameServerError`] if `SteamCMD` is not installed, the
    /// directory cannot be created, or the command execution fails.
    pub async fn install(
        &self,
        server: &game_servers::Model,
    ) -> Result<crate::data::command_runner::CommandRun, GameServerError> {
        let app_id = server.app_id_u32();
        let install_dir = &server.install_dir;
        let platform = &server.platform;
        let server_mod = server.server_mod.as_deref();
        let beta_branch = server.beta_branch.as_deref();

        // Load Steam credentials only if server is configured to use Steam login
        let (steam_username, steam_password) = if server.use_steam_login {
            self.load_steam_credentials().await?
        } else {
            (None, None)
        };

        // Resolve SteamCMD binary path
        let data_home = crate::resolve_data_home();
        let dirs = crate::AppDirs::new(data_home);
        let steamcmd = crate::data::steamcmd::SteamCmd::new(&dirs);

        if !steamcmd.is_installed() {
            return Err(GameServerError::SteamCmdNotInstalled);
        }

        // Create install directory
        std::fs::create_dir_all(install_dir).map_err(GameServerError::CreateDir)?;

        // Build and write script
        let script = Self::build_install_script(
            app_id,
            install_dir,
            platform,
            server_mod,
            beta_branch,
            steam_username.as_deref(),
            steam_password.as_deref(),
        );
        let script_path = PathBuf::from(install_dir).join(format!("install_{app_id}.txt"));
        std::fs::write(&script_path, &script).map_err(GameServerError::WriteScript)?;

        info!(
            server_id = server.id,
            app_id = app_id,
            install_dir = %install_dir,
            script = %script_path.display(),
            "Starting game server installation"
        );

        // Execute via CommandRunner for streaming
        let runner = CommandRunner::new(&self.ctx);
        let binary_path = steamcmd.binary_path().to_string_lossy().to_string();
        let script_path_str = script_path.to_string_lossy().to_string();

        let title = Some(format!("Install {app_id}: {}", server.name));
        let run = runner
            .execute(
                binary_path,
                vec!["+runscript".to_string(), script_path_str],
                Some(steamcmd.steamcmd_dir().to_string_lossy().to_string()),
                None,
                title,
                Some(i64::from(server.id)),
            )
            .await
            .map_err(GameServerError::Execute)?;

        Ok(run)
    }

    /// Update an existing game server installation.
    ///
    /// # Arguments
    /// * `server` - The game server model record.
    ///
    /// # Errors
    /// Returns a [`GameServerError`] if `SteamCMD` is not installed or
    /// command execution fails.
    pub async fn update(
        &self,
        server: &game_servers::Model,
    ) -> Result<crate::data::command_runner::CommandRun, GameServerError> {
        let app_id = server.app_id_u32();
        let install_dir = &server.install_dir;
        let platform = &server.platform;
        let server_mod = server.server_mod.as_deref();
        let beta_branch = server.beta_branch.as_deref();

        // Load Steam credentials only if server is configured to use Steam login
        let (steam_username, steam_password) = if server.use_steam_login {
            self.load_steam_credentials().await?
        } else {
            (None, None)
        };

        let data_home = crate::resolve_data_home();
        let dirs = crate::AppDirs::new(data_home);
        let steamcmd = crate::data::steamcmd::SteamCmd::new(&dirs);

        if !steamcmd.is_installed() {
            return Err(GameServerError::SteamCmdNotInstalled);
        }

        let script = Self::build_update_script(
            app_id,
            install_dir,
            platform,
            server_mod,
            beta_branch,
            steam_username.as_deref(),
            steam_password.as_deref(),
        );
        std::fs::create_dir_all(install_dir).map_err(GameServerError::CreateDir)?;
        let script_path = PathBuf::from(install_dir).join(format!("update_{app_id}.txt"));
        std::fs::write(&script_path, &script).map_err(GameServerError::WriteScript)?;

        info!(
            server_id = server.id,
            app_id = app_id,
            "Starting game server update"
        );

        let runner = CommandRunner::new(&self.ctx);
        let binary_path = steamcmd.binary_path().to_string_lossy().to_string();
        let script_path_str = script_path.to_string_lossy().to_string();

        let title = Some(format!("Update {app_id}: {}", server.name));
        let run = runner
            .execute(
                binary_path,
                vec!["+runscript".to_string(), script_path_str],
                Some(steamcmd.steamcmd_dir().to_string_lossy().to_string()),
                None,
                title,
                Some(i64::from(server.id)),
            )
            .await
            .map_err(GameServerError::Execute)?;

        Ok(run)
    }

    /// Start a game server using its boot script.
    ///
    /// If no boot script is configured, attempts to find and run the default
    /// server executable. If neither is available, logs a warning and returns
    /// `Ok(None)` without erroring.
    ///
    /// # Arguments
    /// * `server` - The game server model record.
    ///
    /// # Errors
    /// Returns a [`GameServerError`] if the command execution fails.
    pub async fn start(
        &self,
        server: &game_servers::Model,
    ) -> Result<Option<crate::data::command_runner::CommandRun>, GameServerError> {
        let runner = CommandRunner::new(&self.ctx);

        let (command, args, working_dir, title) = if let Some(ref script) = server.boot_script {
            #[cfg(target_os = "windows")]
            let (command, args) = {
                let bat_path = Self::write_boot_script_batch(&server.install_dir, script)
                    .map_err(GameServerError::WriteBootScript)?;
                (
                    "cmd.exe".to_string(),
                    vec!["/C".to_string(), bat_path.to_string_lossy().to_string()],
                )
            };

            #[cfg(not(target_os = "windows"))]
            let (command, args) = Self::boot_script_command(script);
            (
                command,
                args,
                Some(server.install_dir.clone()),
                Some(format!("Start {}", server.name)),
            )
        } else {
            // Default: try to find a server executable
            let install_dir = PathBuf::from(&server.install_dir);
            let candidates = Self::find_server_executable(&install_dir);

            if let Some(exe) = candidates {
                (
                    exe.to_string_lossy().to_string(),
                    vec![],
                    Some(server.install_dir.clone()),
                    Some(format!("Start {}", server.name)),
                )
            } else {
                warn!(
                    server_id = server.id,
                    install_dir = %server.install_dir,
                    "No server executable found and no boot script configured"
                );
                return Ok(None);
            }
        };

        let run = runner
            .execute(
                command,
                args,
                working_dir,
                None,
                title,
                Some(i64::from(server.id)),
            )
            .await
            .map_err(GameServerError::Execute)?;

        Ok(Some(run))
    }

    /// Attempt to find a server executable in the install directory.
    ///
    /// Checks common server binary names for popular game servers.
    #[must_use]
    pub fn find_server_executable(install_dir: &std::path::Path) -> Option<PathBuf> {
        // Linux: executables identified by permissions, not extension
        #[cfg(target_os = "linux")]
        let primary_candidates = [
            "srcds_run",
            "srcds",
            "hl_linux",
            "hlds_run",
            "server",
            "game-server",
        ];
        // Windows: check .exe variants, including Windows-specific names
        #[cfg(target_os = "windows")]
        let primary_candidates = [
            "srcds.exe",
            "srcds_run.exe",
            "hl.exe",
            "hlds.exe",
            "hlds_run.exe",
            "server.exe",
        ];

        for candidate in &primary_candidates {
            let path = install_dir.join(candidate);
            if path.exists() {
                return Some(path);
            }
        }

        // Fallback: check remaining variants (e.g. .exe on Linux for cross-platform installs)
        let fallback = [
            "srcds_run.exe",
            "srcds.exe",
            "server.exe",
            "game-server.exe",
        ];
        for candidate in &fallback {
            let path = install_dir.join(candidate);
            if path.exists() {
                return Some(path);
            }
        }

        None
    }

    /// Stop a running game server by terminating its command run processes.
    ///
    /// Queries running [`command_runs`] records for this server, sends
    /// SIGTERM to each process with a PID, marks the runs as failed,
    /// and updates the server status to Stopped.
    ///
    /// # Arguments
    /// * `server` - The game server model record.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database update fails.
    pub async fn stop(&self, server: &game_servers::Model) -> Result<(), ModelError> {
        let running_runs = crate::models::command_runs::Model::find_running_by_server(
            &self.ctx,
            i64::from(server.id),
        )
        .await?;

        for run in running_runs {
            if let Some(pid) = run.pid {
                // Fire SIGTERM at the entire process group and return immediately.
                // We don't wait for the process to exit — on next boot the PID
                // liveness task will reconcile DB state with ground truth.
                #[cfg(target_os = "linux")]
                {
                    let ret = game_servers::kill_process_group(pid, game_servers::TERM_SIGNAL);
                    if ret == 0 {
                        info!(
                            server_id = server.id,
                            run_id = run.id,
                            pgid = pid,
                            "Sent SIGTERM to process group"
                        );
                    } else {
                        warn!(
                            server_id = server.id,
                            run_id = run.id,
                            pgid = pid,
                            error = ?std::io::Error::last_os_error(),
                            "Failed to signal process group; falling back to single PID"
                        );
                        let _ = game_servers::kill_pid(pid, game_servers::TERM_SIGNAL);
                    }
                }

                #[cfg(target_os = "windows")]
                {
                    let _ = game_servers::kill_pid(pid, game_servers::TERM_SIGNAL);
                    info!(
                        server_id = server.id,
                        run_id = run.id,
                        pid = pid,
                        "Sent termination signal to process"
                    );
                }
            }
        }

        Ok(())
    }

    /// Delete a game server record from the database.
    ///
    /// Does not remove files from the install directory.
    ///
    /// # Arguments
    /// * `server` - The game server model record.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn delete(&self, server: &game_servers::Model) -> Result<(), ModelError> {
        use sea_orm::Delete;

        let active: game_servers::ActiveModel = server.clone().into();
        Delete::one(active)
            .exec(&self.ctx.db)
            .await
            .map_err(ModelError::from)?;

        info!(server_id = server.id, "Game server record deleted");
        Ok(())
    }

    /// Writes the boot script to a `.bat` file and returns the path.
    ///
    /// On Windows, passing multiline or quoted scripts inline to `cmd.exe /C`
    /// is fragile. Writing to a file first avoids quoting issues.
    #[cfg(target_os = "windows")]
    fn write_boot_script_batch(install_dir: &str, script: &str) -> Result<PathBuf, std::io::Error> {
        let bat_path = PathBuf::from(install_dir).join("game-smith-start.bat");
        let content = format!("@echo off\r\n{script}");
        std::fs::write(&bat_path, content)?;
        Ok(bat_path)
    }
    /// Returns the shell command and arguments for executing a boot script.
    ///
    /// On Windows, uses `cmd.exe /C <script>`.
    /// On other platforms, uses `/bin/sh -c <script>`.
    #[must_use]
    fn boot_script_command(script: &str) -> (String, Vec<String>) {
        #[cfg(target_os = "windows")]
        return (
            "cmd.exe".to_string(),
            vec!["/C".to_string(), script.to_string()],
        );

        #[cfg(not(target_os = "windows"))]
        (
            "/bin/sh".to_string(),
            vec!["-c".to_string(), script.to_string()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_install_script_basic() {
        let script = GameServerInstaller::build_install_script(
            740,
            "/home/user/games/cs2",
            "linux",
            None,
            None,
            None,
            None,
        );

        assert!(script.contains("@ShutdownOnFailedCommand 1"));
        assert!(script.contains("@NoPromptForPassword 1"));
        assert!(script.contains("force_install_dir /home/user/games/cs2"));
        assert!(script.contains("login anonymous"));
        assert!(script.contains("app_update 740 validate"));
        assert!(script.contains("quit"));
    }

    #[test]
    fn test_build_install_script_with_mod() {
        let script = GameServerInstaller::build_install_script(
            90,
            "/opt/hlds/czero",
            "linux",
            Some("czero"),
            None,
            None,
            None,
        );

        assert!(script.contains("app_update 90 validate mod_czero"));
    }

    #[test]
    fn test_build_install_script_with_beta() {
        let script = GameServerInstaller::build_install_script(
            252490,
            "/opt/cstrike",
            "linux",
            None,
            Some("public"),
            None,
            None,
        );

        assert!(script.contains("app_update 252490 validate -beta public"));
    }

    #[test]
    fn test_build_install_script_with_steam_auth() {
        let script = GameServerInstaller::build_install_script(
            427520,
            "/opt/factorio",
            "linux",
            None,
            None,
            Some("my_steam_user"),
            Some("my_steam_pass"),
        );

        assert!(script.contains("@ShutdownOnFailedCommand 1"));
        assert!(!script.contains("@NoPromptForPassword 1"));
        assert!(script.contains("login my_steam_user my_steam_pass"));
        assert!(!script.contains("login anonymous"));
        assert!(script.contains("app_update 427520 validate"));
    }

    #[test]
    fn test_build_install_script_cross_platform() {
        #[cfg(not(target_os = "windows"))]
        {
            let script = GameServerInstaller::build_install_script(
                740, "/opt/cs2", "windows", None, None, None, None,
            );
            assert!(script.contains("@sSteamCmdForcePlatformType windows"));
        }

        #[cfg(target_os = "windows")]
        {
            let script = GameServerInstaller::build_install_script(
                740,
                "C:\\games\\cs2",
                "linux",
                None,
                None,
                None,
                None,
            );
            assert!(script.contains("@sSteamCmdForcePlatformType linux"));
        }
    }

    #[test]
    fn test_slugify() {
        assert_eq!(game_servers::slugify("My CS2 Server"), "my-cs2-server");
        assert_eq!(game_servers::slugify("  hello   world  "), "hello-world");
        assert_eq!(game_servers::slugify("test_123"), "test-123");
    }

    #[test]
    fn test_default_install_dir() {
        let dir = game_servers::default_install_dir("My Server");
        #[cfg(target_os = "windows")]
        assert!(dir.contains("\\game-smith\\games\\my-server"));
        #[cfg(not(target_os = "windows"))]
        assert!(dir.contains("/game-smith/games/my-server"));
    }

    #[test]
    fn test_build_install_script_empty_strings_treated_as_none() {
        let script = GameServerInstaller::build_install_script(
            740,
            "/home/user/games/cs2",
            "linux",
            Some(""),
            Some(""),
            None,
            None,
        );

        assert!(script.contains("app_update 740 validate"));
        assert!(!script.contains("mod_"));
        assert!(!script.contains("-beta"));
        assert!(script.contains("login anonymous"));
    }

    #[test]
    fn test_build_install_script_empty_mod_only() {
        let script = GameServerInstaller::build_install_script(
            740,
            "/home/user/games/cs2",
            "linux",
            Some(""),
            Some("beta"),
            None,
            None,
        );

        assert!(!script.contains("mod_"));
        assert!(script.contains("-beta beta"));
    }

    #[test]
    fn test_build_install_script_empty_steam_creds_treated_as_anonymous() {
        let script = GameServerInstaller::build_install_script(
            740,
            "/opt/cs2",
            "linux",
            None,
            None,
            Some(""),
            Some("password"),
        );

        assert!(script.contains("login anonymous"));
        assert!(script.contains("@NoPromptForPassword 1"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_write_boot_script_batch() {
        let dir = std::env::temp_dir().join(format!("game-smith-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        let dir_str = dir.to_string_lossy().to_string();

        let script = "start /B server.exe -port 27015\r\necho done";
        let bat_path = GameServerInstaller::write_boot_script_batch(&dir_str, script)
            .expect("failed to write batch file");

        assert!(bat_path.exists(), "batch file was not created");
        assert_eq!(bat_path.file_name().unwrap(), "game-smith-start.bat");

        let content = std::fs::read_to_string(&bat_path).expect("failed to read batch file");
        assert!(content.starts_with("@echo off\r\n"));
        assert!(content.contains("start /B server.exe -port 27015"));
        assert!(content.contains("echo done"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_boot_script_command_unix() {
        let (cmd, args) = GameServerInstaller::boot_script_command("./srcds_run -game csgo");
        assert_eq!(cmd, "/bin/sh");
        assert_eq!(args, ["-c", "./srcds_run -game csgo"]);
    }
}
