use std::io::Write;
pub mod app;
pub mod controllers;
pub mod data;
pub mod desktop;
pub mod initializers;
pub mod models;
pub mod tasks;
pub mod views;
pub mod workers;

/// Resolved application data directories following XDG Base Directory spec.
#[derive(Debug, Clone)]
pub struct AppDirs {
    /// The resolved XDG data home directory.
    pub data_home: String,
    /// Application-specific directory under `data_home` (e.g. `~/.local/share/game-smith`).
    pub app_dir: std::path::PathBuf,
    /// Directory for log files.
    pub logs_dir: std::path::PathBuf,
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
