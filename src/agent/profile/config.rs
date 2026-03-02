//! Agent configuration schema owned by the agent domain.

use crate::agent::identity::AgentRole;
use crate::agent::profile::metadata_types::AgentMetadata;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Unique agent identifier
    pub agent_id: String,

    /// Agent role (Reader or Writer)
    pub role: AgentRole,

    /// System prompt for this agent (legacy, for backward compatibility)
    /// This is the primary behavior-defining prompt that guides agent actions when using LLM providers.
    /// The system prompt is used as the System message role when making provider API calls.
    /// If not provided, a default system prompt will be used.
    /// Prefer `system_prompt_path` for markdown-based prompts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Path to markdown prompt file (new, preferred)
    /// Path can be absolute, tilde-expanded (~/), relative to current directory (./), or relative to XDG config.
    /// The prompt file will be loaded and cached with modification time tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_path: Option<String>,

    /// Agent-specific metadata
    #[serde(default)]
    pub metadata: AgentMetadata,
}

impl AgentConfig {
    /// Validate agent configuration
    pub fn validate(
        &self,
        _providers: &HashMap<String, crate::provider::ProviderConfig>,
    ) -> Result<(), String> {
        crate::agent::profile::validation::validate_agent_config(self, _providers)
    }
}
