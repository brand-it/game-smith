use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use socketioxide::extract::{Data, Extension, SocketRef, State};
use socketioxide::SocketIo;
use tokio::sync::RwLock;
use tokio::task::AbortHandle;

use crate::data::command_runner::CommandRunner;
use crate::models::command_runs::Model as CommandRunModel;

/// Payload for the `subscribe` client→server event.
#[derive(Debug, Deserialize)]
pub struct SubscribePayload {
    pub run_id: i32,
}

/// Payload emitted in the `log` server→client event.
#[derive(Debug, Serialize)]
struct LogEvent {
    data: String,
    bytes: u64,
}

/// Payload emitted in the `status` server→client event.
#[derive(Debug, Serialize)]
struct StatusEvent {
    status: String,
    exit_code: Option<i32>,
}

/// Payload emitted in the `error` server→client event.
#[derive(Debug, Serialize)]
struct ErrorEvent {
    message: String,
}

/// State shared with Socket.IO handlers.
/// Holds a clone of the Loco `AppContext` for database access in
/// command-log event handlers.
#[derive(Clone)]
pub struct CommandLogState {
    pub ctx: loco_rs::app::AppContext,
}

/// Per-socket collection of active streaming task abort handles.
///
/// Stored as a socket extension so the disconnect handler can cancel
/// all in-flight poll tasks when the client leaves.
#[derive(Clone, Default)]
pub struct SubscriptionHandles {
    inner: Arc<RwLock<HashMap<i32, AbortHandle>>>,
}

impl SubscriptionHandles {
    /// Create a new empty [`SubscriptionHandles`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new streaming task handle for the given run id.
    pub async fn insert(&self, run_id: i32, handle: AbortHandle) {
        self.inner.write().await.insert(run_id, handle);
    }

    /// Remove the handle for a run id and return it.
    pub async fn remove(&self, run_id: i32) -> Option<AbortHandle> {
        self.inner.write().await.remove(&run_id)
    }

    /// Cancel all tracked streaming tasks and clear the map.
    pub async fn abort_all(&self) {
        let mut guard = self.inner.write().await;
        for handle in guard.values() {
            handle.abort();
        }
        guard.clear();
    }
}

/// Register the `/commands` namespace on the given [`SocketIo`] instance.
///
/// Sets up per-socket subscription tracking, a `subscribe` handler that
/// spawns a poll-based streaming loop, and a disconnect handler that
/// cancels all in-flight poll tasks.
pub fn register_commands_namespace(io: &SocketIo) {
    io.ns("/commands", async |s: SocketRef| {
        tracing::info!(sid = %s.id, "client connected to /commands");

        // Initialize per-socket subscription tracking
        let handles = SubscriptionHandles::new();
        s.extensions.insert(handles.clone());

        // ── Subscribe handler ────────────────────────────────────
        s.on(
            "subscribe",
            |socket: SocketRef,
             Extension(subs): Extension<SubscriptionHandles>,
             Data(payload): Data<SubscribePayload>,
             State(state): State<CommandLogState>| async move {
                let run_id = payload.run_id;
                tracing::info!(run_id, "subscribe requested");

                let ctx = state.ctx.clone();
                let abort_handle = tokio::spawn(stream_log(ctx, socket, run_id)).abort_handle();
                subs.insert(run_id, abort_handle).await;
            },
        );

        // ── Disconnect handler ───────────────────────────────────
        s.on_disconnect({
            let handles_clone = handles;
            async move |socket: SocketRef| {
                tracing::info!(
                    sid = %socket.id,
                    "client disconnected from /commands"
                );
                handles_clone.abort_all().await;
            }
        });
    });
}

/// Stream log output for a command run until it completes or an error
/// occurs.
///
/// Polls the log file at 500ms intervals, emitting `log` events for new
/// content and a final `status` event when the run finishes.
async fn stream_log(ctx: loco_rs::app::AppContext, socket: SocketRef, run_id: i32) {
    // Look up the run record
    let model = match CommandRunModel::find_by_id(&ctx, run_id).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            let _ = socket.emit(
                "error",
                &ErrorEvent {
                    message: "Run not found".to_string(),
                },
            );
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to look up run");
            let _ = socket.emit(
                "error",
                &ErrorEvent {
                    message: "Failed to look up run".to_string(),
                },
            );
            return;
        }
    };

    // If the run is already finished, emit final status and stop
    if !model.is_running() {
        let _ = socket.emit(
            "status",
            &StatusEvent {
                status: model.status,
                exit_code: model.exit_code,
            },
        );
        return;
    }

    let runner = CommandRunner::new(&ctx);
    let poll_interval = tokio::time::Duration::from_millis(500);
    let mut current_offset: u64 = 0;

    loop {
        // Read new content from the log file
        match runner.tail(run_id, Some(current_offset)).await {
            Ok(content) => {
                if !content.is_empty() {
                    let content_len: u64 = content.len() as u64;
                    current_offset += content_len;

                    let _ = socket.emit(
                        "log",
                        &LogEvent {
                            data: content,
                            bytes: current_offset,
                        },
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to read log tail");
                // Continue polling — the file may become readable again
            }
        }

        // Check if the run is still active
        match CommandRunModel::find_by_id(&ctx, run_id).await {
            Ok(Some(current)) => {
                if !current.is_running() {
                    let _ = socket.emit(
                        "status",
                        &StatusEvent {
                            status: current.status,
                            exit_code: current.exit_code,
                        },
                    );
                    break;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to check run status");
                break;
            }
            Ok(None) => {
                tracing::warn!(run_id, "run record disappeared");
                break;
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
