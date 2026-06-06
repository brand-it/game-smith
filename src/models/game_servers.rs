use super::_entities::game_servers::Column;
pub use super::_entities::game_servers::{ActiveModel, Entity, Model};
use crate::data::game_server_installer::GameServerError;
use loco_rs::app::AppContext;
use loco_rs::model::ModelError;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue, QueryOrder};
use serde::{Deserialize, Serialize};

/// Possible values for the `game_servers.status` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Pending,
    Installing,
    Installed,
    Running,
    Stopped,
    Error,
}

impl ServerStatus {
    /// Returns the canonical lowercase database representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

impl std::str::FromStr for ServerStatus {
    type Err = std::convert::Infallible;

    /// Parse a database string into a [`ServerStatus`].
    /// Unknown values default to [`ServerStatus::Pending`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "installing" => Self::Installing,
            "installed" => Self::Installed,
            "running" => Self::Running,
            "stopped" => Self::Stopped,
            "error" => Self::Error,
            _ => Self::Pending,
        })
    }
}

impl AsRef<str> for ServerStatus {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Generate a URL-safe slug from a display name.
///
/// Lowercases the input, replaces non-alphanumeric characters with hyphens,
/// and collapses consecutive hyphens.
#[must_use]
pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-")
}

/// Strip control characters from a boot script.
///
/// Keeps printable text, LF (`\n`), and TAB (`\t`). Strips everything
/// else in the C0 control range (0x00–0x08, 0x0B–0x0C, 0x0D–0x1F) and
/// DEL (0x7F). This prevents CRLF line endings, null bytes, and other
/// invisible characters from poisoning shell execution.
#[must_use]
pub fn sanitize_boot_script(script: &str) -> String {
    script.chars().filter(|c| is_script_safe_char(*c)).collect()
}

/// Returns `true` if the character is safe for shell script content.
///
/// Safe = printable, LF, or TAB. Everything else in C0 + DEL is rejected.
const fn is_script_safe_char(c: char) -> bool {
    let cp = c as u32;
    // C0 control characters: 0x00–0x1F
    if cp <= 0x1F {
        // Allow LF (0x0A) and TAB (0x09)
        return c == '\n' || c == '\t';
    }
    // DEL is 0x7F
    if cp == 0x7F {
        return false;
    }
    // Everything else (printable ASCII + Unicode) is fine.
    true
}

/// Compute the default install directory for a game server.
///
/// - Linux: `~/game-smith/games/{slug}/`
/// - Windows: `%USERPROFILE%\game-smith\games\{slug}`
#[must_use]
pub fn default_install_dir(name: &str) -> String {
    let slug = slugify(name);
    #[cfg(target_os = "windows")]
    {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| String::from("%USERPROFILE%"));
        format!("{home}\\game-smith\\games\\{slug}")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
        format!("{home}/game-smith/games/{slug}")
    }
}
/// Cross-platform signal constant for termination.
/// Resolves to `libc::SIGTERM` on Linux; `0` on Windows (ignored by `kill_pid`).
#[cfg(target_os = "windows")]
pub const TERM_SIGNAL: i32 = 0;

#[cfg(target_os = "linux")]
use libc;

#[cfg(target_os = "linux")]
pub const TERM_SIGNAL: libc::c_int = libc::SIGTERM;
/// Check if a process is alive by sending signal 0.
/// Returns `true` if the process exists and is accessible.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
#[cfg(target_os = "linux")]
pub fn check_pid_alive(pid: i64) -> bool {
    let result = unsafe { libc::kill(pid as libc::c_int, 0) };
    result == 0
}

/// Check if a process is alive on Windows.
/// Opens the process with `PROCESS_QUERY_INFORMATION` and checks if the
/// handle is still valid.
#[must_use]
#[cfg(target_os = "windows")]
pub fn check_pid_alive(pid: i64) -> bool {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};
    unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, false, pid as u32).is_ok() }
}

/// Send a signal to a process by PID (Linux: `libc::kill`).
/// Returns `Ok(())` on success, `Err` if the process couldn't be signaled.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
#[cfg(target_os = "linux")]
pub fn kill_pid(pid: i64, signal: libc::c_int) -> libc::c_int {
    unsafe { libc::kill(pid as libc::c_int, signal) }
}

