//! Agent registry: in-memory aggregate of loaded agents.

use crate::agent::identity::{AgentIdentity, AgentRole, ValidationResult};
use crate::agent::profile::AgentConfig;
use crate::agent::profile::prompt_contract::{
    KEY_SYSTEM_PROMPT, KEY_USER_PROMPT_DIRECTORY, KEY_USER_PROMPT_FILE,
};
use crate::agent::storage::AgentStorage;
use crate::error::ApiError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use toml;

/// Agent registry for managing agent identities
///
/// Holds in-memory aggregate and delegates persistence to the storage port.
pub struct AgentRegistry {
    agents: HashMap<String, AgentIdentity>,
    storage: Arc<dyn AgentStorage>,
}

impl AgentRegistry {
    /// Create a new empty agent registry with default XDG storage
    pub fn new() -> Self {
        Self::with_storage(Arc::new(crate::agent::storage::XdgAgentStorage::new()))
    }

    /// Create a registry with a specific storage
    pub fn with_storage(storage: Arc<dyn AgentStorage>) -> Self {
        Self {
            agents: HashMap::new(),
            storage,
        }
    }

    /// Register a new agent
    pub fn register(&mut self, identity: AgentIdentity) {
        self.agents.insert(identity.agent_id.clone(), identity);
    }

    /// Get an agent identity by ID
    pub fn get(&self, agent_id: &str) -> Option<&AgentIdentity> {
        self.agents.get(agent_id)
    }

    /// Get an agent identity by ID or return an error
    pub fn get_or_error(&self, agent_id: &str) -> Result<&AgentIdentity, ApiError> {
        self.get(agent_id)
            .ok_or_else(|| ApiError::Unauthorized(format!("Agent not found: {}", agent_id)))
    }

    /// Get all registered agents
    pub fn list_all(&self) -> Vec<&AgentIdentity> {
        self.agents.values().collect()
    }

    /// Remove an agent from the registry
    pub fn remove(&mut self, agent_id: &str) {
        self.agents.remove(agent_id);
    }

    /// Load agents from configuration
    pub fn load_from_config(
        &mut self,
        config: &crate::config::MerkleConfig,
    ) -> Result<(), ApiError> {
        for (_, agent_config) in &config.agents {
            let mut identity = AgentIdentity::new(agent_config.agent_id.clone(), agent_config.role);

            // Store system prompt in metadata if provided
            if let Some(system_prompt) = &agent_config.system_prompt {
                identity
                    .metadata
                    .insert(KEY_SYSTEM_PROMPT.to_string(), system_prompt.clone());
            }

            // Copy metadata from config
            for (key, value) in &agent_config.metadata {
                identity.metadata.insert(key.clone(), value.clone());
            }

            self.register(identity);
        }
        Ok(())
    }

    /// Load agents from XDG directory via the storage
    pub fn load_from_xdg(&mut self) -> Result<(), ApiError> {
        for stored in self.storage.list()? {
            let mut identity =
                AgentIdentity::new(stored.config.agent_id.clone(), stored.config.role);
            if let Some(prompt) = stored.resolved_system_prompt {
                if !prompt.is_empty() {
                    identity
                        .metadata
                        .insert(KEY_SYSTEM_PROMPT.to_string(), prompt);
                }
            }
            for (key, value) in &stored.config.metadata {
                identity.metadata.insert(key.clone(), value.clone());
            }
            self.agents.insert(stored.agent_id.clone(), identity);
        }
        Ok(())
    }

    /// List agents filtered by role
    pub fn list_by_role(&self, role: Option<AgentRole>) -> Vec<&AgentIdentity> {
        if let Some(filter_role) = role {
            self.agents
                .values()
                .filter(|agent| agent.role == filter_role)
                .collect()
        } else {
            self.list_all()
        }
    }

    /// Get the XDG config file path for an agent
    pub fn agent_config_path(&self, agent_id: &str) -> Result<PathBuf, ApiError> {
        self.storage.path_for(agent_id)
    }

