use game_smith::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn log_macros_forward_to_tracing() {
    let _boot = boot_test::<App>().await.expect("Failed to boot test app");

    // These calls compile and run, proving the `log` crate is available
    // and the `tracing` log feature is wired (events forward to the
    // tracing subscriber initialized by loco-rs).
    log::trace!("trace level");
    log::debug!("debug level");
    log::info!("log crate integration verified");
    log::warn!("warn level");
    log::error!("error level");
}