/// Send a signal to an entire process group by PGID.
///
/// Passing a negative process ID to `kill(2)` targets the process group
/// rather than a single process. The PGID is stored as a positive `i64`
/// in the database (it's the child's PID after `setpgid(0, 0)`).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
#[cfg(target_os = "linux")]
pub fn kill_process_group(pgid: i64, signal: libc::c_int) -> libc::c_int {
    // Negative PGID signals the entire group.
    unsafe { libc::kill(-(pgid as libc::c_int), signal) }
}

/// Check if any process in a process group is still alive.
///
/// Signal-0 to a negative PGID succeeds if the group exists.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
#[cfg(target_os = "linux")]
pub fn check_process_group_alive(pgid: i64) -> bool {
    unsafe { libc::kill(-(pgid as libc::c_int), 0) == 0 }
}

/// Terminate a process by PID on Windows using `TerminateProcess`.
/// The `_signal` parameter is ignored on Windows (process is always terminated).
#[must_use]
#[cfg(target_os = "windows")]
pub fn kill_pid(pid: i64, _signal: i32) -> bool {
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_TERMINATE, false, pid as u32) }) else {
        return false;
    };
    unsafe { TerminateProcess(handle, 1).is_ok() }
}

/// Check whether this server has any alive processes on the system.
///
/// Queries `command_runs` for runs associated with this server and
/// checks if any recorded PID is still alive. This is ground truth —
/// distinct from `is_running()`, which reflects user intent.
pub async fn is_alive(ctx: &AppContext, server: &Model) -> bool {
    let runs = crate::models::command_runs::Model::find_by_server(ctx, i64::from(server.id))
        .await
        .unwrap_or_default();

    runs.iter().any(|r| r.pid.is_some_and(check_pid_alive))
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert && self.updated_at.is_unchanged() {
            let mut this = self;
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
            Ok(this)
        } else {
            Ok(self)
        }
    }
}

