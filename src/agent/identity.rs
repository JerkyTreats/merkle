//! Agent identity and capability model.
//!
//! Runtime identity types used by registry, profile, and CLI.

use crate::error::ApiError;
use crate::agent::profile::metadata_types::AgentMetadata;
use serde::{Deserialize, Serialize};

/// Agent role defining what operations an agent can perform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    /// Reader agents can only query context via GetNode API
    Reader,
    /// Writer agents can create frames via PutFrame API and also read context
    #[serde(alias = "Synthesis")]
    Writer,
}

/// Agent capability (for future extensibility)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Can read context frames
    Read,
    /// Can write context frames
    Write,
}

/// Agent identity with role and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    /// Unique identifier for the agent
    pub agent_id: String,
    /// Role of the agent
    pub role: AgentRole,
    /// Additional capabilities (for future extensibility)
    pub capabilities: Vec<Capability>,
    /// Metadata for agent (e.g., system prompts, custom settings)
    #[serde(default)]
    pub metadata: AgentMetadata,
}

impl AgentIdentity {
    /// Create a new agent identity with the given role
    pub fn new(agent_id: String, role: AgentRole) -> Self {
        let capabilities = match role {
            AgentRole::Reader => vec![Capability::Read],
            AgentRole::Writer => vec![Capability::Read, Capability::Write],
        };

        Self {
            agent_id,
            role,
            capabilities,
            metadata: AgentMetadata::new(),
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
}

/// Validation result for agent configuration
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub agent_id: String,
    pub checks: Vec<(String, bool)>,
    pub errors: Vec<String>,
}

impl ValidationResult {
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            checks: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn add_check(&mut self, description: &str, passed: bool) {
        self.checks.push((description.to_string(), passed));
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty() && self.checks.iter().all(|(_, passed)| *passed)
    }

    pub fn total_checks(&self) -> usize {
        self.checks.len()
    }

    pub fn passed_checks(&self) -> usize {
        self.checks.iter().filter(|(_, passed)| *passed).count()
    }
}
