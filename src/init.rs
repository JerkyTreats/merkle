//! Initialization module for default agents and prompts
//!
//! This module handles the initialization of default agent configurations and
//! prompt files via the `meld init` command. Prompts are embedded in the
//! binary at build time and copied to XDG config directories at runtime.

use crate::agent::{AgentRegistry, AgentRole, AgentStorage, XdgAgentStorage};
use crate::config::{xdg, AgentConfig};
use crate::error::ApiError;

/// Default prompts embedded in binary at compile time
pub const DEFAULT_PROMPTS: &[(&str, &str)] = &[
    (
        "code-analyzer.md",
        include_str!("../prompts/code-analyzer.md"),
    ),
    ("docs-writer.md", include_str!("../prompts/docs-writer.md")),
];

/// Default agent configuration data
struct DefaultAgent {
    id: &'static str,
    role: AgentRole,
    prompt_file: Option<&'static str>,
    user_prompt_file: Option<&'static str>,
    user_prompt_directory: Option<&'static str>,
}

const DEFAULT_AGENTS: &[DefaultAgent] = &[
    DefaultAgent {
        id: "reader",
        role: AgentRole::Reader,
        prompt_file: None,
        user_prompt_file: None,
        user_prompt_directory: None,
    },
    DefaultAgent {
        id: "code-analyzer",
        role: AgentRole::Writer,
        prompt_file: Some("prompts/code-analyzer.md"),
        user_prompt_file: Some("Analyze the code file at {path}. Provide a comprehensive analysis including:\n- Code structure and organization\n- Key functions and their purposes\n- Dependencies and relationships\n- Notable patterns or conventions\n- Potential issues or improvements"),
        user_prompt_directory: Some("Analyze the directory structure at {path}. Provide an overview including:\n- Directory purpose and organization\n- Key files and their roles\n- Module relationships\n- Overall architecture patterns"),
    },
    DefaultAgent {
        id: "docs-writer",
        role: AgentRole::Writer,
        prompt_file: Some("prompts/docs-writer.md"),
        user_prompt_file: Some("Generate comprehensive documentation for the code file at {path}. Include:\n- Purpose and overview\n- API documentation\n- Usage examples\n- Important notes and warnings\n- Related components"),
        user_prompt_directory: Some("Generate documentation for the directory at {path}. Include:\n- Directory purpose and structure\n- Module overview\n- Key components and their roles\n- Usage guidelines"),
    },
];

