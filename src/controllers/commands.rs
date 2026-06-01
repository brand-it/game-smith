use crate::initializers::embedded_i18n::EmbeddedViews;
use axum::routing::get;
use loco_rs::controller::views::ViewEngine;
use loco_rs::prelude::*;

use crate::data::command_runner::CommandRunner;
use crate::models::command_runs::Model as CommandRunModel;
/// GET /commands — list the most recent command runs.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the database query fails or rendering fails.
pub async fn list(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    let runs = CommandRunModel::list_recent(&ctx, 100).await?;
    crate::views::commands::list(v, &runs)
}

/// GET /commands/:id — show a single command run with live log tailing.
///
/// # Errors
/// Returns a [`loco_rs::Error`] if the run is not found, the database query fails,
/// or rendering fails.
pub async fn show(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<EmbeddedViews>,
) -> Result<impl IntoResponse> {
    let run = CommandRunModel::find_by_id(&ctx, id)
        .await?
        .ok_or_else(|| loco_rs::Error::string("Command run not found"))?;
    let runner = CommandRunner::new(&ctx);
    let log_content = runner.tail(id, None).await.unwrap_or_default();
    crate::views::commands::show(v, &run, &log_content)
}

/// Register the commands routes.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("commands")
        .add("/", get(list))
        .add("/{id}", get(show))
}
