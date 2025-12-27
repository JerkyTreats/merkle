//! Configuration System
//!
//! Runtime-driven configuration system that enables dynamic agent behavior and model provider
//! management. Supports hierarchical configuration with environment variable overrides and
//! runtime validation.

use crate::agent::AgentRole;
use crate::error::ApiError;
use crate::logging::LoggingConfig;
use crate::provider::{CompletionOptions, ModelProvider};
use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleConfig {
    /// Workspace root path (defaults to current directory)
    pub workspace_root: Option<PathBuf>,

    /// Model provider configurations
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,

    /// Agent definitions
    #[serde(default)]
    pub agents: HashMap<String, AgentConfig>,

    /// System-wide settings
    #[serde(default)]
    pub system: SystemConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Model provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type (OpenAI, Anthropic, Ollama, LocalCustom)
    pub provider_type: ProviderType,

    /// Model identifier (e.g., "gpt-4", "claude-3-opus", "llama2")
    pub model: String,

    /// API key (optional, can be loaded from environment)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Base URL or endpoint (provider-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// Default completion options for this provider
    #[serde(default)]
    pub default_options: CompletionOptions,
}

/// Provider type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "local")]
    LocalCustom,
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Unique agent identifier
    pub agent_id: String,

    /// Agent role (Reader, Writer, Synthesis)
    pub role: AgentRole,

    /// System prompt for this agent
    /// This is the primary behavior-defining prompt that guides agent actions when using LLM providers.
    /// The system prompt is used as the System message role when making provider API calls.
    /// If not provided, a default system prompt will be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Model provider to use (references a provider from providers map)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,

    /// Override completion options for this agent (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_options: Option<CompletionOptions>,

    /// Agent-specific metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// System-wide configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Default workspace root (if not specified)
    #[serde(default = "default_workspace_root")]
    pub default_workspace_root: PathBuf,

    /// Storage paths
    #[serde(default)]
    pub storage: StorageConfig,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to node record store (relative to workspace root)
    #[serde(default = "default_store_path")]
    pub store_path: PathBuf,

    /// Path to frame storage (relative to workspace root)
    #[serde(default = "default_frames_path")]
    pub frames_path: PathBuf,
}

// Default value functions
fn default_workspace_root() -> PathBuf {
    PathBuf::from(".")
}

fn default_store_path() -> PathBuf {
    PathBuf::from(".merkle/store")
}

fn default_frames_path() -> PathBuf {
    PathBuf::from(".merkle/frames")
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            default_workspace_root: default_workspace_root(),
            storage: StorageConfig::default(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            store_path: default_store_path(),
            frames_path: default_frames_path(),
        }
    }
}

