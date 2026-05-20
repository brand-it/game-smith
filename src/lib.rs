use std::io::Write;
pub mod app;
pub mod controllers;
pub mod data;
pub mod initializers;
pub mod mailers;
pub mod models;
pub mod tasks;
pub mod views;
pub mod workers;

pub mod desktop;

/// Install a panic hook that captures structured fields.
///
/// Calls `std::panic::set_hook` with a handler that extracts the panic reason,
/// source location, and backtrace as independent fields for structured logging.
/// Stderr is flushed explicitly to prevent message loss on abrupt process exit.
/// Call this as early as possible in `main()` — before tracing is initialized.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let reason = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic");

        let msg = format!("PANIC: {reason} at {location:?}");
        eprintln!("{msg}");
        let _ = std::io::stderr().flush();

        tracing::error!(
            panic.reason = reason,
            panic.location = location.as_deref(),
            panic.backtrace = %std::backtrace::Backtrace::force_capture(),
            msg,
            "process panicked"
        );
    }));
}