    /// Save agent configuration to XDG directory
    pub fn save_agent_config(&self, agent_id: &str, config: &AgentConfig) -> Result<(), ApiError> {
        self.storage.save(agent_id, config)
    }

    /// Delete agent configuration file
    pub fn delete_agent_config(&self, agent_id: &str) -> Result<(), ApiError> {
        self.storage.delete(agent_id)
    }

    /// Get the agents directory path (for init/cli when they need the directory)
    pub fn agents_dir(&self) -> Result<PathBuf, ApiError> {
        self.storage.agents_dir()
    }

    /// Validate agent configuration and prompt file
    pub fn validate_agent(&self, agent_id: &str) -> Result<ValidationResult, ApiError> {
        let mut result = ValidationResult::new(agent_id.to_string());

        // Check if agent exists in registry
        let agent = match self.get(agent_id) {
            Some(a) => a,
            None => {
                result.add_error("Agent not found in registry".to_string());
                return Ok(result);
            }
        };

        // Get config file path
        let config_path = self.agent_config_path(agent_id)?;

        // Check if config file exists
        if !config_path.exists() {
            result.add_error(format!("Config file not found: {}", config_path.display()));
            return Ok(result);
        }

        // Validate agent ID matches filename
        let expected_filename = format!("{}.toml", agent_id);
        if config_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == expected_filename)
            .unwrap_or(false)
        {
            result.add_check("Agent ID matches filename", true);
        } else {
            result.add_error(format!(
                "Agent ID '{}' doesn't match filename '{}'",
                agent_id,
                config_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
            ));
        }

        // Validate role is valid (should always be valid if loaded)
        result.add_check("Role is valid", true);

