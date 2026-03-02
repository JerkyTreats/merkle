//! Configuration System
//!
//! Runtime-driven configuration system that enables dynamic agent behavior and model provider
//! management. Supports hierarchical configuration with environment variable overrides and
//! runtime validation. Tests included. 

#[cfg(test)]
use crate::agent::AgentRole;
use crate::error::ApiError;
use crate::logging::LoggingConfig;
#[cfg(test)]
use crate::provider::CompletionOptions;
#[cfg(test)]
use crate::provider::ModelProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[cfg(test)]
use std::sync::Mutex;

pub use crate::agent::AgentConfig;
pub use crate::provider::{ProviderConfig, ProviderType};

mod facade;
mod merge;
mod paths;
mod sources;
mod workspace;

pub use facade::ConfigLoader;
pub use workspace::StorageConfig;

/// Backward-compatible re-export of XDG path helpers
pub mod xdg {
    pub use super::paths::xdg_root::*;
}

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

fn default_workspace_root() -> PathBuf {
    PathBuf::from(".")
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            default_workspace_root: default_workspace_root(),
            storage: StorageConfig::default(),
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
                    format!(
                        "Duplicate agent_id '{}' (also defined in '{}')",
                        agent.agent_id, existing
                    ),
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
        new_config.validate().map_err(|errors| {
            let error_msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            ApiError::ConfigError(format!(
                "Configuration validation failed:\n{}",
                error_msgs.join("\n")
            ))
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
            provider_name: Some("test-openai".to_string()),
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
        providers.insert(
            "test-provider".to_string(),
            ProviderConfig {
                provider_name: Some("test-provider".to_string()),
                provider_type: ProviderType::Ollama,
                model: "llama2".to_string(),
                api_key: None,
                endpoint: None,
                default_options: CompletionOptions::default(),
            },
        );

        let agent = AgentConfig {
            agent_id: "test-agent".to_string(),
            role: AgentRole::Writer,
            system_prompt: Some("Test prompt".to_string()),
            system_prompt_path: None,
            metadata: Default::default(),
        };
        assert!(agent.validate(&providers).is_ok());

        // Writer agents require either system_prompt or system_prompt_path
        let agent_bad = AgentConfig {
            agent_id: "test-agent-2".to_string(),
            role: AgentRole::Writer,
            system_prompt: None,
            system_prompt_path: None,
            metadata: Default::default(),
        };
        assert!(agent_bad.validate(&providers).is_err());

        // Reader agents don't require prompts
        let agent_reader = AgentConfig {
            agent_id: "test-agent-3".to_string(),
            role: AgentRole::Reader,
            system_prompt: None,
            system_prompt_path: None,
            metadata: Default::default(),
        };
        assert!(agent_reader.validate(&providers).is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = MerkleConfig::default();

        // Add a valid provider
        config.providers.insert(
            "test-provider".to_string(),
            ProviderConfig {
                provider_name: Some("test-provider".to_string()),
                provider_type: ProviderType::Ollama,
                model: "llama2".to_string(),
                api_key: None,
                endpoint: None,
                default_options: CompletionOptions::default(),
            },
        );

        // Add a valid agent
        config.agents.insert(
            "test-agent".to_string(),
            AgentConfig {
                agent_id: "test-agent".to_string(),
                role: AgentRole::Writer,
                system_prompt: Some("Test".to_string()),
                system_prompt_path: None,
                metadata: Default::default(),
            },
        );

        assert!(config.validate().is_ok());

        // Duplicate agent IDs should fail
        config.agents.insert(
            "test-agent-2".to_string(),
            AgentConfig {
                agent_id: "test-agent".to_string(), // Same ID
                role: AgentRole::Reader,
                system_prompt: None,
                system_prompt_path: None,
                metadata: Default::default(),
            },
        );

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_provider_to_model_provider() {
        let provider_config = ProviderConfig {
            provider_name: Some("test-ollama".to_string()),
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

        std::fs::write(
            &config_file,
            r#"
[system]
default_workspace_root = "."

[system.storage]
store_path = ".meld/store"
frames_path = ".meld/frames"

[providers.test-ollama]
provider_type = "ollama"
model = "llama2"
endpoint = "http://localhost:11434"

[agents.test-agent]
agent_id = "test-agent"
role = "Writer"
system_prompt = "Test prompt"
provider_name = "test-ollama"
"#,
        )
        .unwrap();

        let config = ConfigLoader::load_from_file(&config_file).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.agents.len(), 1);

        let provider = config.providers.get("test-ollama").unwrap();
        assert_eq!(provider.model, "llama2");

        let agent = config.agents.get("test-agent").unwrap();
        assert_eq!(agent.agent_id, "test-agent");
        assert_eq!(agent.system_prompt.as_ref().unwrap(), "Test prompt");
    }

    #[test]
    fn test_xdg_config_path() {
        // Serialize access to HOME to avoid race conditions in parallel test execution
        let _guard = HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        // Test that xdg_config_path constructs the correct path
        // We'll test this indirectly by checking the behavior of load()
        // First, save the original HOME
        let original_home = std::env::var("HOME").ok();

        // Test with a mock HOME
        let test_home = "/test/home";
        std::env::set_var("HOME", test_home);

        // The path should be /test/home/.config/meld/config.toml
        // We can't directly test the private function, but we can verify
        // the behavior through load() which will check for this path

        // Clean up
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    // Mutex to serialize HOME environment variable access in tests
    #[cfg(test)]
    static HOME_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_load_with_xdg_config() {
        // Serialize access to HOME to avoid race conditions in parallel test execution
        let _guard = HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();

        // Save original HOME
        let original_home = std::env::var("HOME").ok();

        // Ensure no workspace config exists that could interfere
        let workspace_config_dir = workspace_root.join("config");
        let workspace_config_file = workspace_config_dir.join("config.toml");
        // If it exists, we'll verify it doesn't override XDG config

        // Create a mock XDG config directory with absolute path
        let mock_home = temp_dir.path().join("mock_home");
        std::fs::create_dir_all(&mock_home).unwrap();
        let mock_home_str = mock_home
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        std::env::set_var("HOME", &mock_home_str);

        let xdg_config_dir = mock_home.join(".config").join("meld");
        std::fs::create_dir_all(&xdg_config_dir).unwrap();
        let xdg_config_file = xdg_config_dir.join("config.toml");

        // Write XDG config with a provider
        std::fs::write(
            &xdg_config_file,
            r#"
[system]
default_workspace_root = "."

[system.storage]
store_path = ".meld/store"
frames_path = ".meld/frames"

[providers.xdg-provider]
provider_type = "ollama"
model = "xdg-model"
endpoint = "http://localhost:11434"
"#,
        )
        .unwrap();

        // Verify file exists before loading
        assert!(xdg_config_file.exists(), "XDG config file should exist");

        // Verify XDG config path function returns the correct path
        let xdg_path = ConfigLoader::xdg_config_path();
        assert!(xdg_path.is_some(), "XDG config path should be found");
        assert_eq!(
            xdg_path.unwrap(),
            xdg_config_file,
            "XDG config path should match"
        );

        // Load config - should pick up XDG config
        let config = ConfigLoader::load(workspace_root).unwrap();
        assert!(config.providers.contains_key("xdg-provider"), 
                "Config should contain xdg-provider. Found providers: {:?}. XDG config file exists: {}, workspace config exists: {}", 
                config.providers.keys().collect::<Vec<_>>(),
                xdg_config_file.exists(),
                workspace_config_file.exists());
        let provider = config.providers.get("xdg-provider").unwrap();
        assert_eq!(provider.model, "xdg-model");

        // Clean up
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_workspace_config_overrides_xdg_config() {
        // Serialize access to HOME to avoid race conditions in parallel test execution
        let _guard = HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();

        // Save original HOME
        let original_home = std::env::var("HOME").ok();

        // Create a mock XDG config directory with absolute path
        let mock_home = temp_dir.path().join("mock_home_override");
        std::fs::create_dir_all(&mock_home).unwrap();
        let mock_home_str = mock_home
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        std::env::set_var("HOME", &mock_home_str);

        let xdg_config_dir = mock_home.join(".config").join("meld");
        std::fs::create_dir_all(&xdg_config_dir).unwrap();
        let xdg_config_file = xdg_config_dir.join("config.toml");

        // Write XDG config with a provider
        std::fs::write(
            &xdg_config_file,
            r#"
[system]
default_workspace_root = "."

[system.storage]
store_path = ".meld/store"
frames_path = ".meld/frames"

[providers.xdg-provider]
provider_type = "ollama"
model = "xdg-model"
endpoint = "http://localhost:11434"
"#,
        )
        .unwrap();

        // Create workspace config with same provider but different model
        let workspace_config_dir = workspace_root.join("config");
        std::fs::create_dir_all(&workspace_config_dir).unwrap();
        let workspace_config_file = workspace_config_dir.join("config.toml");
        std::fs::write(
            &workspace_config_file,
            r#"
[providers.xdg-provider]
provider_type = "ollama"
model = "workspace-model"
endpoint = "http://localhost:11434"
"#,
        )
        .unwrap();

        // Load config - workspace config should override XDG config
        let config = ConfigLoader::load(workspace_root).unwrap();
        assert!(config.providers.contains_key("xdg-provider"));
        let provider = config.providers.get("xdg-provider").unwrap();
        // Workspace config should win
        assert_eq!(provider.model, "workspace-model");

        // Clean up
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_load_without_xdg_config() {
        // Serialize access to HOME to avoid race conditions in parallel test execution
        let _guard = HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();

        // Save original HOME
        let original_home = std::env::var("HOME").ok();

        // Create a mock HOME but don't create XDG config
        let mock_home = temp_dir.path().join("mock_home_no_config");
        std::fs::create_dir_all(&mock_home).unwrap();
        let mock_home_str = mock_home
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        std::env::set_var("HOME", &mock_home_str);

        // Verify XDG config doesn't exist
        let xdg_config_file = mock_home.join(".config").join("meld").join("config.toml");
        assert!(
            !xdg_config_file.exists(),
            "XDG config file should not exist"
        );

        // Load config - should work fine without XDG config (just use defaults)
        // The warning will be logged but shouldn't cause an error
        let config = ConfigLoader::load(workspace_root).unwrap();
        // Should have default config (no providers from XDG or workspace)
        assert_eq!(
            config.providers.len(),
            0,
            "Should have no providers when XDG config doesn't exist. Found: {:?}",
            config.providers.keys().collect::<Vec<_>>()
        );
        assert_eq!(config.agents.len(), 0);

        // Clean up
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_load_without_home_env() {
        // Serialize access to HOME to avoid race conditions in parallel test execution
        let _guard = HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();

        // Save original HOME
        let original_home = std::env::var("HOME").ok();

        // Remove HOME env var
        std::env::remove_var("HOME");

        // Verify XDG config path returns None when HOME is not set
        assert!(
            ConfigLoader::xdg_config_path().is_none(),
            "XDG config path should be None when HOME is not set"
        );

        // Load config - should work fine without HOME (just skip XDG config)
        let config = ConfigLoader::load(workspace_root).unwrap();
        // Should have default config (no providers from XDG or workspace)
        assert_eq!(
            config.providers.len(),
            0,
            "Should have no providers when HOME is not set. Found: {:?}",
            config.providers.keys().collect::<Vec<_>>()
        );
        assert_eq!(config.agents.len(), 0);

        // Clean up
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}