/// Result of initialization operation
#[derive(Debug, Clone)]
pub struct InitResult {
    pub created: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

impl InitResult {
    fn new() -> Self {
        Self {
            created: Vec::new(),
            skipped: Vec::new(),
            errors: Vec::new(),
        }
    }
}

/// Summary of initialization operations
#[derive(Debug, Clone)]
pub struct InitSummary {
    pub prompts: InitResult,
    pub agents: InitResult,
    pub validation: ValidationSummary,
}

/// Preview of what would be initialized
#[derive(Debug, Clone)]
pub struct InitPreview {
    pub prompts: Vec<String>,
    pub agents: Vec<String>,
}

/// Validation summary for initialized agents
#[derive(Debug, Clone)]
pub struct ValidationSummary {
    pub results: Vec<(String, bool, Vec<String>)>, // (agent_id, is_valid, errors)
}

/// Initialize all default prompts
pub fn initialize_prompts(force: bool) -> Result<InitResult, ApiError> {
    let prompts_dir = xdg::prompts_dir()?;
    let mut result = InitResult::new();

    for (filename, content) in DEFAULT_PROMPTS {
        let prompt_path = prompts_dir.join(filename);

        if prompt_path.exists() && !force {
            result.skipped.push(filename.to_string());
            continue;
        }

        match std::fs::write(&prompt_path, *content) {
            Ok(_) => {
                result.created.push(filename.to_string());
            }
            Err(e) => {
                result.errors.push(format!(
                    "Failed to write prompt file {}: {}",
                    prompt_path.display(),
                    e
                ));
            }
        }
    }

    Ok(result)
}

/// Initialize all default agents
pub fn initialize_agents(force: bool) -> Result<InitResult, ApiError> {
    let agents_dir = XdgAgentStorage::new().agents_dir()?;
    let mut result = InitResult::new();

    for agent in DEFAULT_AGENTS {
        let config_path = agents_dir.join(format!("{}.toml", agent.id));

        if config_path.exists() && !force {
            result.skipped.push(agent.id.to_string());
            continue;
        }

        // Create agent config
        let mut agent_config = AgentConfig {
            agent_id: agent.id.to_string(),
            role: agent.role,
            system_prompt: None,
            system_prompt_path: agent.prompt_file.map(|s| s.to_string()),
            metadata: Default::default(),
        };

        // Add user prompt templates to metadata
        if let Some(user_prompt_file) = agent.user_prompt_file {
            agent_config
                .metadata
                .insert("user_prompt_file".to_string(), user_prompt_file.to_string());
        }
        if let Some(user_prompt_directory) = agent.user_prompt_directory {
            agent_config.metadata.insert(
                "user_prompt_directory".to_string(),
                user_prompt_directory.to_string(),
            );
        }

        // Serialize to TOML
        let toml_content = match toml::to_string_pretty(&agent_config) {
            Ok(content) => content,
            Err(e) => {
                result.errors.push(format!(
                    "Failed to serialize agent config for {}: {}",
                    agent.id, e
                ));
                continue;
            }
        };

        // Write to file
        match std::fs::write(&config_path, toml_content) {
            Ok(_) => {
                result.created.push(agent.id.to_string());
            }
            Err(e) => {
                result.errors.push(format!(
                    "Failed to write agent config {}: {}",
                    config_path.display(),
                    e,
                ));
            }
        }
    }

    Ok(result)
}

/// Initialize all default agents and prompts
pub fn initialize_all(force: bool) -> Result<InitSummary, ApiError> {
    // Ensure all XDG directories exist
    XdgAgentStorage::new().agents_dir()?;
    xdg::providers_dir()?;
    xdg::prompts_dir()?;

    // Initialize prompts first
    let prompts_result = initialize_prompts(force)?;

    // Initialize agents
    let agents_result = initialize_agents(force)?;

    // Validate initialization
    let validation = validate_initialization()?;

    Ok(InitSummary {
        prompts: prompts_result,
        agents: agents_result,
        validation,
    })
}

/// List what would be initialized without actually creating files
pub fn list_initialization() -> Result<InitPreview, ApiError> {
    let prompts_dir = xdg::prompts_dir()?;
    let agents_dir = XdgAgentStorage::new().agents_dir()?;

    let mut prompts = Vec::new();
    let mut agents = Vec::new();

    // Check prompts
    for (filename, _) in DEFAULT_PROMPTS {
        let prompt_path = prompts_dir.join(filename);
        if !prompt_path.exists() {
            prompts.push(filename.to_string());
        }
    }

    // Check agents
    for agent in DEFAULT_AGENTS {
        let config_path = agents_dir.join(format!("{}.toml", agent.id));
        if !config_path.exists() {
            agents.push(agent.id.to_string());
        }
    }

    Ok(InitPreview { prompts, agents })
}

/// Validate all initialized agents
pub fn validate_initialization() -> Result<ValidationSummary, ApiError> {
    let mut registry = AgentRegistry::new();
    registry.load_from_xdg()?;

    let agent_ids = ["reader", "code-analyzer", "docs-writer"];
    let mut results = Vec::new();

    for agent_id in &agent_ids {
        match registry.validate_agent(agent_id) {
            Ok(validation_result) => {
                let is_valid = validation_result.is_valid();
                let errors = validation_result.errors.clone();
                results.push((agent_id.to_string(), is_valid, errors));
            }
            Err(e) => {
                results.push((
                    agent_id.to_string(),
                    false,
                    vec![format!("Validation error: {}", e)],
                ));
            }
        }
    }

    Ok(ValidationSummary { results })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompts_embedded() {
        assert!(!DEFAULT_PROMPTS.is_empty());
        for (filename, content) in DEFAULT_PROMPTS {
            assert!(!filename.is_empty());
            assert!(!content.is_empty());
        }
    }

    #[test]
    fn test_prompt_content_valid() {
        for (_, content) in DEFAULT_PROMPTS {
            // Verify content is valid UTF-8 (this will panic if not)
            let _ = content.to_string();
        }
    }

    #[test]
    fn test_agent_configs_valid() {
        for agent in DEFAULT_AGENTS {
            let mut agent_config = AgentConfig {
                agent_id: agent.id.to_string(),
                role: agent.role,
                system_prompt: None,
                system_prompt_path: agent.prompt_file.map(|s| s.to_string()),
                metadata: Default::default(),
            };

            if let Some(user_prompt_file) = agent.user_prompt_file {
                agent_config
                    .metadata
                    .insert("user_prompt_file".to_string(), user_prompt_file.to_string());
            }
            if let Some(user_prompt_directory) = agent.user_prompt_directory {
                agent_config.metadata.insert(
                    "user_prompt_directory".to_string(),
                    user_prompt_directory.to_string(),
                );
            }

            // Verify it serializes correctly
            let toml_content = toml::to_string_pretty(&agent_config);
            assert!(
                toml_content.is_ok(),
                "Failed to serialize agent config for {}",
                agent.id
            );
        }
    }
}
