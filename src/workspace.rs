//! Workspace domain: command orchestration, status assembly, and watch runtime.

mod ci;
mod commands;
mod facade;
mod format;
mod section;
mod types;
mod watch;

pub use facade::*;
