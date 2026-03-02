//! Agent profile: config shape and validation.

pub mod config;
pub mod metadata_types;
pub mod prompt_contract;
pub mod validation;

pub use config::AgentConfig;
pub use metadata_types::AgentMetadata;
pub use prompt_contract::PromptContract;
pub use validation::validate_agent_config;
