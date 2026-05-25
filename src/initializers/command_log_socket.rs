use async_trait::async_trait;
use axum::Router as AxumRouter;
use loco_rs::{
    app::{AppContext, Initializer},
    Result,
};
use socketioxide::SocketIo;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use crate::initializers::command_namespace;

/// Initializer that layers a Socket.IO server onto the Axum router.
///
/// Registers a `/commands` namespace with:
/// - A `subscribe` handler that spawns a poll-based streaming loop
///   for the given `run_id`.
/// - A disconnect handler that cancels all active streaming tasks.
pub struct CommandLogInitializer;

#[async_trait]
impl Initializer for CommandLogInitializer {
    fn name(&self) -> String {
        "command-log-socket".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
        let ctx = ctx.clone();
        let (layer, io) = SocketIo::builder()
            .with_state(command_namespace::CommandLogState { ctx })
            .build_layer();

        // Register the /commands namespace
        command_namespace::register_commands_namespace(&io);

        // ── Apply CORS + Socket.IO layer ─────────────────────────────
        let router = router.layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .layer(layer),
        );

        Ok(router)
    }
}
