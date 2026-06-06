//! Cross-platform process management.
//!
//! Re-exports platform-specific implementations so callers can use a single
//! import path regardless of target OS.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;
