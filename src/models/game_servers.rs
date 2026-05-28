use super::_entities::game_servers::Column;
pub use super::_entities::game_servers::{ActiveModel, Entity, Model};
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

/// Compute the default install directory for a game server.
///
/// Format: `~/game-smith/games/{slug}/`
#[must_use]
pub fn default_install_dir(name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
    let slug = slugify(name);
    format!("{home}/game-smith/games/{slug}")
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

/// Check whether this server process is actually alive.
///
/// Returns `true` only if there is a running [`command_runs`][super::command_runs::Model] record
/// for this server whose PID corresponds to a living process.
/// The DB status column is ignored — it represents intent, not observation.
pub async fn is_alive(ctx: &AppContext, server: &Model) -> bool {
    let Ok(runs) =
        super::command_runs::Model::find_running_by_server(ctx, i64::from(server.id)).await
    else {
        return false;
    };
    runs.iter().any(|run| run.pid.is_some_and(check_pid_alive))
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
            pid: ActiveValue::NotSet,
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

    /// Update the PID of a running game server.
    ///
    /// # Errors
    /// Returns a [`ModelError`] if the database operation fails.
    pub async fn update_pid(
        &mut self,
        ctx: &AppContext,
        pid: Option<i64>,
    ) -> Result<Model, ModelError> {
        self.pid = ActiveValue::Set(pid);
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
        self.boot_script = ActiveValue::Set(boot_script);
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

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;

    #[test]
    fn term_signal_is_zero_on_windows() {
        assert_eq!(TERM_SIGNAL, 0);
    }

    #[test]
    fn check_pid_alive_returns_true_for_system_process() {
        // PID 4 is the System process on Windows, always alive
        assert!(check_pid_alive(4));
    }

    #[test]
    fn check_pid_alive_returns_false_for_nonexistent_on_windows() {
        assert!(!check_pid_alive(999999));
    }

    #[test]
    fn kill_pid_returns_false_for_nonexistent_process() {
        // Cannot open a non-existent process, so kill_pid returns false
        assert!(!kill_pid(999999, 0));
    }
}