/// Domain operations for creating and querying game server records.
impl ActiveModel {
    /// Create a new game server record in "pending" status.
    ///
    /// # Arguments
    /// * `ctx` - Application context with database connection.
    /// * `app_id` - Steam App ID for the game server.
    /// * `name` - User-defined display name.
    /// * `install_dir` - Absolute path where the game will be installed.
    /// * `platform` - Target platform ("linux", "windows", "macos").
    /// * `server_mod` - Optional mod name for HL1 games.
    /// * `beta_branch` - Optional beta branch name for `app_update`.
    /// * `use_steam_login` - When `true`, use Steam credentials; when `false`, use anonymous login.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        ctx: &AppContext,
        app_id: u32,
        name: String,
        install_dir: String,
        platform: String,
        server_mod: Option<String>,
        beta_branch: Option<String>,
        use_steam_login: bool,
    ) -> Result<Model, ModelError> {
        let now = chrono::Utc::now();
        let record = Self {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::Set(now.into()),
            updated_at: ActiveValue::Set(now.into()),
            app_id: ActiveValue::Set(i32::try_from(app_id).map_err(|e| {
                ModelError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            })?),
            name: ActiveValue::Set(name),
            install_dir: ActiveValue::Set(install_dir),
            platform: ActiveValue::Set(platform),
            status: ActiveValue::Set(ServerStatus::Pending.as_str().to_string()),
            boot_script: ActiveValue::Set(None),
            auto_start: ActiveValue::Set(false),
            auto_restart: ActiveValue::Set(false),
            auto_update: ActiveValue::Set(false),
            update_on_start: ActiveValue::Set(false),
            restart_schedule: ActiveValue::Set(None),
            last_error: ActiveValue::NotSet,
            server_mod: ActiveValue::Set(server_mod),
            beta_branch: ActiveValue::Set(beta_branch),
            use_steam_login: ActiveValue::Set(use_steam_login),
        };
        record.insert(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update the status of a game server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_status(
        &mut self,
        ctx: &AppContext,
        status: ServerStatus,
        last_error: Option<String>,
    ) -> Result<Model, ModelError> {
        self.status = ActiveValue::Set(status.as_str().to_string());
        if last_error.is_some() {
            self.last_error = ActiveValue::Set(last_error);
        }
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update the server status to [`ServerStatus::Stopped`] without clearing
    /// any existing `last_error`.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_stop(&mut self, ctx: &AppContext) -> Result<Model, ModelError> {
        self.status = ActiveValue::Set(ServerStatus::Stopped.as_str().to_string());
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update the server status to [`ServerStatus::Running`].
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_running(&mut self, ctx: &AppContext) -> Result<Model, ModelError> {
        self.status = ActiveValue::Set(ServerStatus::Running.as_str().to_string());
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update the boot script for a game server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_boot_script(
        &mut self,
        ctx: &AppContext,
        boot_script: Option<String>,
    ) -> Result<Model, ModelError> {
        // Sanitize: shell scripts must not contain control characters that
        // poison execution. Keep printable text, LF, and TAB; strip everything
        // else in the C0 control range (including CR from Windows editors).
        let script = boot_script.as_ref().map(|s| sanitize_boot_script(s));
        self.boot_script = ActiveValue::Set(script);
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update the auto-restart setting for a game server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_auto_restart(
        &mut self,
        ctx: &AppContext,
        auto_restart: bool,
    ) -> Result<Model, ModelError> {
        self.auto_restart = ActiveValue::Set(auto_restart);
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update the auto-start setting for a game server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_auto_start(
        &mut self,
        ctx: &AppContext,
        auto_start: bool,
    ) -> Result<Model, ModelError> {
        self.auto_start = ActiveValue::Set(auto_start);
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }

    /// Update multiple server settings in a single database call.
    ///
    /// Updates `name`, `install_dir`, `server_mod`, `beta_branch`,
    /// and `use_steam_login` atomically. Platform is managed by internal
    /// systems and cannot be changed by the user.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_settings(
        &mut self,
        ctx: &AppContext,
        name: String,
        install_dir: String,
        server_mod: Option<String>,
        beta_branch: Option<String>,
        use_steam_login: bool,
    ) -> Result<Model, ModelError> {
        self.name = ActiveValue::Set(name);
        self.install_dir = ActiveValue::Set(install_dir);
        self.server_mod = ActiveValue::Set(server_mod);
        self.beta_branch = ActiveValue::Set(beta_branch);
        self.use_steam_login = ActiveValue::Set(use_steam_login);
        self.clone().update(&ctx.db).await.map_err(ModelError::from)
    }
}

/// Read-oriented helpers on persisted records.
impl Model {
    /// Find a game server by its primary key.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_by_id(ctx: &AppContext, id: i32) -> Result<Option<Self>, ModelError> {
        Entity::find_by_id(id)
            .one(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// List all game servers, ordered by creation time descending.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn list(ctx: &AppContext) -> Result<Vec<Self>, ModelError> {
        Entity::find()
            .order_by_desc(Column::CreatedAt)
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Find game servers currently in "running" status.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_running(ctx: &AppContext) -> Result<Vec<Self>, ModelError> {
        let query = Entity::find().filter(Column::Status.eq(ServerStatus::Running.as_str()));
        query.all(&ctx.db).await.map_err(ModelError::from)
    }

    /// Find game servers that have a living process on the system.
    ///
    /// This is the ground-truth check for shutdown: DB status is *intent*,
    /// not actual state. A server with stale DB status but a live process
    /// (or vice versa) is resolved by checking the PID.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn find_alive(ctx: &AppContext) -> Result<Vec<Self>, ModelError> {
        let candidates = Self::list(ctx).await?;
        let mut alive = Vec::new();
        for s in candidates {
            if is_alive(ctx, &s).await {
                alive.push(s);
            }
        }
        Ok(alive)
    }

    /// Find game servers with auto-start enabled.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database query fails.
    pub async fn find_auto_start(ctx: &AppContext) -> Result<Vec<Self>, ModelError> {
        Entity::find()
            .filter(Column::AutoStart.eq(true))
            .all(&ctx.db)
            .await
            .map_err(ModelError::from)
    }

    /// Returns the Steam App ID as an unsigned value.
    #[must_use]
    pub fn app_id_u32(&self) -> u32 {
        u32::try_from(self.app_id).unwrap_or(0)
    }

    /// Returns the DB status as a typed [`ServerStatus`].
    #[must_use]
    pub fn status(&self) -> ServerStatus {
        self.status.parse().unwrap_or(ServerStatus::Pending)
    }

    /// Check whether this server is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status() == ServerStatus::Running
    }

    /// Check whether this server is installed.
    #[must_use]
    pub fn is_installed(&self) -> bool {
        self.status() == ServerStatus::Installed
    }

    /// Check whether this server is in an error state.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status() == ServerStatus::Error
    }

    /// Start this game server using its boot script or default executable.
    ///
    /// Returns early with `Ok(false)` if the server is already running or
    /// currently installing. Otherwise delegates to
    /// [`GameServerInstaller::start`] and updates the server's status to
    /// [`ServerStatus::Running`] when a process is launched.
    ///
    /// # Errors
    /// Returns a [`GameServerError`] if the start command fails.
    pub async fn start(&self, ctx: &AppContext) -> std::result::Result<bool, GameServerError> {
        // Guard: don't restart if already alive or installing.
        if is_alive(ctx, self).await || self.status() == ServerStatus::Installing {
            return Ok(false);
        }
        let installer = crate::data::game_server_installer::GameServerInstaller::new(ctx);
        let started = installer.start(self).await?.is_some();
        if started {
            let mut active: ActiveModel = self.clone().into();
            crate::log_result(
                active.update_running(ctx).await,
                "updated server status to Running",
                "failed to update server status to Running",
            );
        }
        Ok(started)
    }

    /// Stop this game server gracefully by terminating its running process(es).
    ///
    /// Updates the server status to [`ServerStatus::Stopped`] first to prevent
    /// the worker's auto-restart logic from kicking in, then sends SIGTERM to
    /// all running command runs associated with this server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database update fails.
    pub async fn stop(&self, ctx: &AppContext) -> std::result::Result<(), ModelError> {
        use crate::data::game_server_installer::GameServerInstaller;

        // Update status to Stopped BEFORE sending SIGTERM so the worker's
        // auto-restart logic sees the server is no longer Running.
        let mut active: ActiveModel = self.clone().into();
        crate::log_result(
            active.update_stop(ctx).await,
            "updated server status to Stopped",
            "failed to update server status to Stopped",
        );
        GameServerInstaller::new(ctx).stop(self).await
    }
}

impl Entity {}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn term_signal_is_sigterm() {
        assert_eq!(TERM_SIGNAL, libc::SIGTERM);
    }

    #[test]
    fn check_pid_alive_returns_true_for_self() {
        let pid = std::process::id() as i64;
        assert!(check_pid_alive(pid));
    }

    #[test]
    fn check_pid_alive_returns_false_for_nonexistent() {
        assert!(!check_pid_alive(999999));
    }
}

#[cfg(test)]
mod sanitize_tests {
    use super::*;

    #[test]
    fn sanitize_removes_cr() {
        let out = sanitize_boot_script("echo hello\r\necho world\r\n");
        assert_eq!(out, "echo hello\necho world\n");
    }

    #[test]
    fn sanitize_keeps_lf_and_tab() {
        let out = sanitize_boot_script("line1\n\tindented\n");
        assert_eq!(out, "line1\n\tindented\n");
    }

    #[test]
    fn sanitize_strips_null_bytes() {
        let out = sanitize_boot_script("hello\x00world");
        assert_eq!(out, "helloworld");
    }

    #[test]
    fn sanitize_strips_del() {
        let out = sanitize_boot_script("hello\x7Fworld");
        assert_eq!(out, "helloworld");
    }

    #[test]
    fn sanitize_strips_all_c0_controls() {
        // Feed every C0 character (0x00–0x1F) into the sanitizer.
        let input: String = (0u8..=0x1F).map(|b| b as char).collect();
        let out = sanitize_boot_script(&input);
        // Only \t (0x09) and \n (0x0A) should survive.
        assert_eq!(out, "\t\n");
    }

    #[test]
    fn sanitize_keeps_printable_ascii() {
        let input: String = (0x20u8..=0x7Eu8).map(|b| b as char).collect();
        assert_eq!(sanitize_boot_script(&input), input);
    }

    #[test]
    fn sanitize_keeps_unicode() {
        let out = sanitize_boot_script("café ☕ 你好");
        assert_eq!(out, "café ☕ 你好");
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;

    #[test]
    fn term_signal_is_zero_on_windows() {
        assert_eq!(TERM_SIGNAL, 0);
    }

    #[test]
    fn kill_pid_returns_false_for_nonexistent_process() {
        assert!(!kill_pid(999999, 0));
    }
}
