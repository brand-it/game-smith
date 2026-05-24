use std::path::PathBuf;

use game_smith::data::steamcmd::SteamCmd;
use game_smith::{resolve_data_home, AppDirs};

#[test]
fn test_steamcmd_paths() {
    let data_home = std::env::temp_dir();
    let dirs = AppDirs::new(data_home.to_string_lossy().to_string());
    let steamcmd = SteamCmd::new(&dirs);

    assert!(
        steamcmd.steamcmd_dir().ends_with("steamcmd"),
        "steamcmd_dir should end with 'steamcmd'"
    );

    #[cfg(target_os = "windows")]
    assert!(
        steamcmd.binary_path().ends_with("steamcmd.exe"),
        "Windows binary should be steamcmd.exe"
    );
    #[cfg(not(target_os = "windows"))]
    assert!(
        steamcmd.binary_path().ends_with("steamcmd.sh"),
        "Linux binary should be steamcmd.sh"
    );
}

#[test]
fn test_is_installed_false() {
    let data_home = std::env::temp_dir();
    let dirs = AppDirs::new(data_home.to_string_lossy().to_string());
    let steamcmd = SteamCmd::new(&dirs);

    assert!(
        !steamcmd.is_installed(),
        "is_installed should return false for non-existent binary"
    );
}

#[test]
fn test_build_update_app_args() {
    let install_dir = PathBuf::from("/opt/games/counter-strike");
    let args = SteamCmd::build_update_app_args(740, &install_dir);

    assert_eq!(args.len(), 6);
    assert_eq!(args[0], "+force_install_dir");
    assert_eq!(args[1], "/opt/games/counter-strike");
    assert_eq!(args[2], "+app_update");
    assert_eq!(args[3], "740");
    assert_eq!(args[4], "+validate");
    assert_eq!(args[5], "+quit");
}

#[test]
fn test_build_update_app_args_hlds() {
    let install_dir = PathBuf::from("/opt/games/hlds");
    let args = SteamCmd::build_update_app_args(90, &install_dir);

    assert_eq!(args[3], "90");
}

#[test]
fn test_missing_deps_error_message() {
    use game_smith::data::steamcmd::SteamCmdError;

    let err = SteamCmdError::MissingDependencies(
        "Install 32-bit libraries: sudo apt-get install lib32gcc-s1".to_string(),
    );
    let msg = err.to_string();
    assert!(msg.contains("32-bit"));
    assert!(msg.contains("Install"));
}

#[test]
fn test_steamcmd_new_uses_data_home() {
    let dirs = AppDirs::new(resolve_data_home());
    let steamcmd = SteamCmd::new(&dirs);

    let path_str = steamcmd.steamcmd_dir().to_string_lossy();
    assert!(
        path_str.contains("game-smith"),
        "Path should contain 'game-smith' app directory"
    );
    assert!(
        path_str.ends_with("steamcmd") || path_str.ends_with("steamcmd/"),
        "Path should end with steamcmd directory"
    );
}

/// Integration test that runs a simple steamcmd command to verify the
/// installed binary is functional.
///
/// This test is marked `#[ignore]` because it requires:
/// - Network access to download SteamCMD (if not already installed)
/// - A working SteamCMD installation on the system
///
/// Run locally with: `cargo test test_steamcmd_run_command -- --ignored --nocapture`
#[tokio::test]
#[ignore]
async fn test_steamcmd_run_command() {
    let data_home = resolve_data_home();
    let dirs = AppDirs::new(data_home);
    let steamcmd = SteamCmd::new(&dirs);

    // Ensure steamcmd is installed
    steamcmd
        .ensure_installed()
        .await
        .expect("Failed to install SteamCMD");

    assert!(
        steamcmd.is_installed(),
        "SteamCMD should be installed after ensure_installed()"
    );

    // Run a simple command: +version prints the version and exits
    let result = steamcmd.run_command(&["+version", "+quit"]).await;

    // The version command may fail if steamcmd requires a login or network,
    // but it should at least be able to spawn
    match result {
        Ok(()) => {}
        Err(game_smith::data::steamcmd::SteamCmdError::ExitStatus(code)) => {
            // Exit status is acceptable for a network-dependent command
            eprintln!("SteamCMD exited with status {code}");
        }
        Err(e) => {
            panic!("Unexpected error: {e}");
        }
    }
}
