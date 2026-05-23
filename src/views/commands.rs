use loco_rs::prelude::*;

use crate::models::command_runs::Model as CommandRunModel;

/// Render the command run list page.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn list(v: impl ViewRenderer, runs: &[CommandRunModel]) -> Result<impl IntoResponse> {
    format::render().view(&v, "commands/list.html", data!({ "runs": runs }))
}

/// Render a single command run detail page with live log tailing.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn show(v: impl ViewRenderer, run: &CommandRunModel) -> Result<impl IntoResponse> {
    format::render().view(&v, "commands/detail.html", data!({ "run": run }))
}
