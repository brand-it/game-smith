use loco_rs::prelude::*;

/// Render the `SteamCMD` installation status page.
///
/// Shows whether `SteamCMD` is installed and provides an install button
/// if missing.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn status(
    v: impl ViewRenderer,
    binary_path: &str,
    installed: bool,
) -> Result<impl IntoResponse> {
    format::render().view(
        &v,
        "steamcmd/status.html",
        data!({ "binary_path": binary_path, "installed": installed }),
    )
}
