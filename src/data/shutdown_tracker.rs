//! Shared shutdown state tracker.
//!
//! Holds per-server shutdown status so the `/shutdown/status` API and the
//! shutdown page can report progress while the background task performs
//! the actual stop operations.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

static TRACKER: OnceLock<Arc<Mutex<ShutdownState>>> = OnceLock::new();

/// Per-server shutdown status.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerShutdownStatus {
    /// Shutdown has not yet started for this server.
    Pending,
    /// Termination signal sent; waiting for process to exit.
    Stopping,
    /// Server process confirmed stopped.
    Stopped,
    /// Failed to stop (includes error reason).
    Failed(String),
}

/// A single server's shutdown tracking record.
#[derive(Clone, Debug, Serialize)]
pub struct ShutdownServer {
    pub id: i32,
    pub name: String,
    pub status: ServerShutdownStatus,
}

/// Top-level shutdown state exposed via the status API.
#[derive(Clone, Debug, Serialize)]
pub struct ShutdownStatus {
    /// Whether a shutdown sequence is currently in progress.
    pub shutting_down: bool,
    /// Per-server status.
    pub servers: Vec<ShutdownServer>,
}

/// Internal mutable state protected by a Mutex.
#[derive(Clone, Debug, Default)]
struct ShutdownState {
    shutting_down: bool,
    /// Server ID → (name, status).
    servers: HashMap<i32, (String, ServerShutdownStatus)>,
}

impl ShutdownState {
    fn status(&self) -> ShutdownStatus {
        let mut servers: Vec<ShutdownServer> = self
            .servers
            .iter()
            .map(|(id, (name, status))| ShutdownServer {
                id: *id,
                name: name.clone(),
                status: status.clone(),
            })
            .collect();
        servers.sort_by_key(|s| s.id);
        ShutdownStatus {
            shutting_down: self.shutting_down,
            servers,
        }
    }
}

/// Initialize the tracker with a list of servers to shut down.
///
/// Idempotent — subsequent calls are no-ops while the tracker is active.
/// Returns `false` if the tracker was already initialized (another shutdown
/// is already in progress).
pub fn init(servers: &[(i32, String)]) -> bool {
    let state = ShutdownState {
        shutting_down: true,
        servers: servers
            .iter()
            .map(|(id, name)| (*id, (name.clone(), ServerShutdownStatus::Pending)))
            .collect(),
    };

    match TRACKER.set(Arc::new(Mutex::new(state))) {
        Ok(()) => true,
        Err(_) => false,
    }
}

/// Get the current shutdown status snapshot.
pub async fn status() -> ShutdownStatus {
    if let Some(tracker) = TRACKER.get() {
        let guard = tracker.lock().await;
        guard.status()
    } else {
        ShutdownStatus {
            shutting_down: false,
            servers: Vec::new(),
        }
    }
}

/// Mark a server as `Stopping`.
pub async fn set_stopping(server_id: i32) {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        if let Some((_, status)) = guard.servers.get_mut(&server_id) {
            *status = ServerShutdownStatus::Stopping;
        }
    }
}

/// Mark a server as `Stopped`.
pub async fn set_stopped(server_id: i32) {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        if let Some((_, status)) = guard.servers.get_mut(&server_id) {
            *status = ServerShutdownStatus::Stopped;
        }
    }
}

/// Mark a server as `Failed` with an error message.
pub async fn set_failed(server_id: i32, reason: impl Into<String>) {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        if let Some((_, status)) = guard.servers.get_mut(&server_id) {
            *status = ServerShutdownStatus::Failed(reason.into());
        }
    }
}

/// Signal that all server stop operations are complete. The HTTP server
/// will exit shortly after.
pub async fn mark_complete() {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        guard.shutting_down = false;
    }
}