impl Default for MerkleConfig {
    fn default() -> Self {
        Self {
            workspace_root: None,
            providers: HashMap::new(),
            agents: HashMap::new(),
            system: SystemConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// Configuration validation errors
#[derive(Debug, Clone)]
pub enum ValidationError {
    Provider(String, String),
    Agent(String, String),
    System(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Provider(name, msg) => {
                write!(f, "Provider '{}': {}", name, msg)
            }
            ValidationError::Agent(name, msg) => {
                write!(f, "Agent '{}': {}", name, msg)
            }
            ValidationError::System(msg) => {
                write!(f, "System: {}", msg)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

impl ProviderConfig {
    /// Validate provider configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate model is not empty
        if self.model.trim().is_empty() {
            return Err("Model name cannot be empty".to_string());
        }

        // Validate API key for cloud providers
        match self.provider_type {
            ProviderType::OpenAI | ProviderType::Anthropic => {
                // API key can be in config or environment, so we don't require it here
                // It will be checked when creating the client
            }
            ProviderType::Ollama | ProviderType::LocalCustom => {
                // Local providers don't require API keys
            }
        }

        // Validate endpoint URL if provided
        if let Some(endpoint) = &self.endpoint {
            if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                return Err(format!("Invalid endpoint URL: {}", endpoint));
            }
        }

        // Validate completion options
        if let Some(temp) = self.default_options.temperature {
            if temp < 0.0 || temp > 2.0 {
                return Err(format!("Temperature must be between 0.0 and 2.0, got {}", temp));
            }
        }

        Ok(())
    }

    /// Convert ProviderConfig to ModelProvider
    pub fn to_model_provider(&self) -> Result<ModelProvider, ApiError> {
        // Try to get API key from config or environment
        let api_key = self.api_key.clone().or_else(|| {
            match self.provider_type {
                ProviderType::OpenAI => std::env::var("OPENAI_API_KEY").ok(),
                ProviderType::Anthropic => std::env::var("ANTHROPIC_API_KEY").ok(),
                _ => None,
            }
        });

        match self.provider_type {
            ProviderType::OpenAI => {
                let api_key = api_key.ok_or_else(|| {
                    ApiError::ProviderNotConfigured(
                        "OpenAI API key required (set in config or OPENAI_API_KEY env var)".to_string()
                    )
                })?;
                Ok(ModelProvider::OpenAI {
                    model: self.model.clone(),
                    api_key,
                    base_url: self.endpoint.clone(),
                })
            }
            ProviderType::Anthropic => {
                let api_key = api_key.ok_or_else(|| {
                    ApiError::ProviderNotConfigured(
                        "Anthropic API key required (set in config or ANTHROPIC_API_KEY env var)".to_string()
                    )
                })?;
                Ok(ModelProvider::Anthropic {
                    model: self.model.clone(),
                    api_key,
                })
            }
            ProviderType::Ollama => {
                Ok(ModelProvider::Ollama {
                    model: self.model.clone(),
                    base_url: self.endpoint.clone(),
                })
            }
            ProviderType::LocalCustom => {
                let endpoint = self.endpoint.clone().ok_or_else(|| {
                    ApiError::ProviderNotConfigured(
                        "LocalCustom provider requires endpoint".to_string()
                    )
                })?;
                Ok(ModelProvider::LocalCustom {
                    model: self.model.clone(),
                    endpoint,
                    api_key: api_key,
                })
            }
        }
    }
}

impl AgentConfig {
    /// Validate agent configuration
    pub fn validate(&self, providers: &HashMap<String, ProviderConfig>) -> Result<(), String> {
        // Validate agent_id is not empty
        if self.agent_id.trim().is_empty() {
            return Err("Agent ID cannot be empty".to_string());
        }

        // Validate provider reference exists
        if let Some(provider_name) = &self.provider_name {
            if !providers.contains_key(provider_name) {
                return Err(format!("Provider '{}' not found in providers map", provider_name));
            }
        }

        // Validate system prompt is not empty if provided
        if let Some(ref prompt) = self.system_prompt {
            if prompt.trim().is_empty() {
                return Err("System prompt cannot be empty if provided".to_string());
            }
        }

        Ok(())
    }
}

impl SystemConfig {
    /// Validate system configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate storage paths are not empty
        if self.storage.store_path.as_os_str().is_empty() {
            return Err("Store path cannot be empty".to_string());
        }
        if self.storage.frames_path.as_os_str().is_empty() {
            return Err("Frames path cannot be empty".to_string());
        }

        Ok(())
    }
}

impl MerkleConfig {
    /// Validate the entire configuration
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Validate providers
        for (name, provider) in &self.providers {
            if let Err(e) = provider.validate() {
                errors.push(ValidationError::Provider(name.clone(), e));
            }
        }

        // Validate agents
        for (name, agent) in &self.agents {
            if let Err(e) = agent.validate(&self.providers) {
                errors.push(ValidationError::Agent(name.clone(), e));
            }
        }

        // Validate system config
        if let Err(e) = self.system.validate() {
            errors.push(ValidationError::System(e));
        }

        // Check for duplicate agent IDs
        let mut agent_ids = HashMap::new();
        for (name, agent) in &self.agents {
            if let Some(existing) = agent_ids.insert(&agent.agent_id, name) {
                errors.push(ValidationError::Agent(
                    name.clone(),
                    format!("Duplicate agent_id '{}' (also defined in '{}')", agent.agent_id, existing),
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Configuration loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from files and environment
    pub fn load(workspace_root: &Path) -> Result<MerkleConfig, ConfigError> {
        let config_dir = workspace_root.join("config");
        let merkle_config_dir = workspace_root.join(".merkle");

        let env_name = std::env::var("MERKLE_ENV")
            .unwrap_or_else(|_| "development".to_string());

        let mut builder = Config::builder()
            // Set default values
            .set_default("system.default_workspace_root", ".")?
            .set_default("system.storage.store_path", ".merkle/store")?
            .set_default("system.storage.frames_path", ".merkle/frames")?;

        // Load base config from config/config.toml
        let base_config_path = config_dir.join("config.toml");
        if base_config_path.exists() {
            builder = builder.add_source(
                File::with_name(base_config_path.to_str().unwrap())
                    .required(false)
            );
        }

        // Load .merkle/config.toml if it exists
        let merkle_config_path = merkle_config_dir.join("config.toml");
        if merkle_config_path.exists() {
            builder = builder.add_source(
                File::with_name(merkle_config_path.to_str().unwrap())
                    .required(false)
            );
        }

        // Load environment-specific config
        let env_config_path = config_dir.join(format!("{}.toml", env_name));
        if env_config_path.exists() {
            builder = builder.add_source(
                File::with_name(env_config_path.to_str().unwrap())
                    .required(false)
            );
        }

        // Override with environment variables (MERKLE_* prefix, __ separator)
        builder = builder.add_source(
            Environment::with_prefix("MERKLE")
                .separator("__")
                .try_parsing(true)
        );

        let config = builder.build()?;
        config.try_deserialize()
    }

    /// Load configuration from a specific file
    pub fn load_from_file(path: &Path) -> Result<MerkleConfig, ConfigError> {
        let mut builder = Config::builder()
            // Set default values
            .set_default("system.default_workspace_root", ".")?
            .set_default("system.storage.store_path", ".merkle/store")?
            .set_default("system.storage.frames_path", ".merkle/frames")?;

        builder = builder.add_source(
            File::with_name(path.to_str().unwrap())
        );

        // Override with environment variables
        builder = builder.add_source(
            Environment::with_prefix("MERKLE")
                .separator("__")
                .try_parsing(true)
        );

        let config = builder.build()?;
        config.try_deserialize()
    }

    /// Create default configuration
    pub fn default() -> MerkleConfig {
        MerkleConfig::default()
    }
}

/// Configuration manager for runtime updates
pub struct ConfigManager {
    config: Arc<RwLock<MerkleConfig>>,
}

impl ConfigManager {
    /// Create a new configuration manager with the given config
    pub fn new(config: MerkleConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Reload configuration from files
    pub fn reload(&self, workspace_root: &Path) -> Result<(), ApiError> {
        let new_config = ConfigLoader::load(workspace_root)
            .map_err(|e| ApiError::ConfigError(format!("Failed to load config: {}", e)))?;

        // Validate new configuration
        new_config.validate()
            .map_err(|errors| {
                let error_msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                ApiError::ConfigError(format!("Configuration validation failed:\n{}", error_msgs.join("\n")))
            })?;

        *self.config.write().unwrap() = new_config;
        Ok(())
    }

    /// Get current configuration (read-only)
    pub fn get(&self) -> MerkleConfig {
        self.config.read().unwrap().clone()
    }

    /// Get a mutable reference to the configuration (for runtime updates)
    pub fn get_mut(&mut self) -> &mut MerkleConfig {
        // This requires &mut self, so we need to restructure if we want thread-safe updates
        // For now, we'll use reload for updates
        unimplemented!("Use reload() for configuration updates")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = MerkleConfig::default();
        assert!(config.providers.is_empty());
        assert!(config.agents.is_empty());
        assert_eq!(config.system.default_workspace_root, PathBuf::from("."));
    }

    #[test]
    fn test_provider_config_validation() {
        let mut provider = ProviderConfig {
            provider_type: ProviderType::OpenAI,
            model: "gpt-4".to_string(),
            api_key: Some("test-key".to_string()),
            endpoint: None,
            default_options: CompletionOptions::default(),
        };
        assert!(provider.validate().is_ok());

        // Empty model should fail
        provider.model = "".to_string();
        assert!(provider.validate().is_err());

        // Invalid endpoint should fail
        provider.model = "gpt-4".to_string();
        provider.endpoint = Some("not-a-url".to_string());
        assert!(provider.validate().is_err());
    }

    #[test]
    fn test_agent_config_validation() {
        let mut providers = HashMap::new();
        providers.insert("test-provider".to_string(), ProviderConfig {
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            api_key: None,
            endpoint: None,
            default_options: CompletionOptions::default(),
        });

        let agent = AgentConfig {
            agent_id: "test-agent".to_string(),
            role: AgentRole::Writer,
            system_prompt: Some("Test prompt".to_string()),
            provider_name: Some("test-provider".to_string()),
            completion_options: None,
            metadata: HashMap::new(),
        };
        assert!(agent.validate(&providers).is_ok());

        // Missing provider should fail
        let agent_bad = AgentConfig {
            agent_id: "test-agent-2".to_string(),
            role: AgentRole::Writer,
            system_prompt: None,
            provider_name: Some("nonexistent".to_string()),
            completion_options: None,
            metadata: HashMap::new(),
        };
        assert!(agent_bad.validate(&providers).is_err());
    }

    #[test]
    fn test_config_validation() {
        let mut config = MerkleConfig::default();

        // Add a valid provider
        config.providers.insert("test-provider".to_string(), ProviderConfig {
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            api_key: None,
            endpoint: None,
            default_options: CompletionOptions::default(),
        });

        // Add a valid agent
        config.agents.insert("test-agent".to_string(), AgentConfig {
            agent_id: "test-agent".to_string(),
            role: AgentRole::Writer,
            system_prompt: Some("Test".to_string()),
            provider_name: Some("test-provider".to_string()),
            completion_options: None,
            metadata: HashMap::new(),
        });

        assert!(config.validate().is_ok());

        // Duplicate agent IDs should fail
        config.agents.insert("test-agent-2".to_string(), AgentConfig {
            agent_id: "test-agent".to_string(), // Same ID
            role: AgentRole::Reader,
            system_prompt: None,
            provider_name: None,
            completion_options: None,
            metadata: HashMap::new(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_provider_to_model_provider() {
        let provider_config = ProviderConfig {
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            api_key: None,
            endpoint: Some("http://localhost:11434".to_string()),
            default_options: CompletionOptions::default(),
        };

        let model_provider = provider_config.to_model_provider().unwrap();
        match model_provider {
            ModelProvider::Ollama { model, base_url } => {
                assert_eq!(model, "llama2");
                assert_eq!(base_url, Some("http://localhost:11434".to_string()));
            }
            _ => panic!("Wrong provider type"),
        }
    }

    #[test]
    fn test_config_loader_default() {
        let config = ConfigLoader::default();
        assert!(config.providers.is_empty());
        assert!(config.agents.is_empty());
    }

    #[test]
    fn test_load_from_toml_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("test_config.toml");

        std::fs::write(&config_file, r#"
[system]
default_workspace_root = "."

[system.storage]
store_path = ".merkle/store"
frames_path = ".merkle/frames"

[providers.test-ollama]
provider_type = "ollama"
model = "llama2"
endpoint = "http://localhost:11434"

[agents.test-agent]
agent_id = "test-agent"
role = "Writer"
system_prompt = "Test prompt"
provider_name = "test-ollama"
"#).unwrap();

        let config = ConfigLoader::load_from_file(&config_file).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.agents.len(), 1);

        let provider = config.providers.get("test-ollama").unwrap();
        assert_eq!(provider.model, "llama2");

        let agent = config.agents.get("test-agent").unwrap();
        assert_eq!(agent.agent_id, "test-agent");
        assert_eq!(agent.system_prompt.as_ref().unwrap(), "Test prompt");
    }
}
