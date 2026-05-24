use loco_rs::prelude::*;

/// Render the `SteamCMD` installation status page.
///
/// Shows health status (healthy/not installed/broken), last health check
/// details, and provides an install button if `SteamCMD` is missing or broken.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::needless_pass_by_value)]
pub fn status(
    v: impl ViewRenderer,
    binary_path: &str,
    installed: bool,
    health_status: &str,
    last_check_id: Option<i32>,
    last_check_status: Option<&str>,
) -> Result<impl IntoResponse> {
    format::render().view(
        &v,
        "steamcmd/status.html",
        data!({
            "binary_path": binary_path,
            "installed": installed,
            "health_status": health_status,
            "last_check_id": last_check_id,
            "last_check_status": last_check_status,
        }),
    )
}