        // Load and validate config file
        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) => {
                result.add_error(format!("Failed to read config file: {}", e));
                return Ok(result);
            }
        };

        let agent_config: AgentConfig = match toml::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                result.add_error(format!("Failed to parse config file: {}", e));
                return Ok(result);
            }
        };

        // Validate prompt file if needed
        if agent.role != AgentRole::Reader {
            if let Some(ref prompt_path) = agent_config.system_prompt_path {
                let base_dir = crate::config::xdg::config_home()?.join("meld");

                match crate::agent::prompt::resolve_prompt_path(prompt_path, &base_dir) {
                    Ok(resolved_path) => {
                        // Check if file exists
                        if resolved_path.exists() {
                            result.add_check("Prompt file exists", true);

                            // Check if file is readable
                            match std::fs::metadata(&resolved_path) {
                                Ok(_) => result.add_check("Prompt file is readable", true),
                                Err(e) => {
                                    result.add_error(format!("Prompt file not readable: {}", e))
                                }
                            }

                            // Check if file is valid UTF-8
                            match std::fs::read_to_string(&resolved_path) {
                                Ok(_) => result.add_check("Prompt file is valid UTF-8", true),
                                Err(e) => result
                                    .add_error(format!("Prompt file is not valid UTF-8: {}", e)),
                            }
                        } else {
                            result.add_error(format!(
                                "Prompt file not found: {}",
                                resolved_path.display()
                            ));
                        }
                    }
                    Err(e) => {
                        result.add_error(format!("Failed to resolve prompt path: {}", e));
                    }
                }
            } else {
                result.add_error("Missing system_prompt_path for non-reader role".to_string());
            }

            // Check for user prompt templates in metadata
            if agent.metadata.get(KEY_USER_PROMPT_FILE).is_some() {
                result.add_check("user_prompt_file template present", true);
            } else {
                result.add_error(
                    "Missing user_prompt_file in metadata for non-reader role".to_string(),
                );
            }

            if agent.metadata.get(KEY_USER_PROMPT_DIRECTORY).is_some() {
                result.add_check("user_prompt_directory template present", true);
            } else {
                result.add_error(
                    "Missing user_prompt_directory in metadata for non-reader role".to_string(),
                );
            }
        } else {
            // Reader agents don't need prompts
            result.add_check("Reader agent (no prompt required)", true);
        }

        Ok(result)
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reader_agent() {
        let agent = AgentIdentity::new("reader-1".to_string(), AgentRole::Reader);
        assert!(agent.can_read());
        assert!(!agent.can_write());
        assert!(agent.verify_read().is_ok());
        assert!(agent.verify_write().is_err());
    }

    #[test]
    fn test_writer_agent() {
        let agent = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
        assert!(agent.can_read());
        assert!(agent.can_write());
        assert!(agent.verify_read().is_ok());
        assert!(agent.verify_write().is_ok());
    }

    #[test]
    fn test_agent_registry() {
        let mut registry = AgentRegistry::new();

        let agent1 = AgentIdentity::new("agent-1".to_string(), AgentRole::Reader);
        let agent2 = AgentIdentity::new("agent-2".to_string(), AgentRole::Writer);

        registry.register(agent1);
        registry.register(agent2);

        assert!(registry.get("agent-1").is_some());
        assert!(registry.get("agent-2").is_some());
        assert!(registry.get("agent-3").is_none());

        assert!(registry.get_or_error("agent-1").is_ok());
        assert!(registry.get_or_error("agent-3").is_err());
    }

    #[test]
    fn test_agent_registry_list_all() {
        let mut registry = AgentRegistry::new();

        let agent1 = AgentIdentity::new("agent-1".to_string(), AgentRole::Reader);
        let agent2 = AgentIdentity::new("agent-2".to_string(), AgentRole::Writer);
        let agent3 = AgentIdentity::new("agent-3".to_string(), AgentRole::Writer);

        registry.register(agent1);
        registry.register(agent2);
        registry.register(agent3);

        let all_agents = registry.list_all();
        assert_eq!(all_agents.len(), 3);

        let agent_ids: Vec<String> = all_agents.iter().map(|a| a.agent_id.clone()).collect();
        assert!(agent_ids.contains(&"agent-1".to_string()));
        assert!(agent_ids.contains(&"agent-2".to_string()));
        assert!(agent_ids.contains(&"agent-3".to_string()));
    }

    #[test]
    fn test_list_by_role() {
        let mut registry = AgentRegistry::new();

        let agent1 = AgentIdentity::new("agent-1".to_string(), AgentRole::Reader);
        let agent2 = AgentIdentity::new("agent-2".to_string(), AgentRole::Writer);
        let agent3 = AgentIdentity::new("agent-3".to_string(), AgentRole::Writer);
        let agent4 = AgentIdentity::new("agent-4".to_string(), AgentRole::Writer);

        registry.register(agent1);
        registry.register(agent2);
        registry.register(agent3);
        registry.register(agent4);

        let readers = registry.list_by_role(Some(AgentRole::Reader));
        assert_eq!(readers.len(), 1);
        assert_eq!(readers[0].agent_id, "agent-1");

        let writers = registry.list_by_role(Some(AgentRole::Writer));
        assert_eq!(writers.len(), 3);
        let writer_ids: Vec<String> = writers.iter().map(|a| a.agent_id.clone()).collect();
        assert!(writer_ids.contains(&"agent-2".to_string()));
        assert!(writer_ids.contains(&"agent-3".to_string()));
        assert!(writer_ids.contains(&"agent-4".to_string()));

        let all = registry.list_by_role(None);
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn test_get_agent_config_path() {
        let registry = AgentRegistry::new();
        let path = registry.agent_config_path("test-agent");
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(path.to_string_lossy().contains("test-agent"));
        assert!(path.to_string_lossy().ends_with(".toml"));
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new("test-agent".to_string());

        assert_eq!(result.agent_id, "test-agent");
        assert!(result.is_valid());
        assert_eq!(result.total_checks(), 0);
        assert_eq!(result.passed_checks(), 0);

        result.add_check("Test check 1", true);
        result.add_check("Test check 2", true);
        result.add_check("Test check 3", false);

        assert!(!result.is_valid());
        assert_eq!(result.total_checks(), 3);
        assert_eq!(result.passed_checks(), 2);

        result.add_error("Test error".to_string());
        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 1);
    }
}
