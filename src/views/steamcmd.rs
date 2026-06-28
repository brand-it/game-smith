use loco_rs::prelude::*;

use crate::data::steamcmd::DistroInfo;

/// Render the `SteamCMD` installation status page.
///
/// Shows health status (healthy/not installed/broken), last health check
/// details, and provides an install button if `SteamCMD` is missing or broken.
///
/// # Errors
/// Returns an error if template rendering fails.
#[allow(clippy::too_many_arguments)]
pub fn status(
    v: &impl ViewRenderer,
    binary_path: &str,
    installed: bool,
    health_status: &str,
    broken_message: Option<&str>,
    last_check_id: Option<i32>,
    last_check_status: Option<&str>,
    platform: &str,
    distro: Option<&DistroInfo>,
) -> Result<impl IntoResponse> {
    format::render().view(
        v,
        "steamcmd/status.html",
        data!({
            "binary_path": binary_path,
            "installed": installed,
            "health_status": health_status,
            "broken_message": broken_message,
            "last_check_id": last_check_id,
            "last_check_status": last_check_status,
            "platform": platform,
            "distro_label": distro.as_ref().map(|d| &d.label),
            "distro_command": distro.as_ref().map(|d| &d.install_command),
        }),
    )
}
