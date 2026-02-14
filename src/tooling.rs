//! Tooling & Integration Layer
//!
//! Provides CLI tools, editor hooks, CI integration, and adapters for internal agents.
//! Ensures the context engine can be used from various environments while maintaining
//! determinism and idempotency.

pub mod adapter;
pub mod ci;
pub mod cli;
pub mod editor;
pub mod watch;

pub use adapter::{AgentAdapter, ContextApiAdapter};
pub use ci::{BatchOperation, BatchReport, CiIntegration};
pub use cli::{Cli, CliContext, Commands};
pub use editor::EditorHooks;
pub use watch::{WatchConfig, WatchDaemon};
