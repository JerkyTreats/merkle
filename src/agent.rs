//! Agent Read/Write Model
//!
//! Defines how agents interact with nodes and context frames. Establishes clear
//! boundaries between read and write operations, ensuring agents can safely
//! operate concurrently while maintaining data integrity.

use crate::error::ApiError;
use crate::provider::ModelProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent role defining what operations an agent can perform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    /// Reader agents can only query context via GetNode API
    Reader,
    /// Writer agents can create frames via PutFrame API and also read context
    Writer,
    /// Synthesis agents are special writer agents that generate branch/directory frames
    Synthesis,
}

/// Agent capability (for future extensibility)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Can read context frames
    Read,
    /// Can write context frames
    Write,
    /// Can synthesize branch frames
    Synthesize,
}

/// Agent identity with role and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    /// Unique identifier for the agent
    pub agent_id: String,
    /// Role of the agent (Reader, Writer, or Synthesis)
    pub role: AgentRole,
    /// Additional capabilities (for future extensibility)
    pub capabilities: Vec<Capability>,
    /// Optional model provider for LLM-powered operations
    pub provider: Option<ModelProvider>,
}

impl AgentIdentity {
    /// Create a new agent identity with the given role
    pub fn new(agent_id: String, role: AgentRole) -> Self {
        let capabilities = match role {
            AgentRole::Reader => vec![Capability::Read],
            AgentRole::Writer => vec![Capability::Read, Capability::Write],
            AgentRole::Synthesis => vec![Capability::Read, Capability::Write, Capability::Synthesize],
        };

        Self {
            agent_id,
            role,
            capabilities,
            provider: None,
        }
    }

    /// Check if the agent has read capability
    pub fn can_read(&self) -> bool {
        self.capabilities.contains(&Capability::Read)
    }

    /// Check if the agent has write capability
    pub fn can_write(&self) -> bool {
        self.capabilities.contains(&Capability::Write)
    }

    /// Check if the agent has synthesize capability
    pub fn can_synthesize(&self) -> bool {
        self.capabilities.contains(&Capability::Synthesize)
    }

    /// Verify that the agent can perform read operations
    pub fn verify_read(&self) -> Result<(), ApiError> {
        if !self.can_read() {
            return Err(ApiError::Unauthorized(format!(
                "Agent {} (role: {:?}) cannot read",
                self.agent_id, self.role
            )));
        }
        Ok(())
    }

    /// Verify that the agent can perform write operations
    pub fn verify_write(&self) -> Result<(), ApiError> {
        if !self.can_write() {
            return Err(ApiError::Unauthorized(format!(
                "Agent {} (role: {:?}) cannot write",
                self.agent_id, self.role
            )));
        }
        Ok(())
    }

    /// Verify that the agent can perform synthesis operations
    pub fn verify_synthesize(&self) -> Result<(), ApiError> {
        if !self.can_synthesize() {
            return Err(ApiError::Unauthorized(format!(
                "Agent {} (role: {:?}) cannot synthesize",
                self.agent_id, self.role
            )));
        }
        Ok(())
    }
}

/// Agent registry for managing agent identities
///
/// In a production system, this would be backed by persistent storage.
/// For Phase 2A, we use an in-memory registry.
pub struct AgentRegistry {
    agents: HashMap<String, AgentIdentity>,
}

impl AgentRegistry {
    /// Create a new empty agent registry
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
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
        self.get(agent_id).ok_or_else(|| {
            ApiError::Unauthorized(format!("Agent not found: {}", agent_id))
        })
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
        assert!(!agent.can_synthesize());
        assert!(agent.verify_read().is_ok());
        assert!(agent.verify_write().is_err());
        assert!(agent.verify_synthesize().is_err());
    }

    #[test]
    fn test_writer_agent() {
        let agent = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
        assert!(agent.can_read());
        assert!(agent.can_write());
        assert!(!agent.can_synthesize());
        assert!(agent.verify_read().is_ok());
        assert!(agent.verify_write().is_ok());
        assert!(agent.verify_synthesize().is_err());
    }

    #[test]
    fn test_synthesis_agent() {
        let agent = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
        assert!(agent.can_read());
        assert!(agent.can_write());
        assert!(agent.can_synthesize());
        assert!(agent.verify_read().is_ok());
        assert!(agent.verify_write().is_ok());
        assert!(agent.verify_synthesize().is_ok());
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
    fn test_agent_with_provider() {
        let mut agent = AgentIdentity::new("agent-with-provider".to_string(), AgentRole::Writer);
        assert!(agent.provider.is_none());

        agent.provider = Some(ModelProvider::Ollama {
            model: "llama2".to_string(),
            base_url: None,
        });

        assert!(agent.provider.is_some());
        match agent.provider.as_ref().unwrap() {
            ModelProvider::Ollama { model, .. } => {
                assert_eq!(model, "llama2");
            }
            _ => panic!("Wrong provider type"),
        }
    }
}
