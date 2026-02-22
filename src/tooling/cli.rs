//! CLI tooling: re-exports from CLI domain. No orchestration; single route table lives in `crate::cli`.

pub use crate::cli::{
    AgentCommands, Cli, Commands, ContextCommands, ProviderCommands, WorkspaceCommands,
};
pub use crate::cli::RunContext as CliContext;
