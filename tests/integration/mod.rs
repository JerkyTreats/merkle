//! Integration tests for the Merkle filesystem state management system

mod agent_authorization;
mod agent_cli;
mod blake3_verification;
mod branch_synthesis;
mod config_integration;
mod context_api;
mod context_cli;
mod frame_queue;
mod hasher_verification;
mod init_command;
mod model_providers;
mod node_deletion;
mod progress_observability;
mod provider_cli;
mod store_integration;
mod test_utils;
mod tooling_integration;
mod tree_determinism;
mod tree_structure;
mod unified_status;
mod workspace_commands;
mod workspace_isolation;
mod xdg_config;

pub use test_utils::{with_xdg_data_home, with_xdg_env};
