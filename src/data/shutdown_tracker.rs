//! Shared shutdown state tracker.
//!
//! Holds per-server shutdown status so the `/shutdown/status` API and the
//! shutdown page can report progress while the background task performs
//! the actual stop operations.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

static TRACKER: OnceLock<Arc<Mutex<Option<ShutdownState>>>> = OnceLock::new();

/// Per-server shutdown status.
#[derive(Clone, Debug)]
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

impl Serialize for ServerShutdownStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Pending => serializer.serialize_str("pending"),
            Self::Stopping => serializer.serialize_str("stopping"),
            Self::Stopped => serializer.serialize_str("stopped"),
            Self::Failed(_) => serializer.serialize_str("failed"),
        }
    }
}

/// A single server's shutdown tracking record.
#[derive(Clone, Debug, Serialize)]
pub struct ShutdownServer {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
            .map(|(id, (name, status))| {
                let error = match status {
                    ServerShutdownStatus::Failed(msg) => Some(msg.clone()),
                    _ => None,
                };
                ShutdownServer {
                    id: *id,
                    name: name.clone(),
                    status: status.clone(),
                    error,
                }
            })
            .collect();
        servers.sort_by_key(|s| s.id);
        ShutdownStatus {
            shutting_down: self.shutting_down,
            servers,
        }
    }
}

/// Initialize (or reset) the tracker with a list of servers to shut down.
///
/// Can be called multiple times — resets the tracker state so that each
/// visit to `/shutdown` starts with a fresh view of running servers.
pub async fn init(servers: &[(i32, String)]) {
    let tracker = TRACKER.get_or_init(|| Arc::new(Mutex::new(None)));
    let state = ShutdownState {
        shutting_down: true,
        servers: servers
            .iter()
            .map(|(id, name)| (*id, (name.clone(), ServerShutdownStatus::Pending)))
            .collect(),
    };
    let mut guard = tracker.lock().await;
    *guard = Some(state);
}

/// Get the current shutdown status snapshot.
pub async fn status() -> ShutdownStatus {
    if let Some(tracker) = TRACKER.get() {
        let guard = tracker.lock().await;
        if let Some(ref state) = *guard {
            return state.status();
        }
    }
    ShutdownStatus {
        shutting_down: false,
        servers: Vec::new(),
    }
}

/// Mark a server as `Stopping`.
pub async fn set_stopping(server_id: i32) {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        if let Some(state) = guard.as_mut() {
            if let Some((_, status)) = state.servers.get_mut(&server_id) {
                *status = ServerShutdownStatus::Stopping;
            }
        }
    }
}

/// Mark a server as `Stopped`.
pub async fn set_stopped(server_id: i32) {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        if let Some(state) = guard.as_mut() {
            if let Some((_, status)) = state.servers.get_mut(&server_id) {
                *status = ServerShutdownStatus::Stopped;
            }
        }
    }
}

/// Mark a server as `Failed` with an error message.
pub async fn set_failed(server_id: i32, reason: impl Into<String>) {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        if let Some(state) = guard.as_mut() {
            if let Some((_, status)) = state.servers.get_mut(&server_id) {
                *status = ServerShutdownStatus::Failed(reason.into());
            }
        }
    }
}

/// Signal that all server stop operations are complete. The HTTP server
/// will exit shortly after.
pub async fn mark_complete() {
    if let Some(tracker) = TRACKER.get() {
        let mut guard = tracker.lock().await;
        if let Some(state) = guard.as_mut() {
            state.shutting_down = false;
        }
    }
}
