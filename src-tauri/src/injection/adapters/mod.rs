//! Platform-specific injection adapters.

#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub mod unix;
