use std::io::Write;
/// Resolved application data directories following XDG Base Directory spec.
#[derive(Debug, Clone)]
pub struct AppDirs {
    /// The resolved XDG data home directory.
    pub data_home: String,
    /// Application-specific directory under `data_home` (e.g. `~/.local/share/game-smith`).
    pub app_dir: std::path::PathBuf,
    /// Directory for log files.
    pub logs_dir: std::path::PathBuf,
    /// Path to the persisted JWT secret key file.
    pub secret_path: std::path::PathBuf,
    /// Path to the `SQLite` database file.
    pub db_path: std::path::PathBuf,
}

impl AppDirs {
    /// Derive all sub-paths from the XDG data home directory.
    #[must_use]
    pub fn new(data_home: String) -> Self {
        let app_dir = std::path::PathBuf::from(&data_home).join("game-smith");
        Self {
            logs_dir: app_dir.join("logs"),
            secret_path: app_dir.join("secret_key"),
            db_path: app_dir.join("game-smith.sqlite"),
            app_dir,
            data_home,
        }
    }

    /// Returns the `SQLite` connection URI for the database.
    #[must_use]
    pub fn db_uri(&self) -> String {
        format!("sqlite://{}?mode=rwc", self.db_path.display())
    }
}

/// A persisted JWT signing secret.
///
/// Wraps a secret string to prevent accidental leaking through Debug output
/// and provides load-or-generate semantics for the persisted secret file.
#[derive(Debug, Clone)]
pub struct JwtSecret(String);

impl JwtSecret {
    /// Load an existing secret from `path`, or generate and persist a new one.
    ///
    /// The generated secret is two UUIDs joined with a dash for sufficient
    /// entropy. On Unix, the file is written with mode `0600`.
    ///
    /// # Panics
    /// Panics if the secret file cannot be read or written, or if permissions
    /// cannot be set on Unix.
    #[must_use]
    pub fn load_or_generate(path: &std::path::Path) -> Self {
        let secret = if path.exists() {
            std::fs::read_to_string(path)
                .expect("failed to read secret_key file")
                .trim()
                .to_string()
        } else {
            let new_secret =
                uuid::Uuid::new_v4().to_string() + "-" + &uuid::Uuid::new_v4().to_string();
            std::fs::write(path, &new_secret).expect("failed to write secret_key");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                    .expect("failed to set secret_key permissions");
            }
            eprintln!("game-smith: generated new JWT secret at {}", path.display());
            new_secret
        };
        Self(secret)
    }

    /// Returns the raw secret string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resolve the user's XDG data home directory.
///
/// Reads `$XDG_DATA_HOME`; falls back to `$HOME/.local/share`.
#[must_use]
pub fn resolve_data_home() -> String {
    std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| format!("{}/.local/share", std::env::var("HOME").unwrap_or_default()))
}

/// Create the application's data directories.
///
/// Creates the logs directory so the file appender doesn't emit a warning
/// on first boot. Fatal if it fails.
///
/// # Panics
/// Panics if the logs directory cannot be created.
pub fn create_data_dirs(dirs: &AppDirs) {
    std::fs::create_dir_all(&dirs.logs_dir).expect("failed to create logs dir");
}
pub mod app;
pub mod controllers;
pub mod data;
pub mod initializers;
pub mod mailers;
pub mod models;
pub mod tasks;
pub mod views;
pub mod workers;

pub mod desktop;

/// Install a panic hook that captures structured fields.
///
/// Calls `std::panic::set_hook` with a handler that extracts the panic reason,
/// source location, and backtrace as independent fields for structured logging.
/// Stderr is flushed explicitly to prevent message loss on abrupt process exit.
/// Call this as early as possible in `main()` — before tracing is initialized.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let reason = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic");

        let msg = format!("PANIC: {reason} at {location:?}");
        eprintln!("{msg}");
        let _ = std::io::stderr().flush();

        tracing::error!(
            panic.reason = reason,
            panic.location = location.as_deref(),
            panic.backtrace = %std::backtrace::Backtrace::force_capture(),
            msg,
            "process panicked"
        );
    }));
}
