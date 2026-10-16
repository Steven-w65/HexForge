//! Opt-in measurement of the real presented interface. Scene submission is a
//! different milestone: only an independent observer can supply presentation.
pub mod clock;
#[cfg(windows)]
pub mod desktop;
pub mod diagnostics;
pub mod observer;
pub mod presentation;
pub mod trace;
