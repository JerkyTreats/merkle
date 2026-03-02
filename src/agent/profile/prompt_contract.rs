//! Prompt contract adapter exported by the agent profile domain.

use crate::agent::identity::AgentIdentity;
use crate::agent::profile::metadata_types::AgentMetadata;
use crate::error::ApiError;
use crate::store::NodeType;

pub const KEY_SYSTEM_PROMPT: &str = "system_prompt";
pub const KEY_USER_PROMPT_FILE: &str = "user_prompt_file";
pub const KEY_USER_PROMPT_DIRECTORY: &str = "user_prompt_directory";

#[derive(Debug, Clone)]
pub struct PromptContract {
    pub system_prompt: String,
    pub user_prompt_file: String,
    pub user_prompt_directory: String,
}

impl PromptContract {
    pub fn from_agent(agent: &AgentIdentity) -> Result<Self, ApiError> {
        Self::from_metadata(&agent.agent_id, &agent.metadata)
    }

    pub fn from_metadata(agent_id: &str, metadata: &AgentMetadata) -> Result<Self, ApiError> {
        let system_prompt = get_required(agent_id, metadata, KEY_SYSTEM_PROMPT)?;
        let user_prompt_file = get_required(agent_id, metadata, KEY_USER_PROMPT_FILE)?;
        let user_prompt_directory = get_required(agent_id, metadata, KEY_USER_PROMPT_DIRECTORY)?;
        Ok(Self {
            system_prompt,
            user_prompt_file,
            user_prompt_directory,
        })
    }

    pub fn render_user_prompt(&self, node_type: NodeType, path: &str, file_size: Option<u64>) -> String {
        let template = match node_type {
            NodeType::File { .. } => &self.user_prompt_file,
            NodeType::Directory => &self.user_prompt_directory,
        };

        let mut rendered = template
            .replace("{path}", path)
            .replace(
                "{node_type}",
                match node_type {
                    NodeType::File { .. } => "File",
                    NodeType::Directory => "Directory",
                },
            );

        if let Some(size) = file_size {
            rendered = rendered.replace("{file_size}", &size.to_string());
        }
        rendered
    }
}

fn get_required(agent_id: &str, metadata: &AgentMetadata, key: &'static str) -> Result<String, ApiError> {
    metadata
        .get(key)
        .cloned()
        .ok_or_else(|| ApiError::MissingPromptContractField {
            agent_id: agent_id.to_string(),
            field: key,
        })
}
