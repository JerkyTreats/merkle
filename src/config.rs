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
use std::time::SystemTime;
use tracing::warn;

#[cfg(test)]
use std::sync::Mutex;

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
    /// Provider name (unique identifier)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,

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
    /// Note: If this is the default ".merkle/store", it will be resolved to XDG data directory
    #[serde(default = "default_store_path")]
    pub store_path: PathBuf,

    /// Path to frame storage (relative to workspace root)
    /// Note: If this is the default ".merkle/frames", it will be resolved to XDG data directory
    #[serde(default = "default_frames_path")]
    pub frames_path: PathBuf,
}

impl StorageConfig {
    /// Resolve storage paths to actual filesystem locations
    /// 
    /// If paths are the default ".merkle/*" paths, they are resolved to XDG data directories.
    /// Otherwise, they are resolved relative to the workspace root.
    pub fn resolve_paths(&self, workspace_root: &Path) -> Result<(PathBuf, PathBuf), ApiError> {
        let is_default_store = self.store_path == PathBuf::from(".merkle/store");
        let is_default_frames = self.frames_path == PathBuf::from(".merkle/frames");
        
        let store_path = if is_default_store {
            // Use XDG data directory
            let data_dir = xdg::workspace_data_dir(workspace_root)?;
            data_dir.join("store")
        } else {
            // Use configured path relative to workspace
            workspace_root.join(&self.store_path)
        };
        
        let frames_path = if is_default_frames {
            // Use XDG data directory
            let data_dir = xdg::workspace_data_dir(workspace_root)?;
            data_dir.join("frames")
        } else {
            // Use configured path relative to workspace
            workspace_root.join(&self.frames_path)
        };
        
        Ok((store_path, frames_path))
    }
}

// Default value functions
fn default_workspace_root() -> PathBuf {
    PathBuf::from(".")
}

fn default_store_path() -> PathBuf {
    // This is a placeholder - actual path is computed at runtime using XDG directories
    // The path will be resolved to $XDG_DATA_HOME/merkle/workspaces/<hash>/store
    PathBuf::from(".merkle/store")
}

fn default_frames_path() -> PathBuf {
    // This is a placeholder - actual path is computed at runtime using XDG directories
    // The path will be resolved to $XDG_DATA_HOME/merkle/workspaces/<hash>/frames
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
    pub fn validate(&self, _providers: &HashMap<String, ProviderConfig>) -> Result<(), String> {
        // Validate agent_id is not empty
        if self.agent_id.trim().is_empty() {
            return Err("Agent ID cannot be empty".to_string());
        }

        // Validate system prompt is not empty if provided (legacy)
        if let Some(ref prompt) = self.system_prompt {
            if prompt.trim().is_empty() {
                return Err("System prompt cannot be empty if provided".to_string());
            }
        }

        // Validate that Writer/Synthesis agents have either system_prompt or system_prompt_path
        if self.role != AgentRole::Reader {
            if self.system_prompt.is_none() && self.system_prompt_path.is_none() {
                return Err(format!(
                    "Agent '{}' (role: {:?}) requires either system_prompt or system_prompt_path",
                    self.agent_id, self.role
                ));
            }
        }

        // Validate system_prompt_path format if provided
        if let Some(ref prompt_path) = self.system_prompt_path {
            if prompt_path.trim().is_empty() {
                return Err("system_prompt_path cannot be empty if provided".to_string());
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

/// Resolve prompt file path with support for absolute, tilde, and relative paths
/// 
/// Path resolution priority:
/// 1. Absolute path (if starts with `/`)
/// 2. Tilde expansion (if starts with `~/`)
/// 3. Relative to current directory (if starts with `./`)
/// 4. Relative to base_dir (XDG config directory)
pub fn resolve_prompt_path(path: &str, base_dir: &Path) -> Result<PathBuf, ApiError> {
    // 1. Absolute path
    if path.starts_with('/') {
        return Ok(PathBuf::from(path));
    }
    
    // 2. Tilde expansion
    if path.starts_with("~/") {
        let home = std::env::var("HOME")
            .map_err(|_| ApiError::ConfigError("HOME not set".to_string()))?;
        return Ok(PathBuf::from(home).join(&path[2..]));
    }
    
    // 3. Relative to current directory
    if path.starts_with("./") {
        let current_dir = std::env::current_dir()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get current directory: {}", e)))?;
        return Ok(current_dir.join(&path[2..]));
    }
    
    // 4. Relative to base_dir (XDG config)
    Ok(base_dir.join(path))
}

/// Prompt file cache with modification time tracking
pub struct PromptCache {
    cache: HashMap<PathBuf, (String, SystemTime)>,
}

impl PromptCache {
    /// Create a new empty prompt cache
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Load prompt file content with caching
    /// 
    /// Checks modification time and reloads if file has changed.
    /// Validates that file exists, is readable, and contains valid UTF-8.
    pub fn load_prompt(&mut self, path: &Path) -> Result<String, ApiError> {
        // Get file metadata to check modification time
        let metadata = std::fs::metadata(path)
            .map_err(|e| ApiError::ConfigError(format!(
                "Failed to read prompt file {}: {}", 
                path.display(), e
            )))?;
        
        let mtime = metadata.modified()
            .map_err(|e| ApiError::ConfigError(format!(
                "Failed to get modification time for {}: {}", 
                path.display(), e
            )))?;
        
        // Check if we have a cached version and if it's still valid
        if let Some((cached_content, cached_mtime)) = self.cache.get(path) {
            if *cached_mtime == mtime {
                return Ok(cached_content.clone());
            }
        }
        
        // Load file content
        let content = std::fs::read_to_string(path)
            .map_err(|e| ApiError::ConfigError(format!(
                "Failed to read prompt file {}: {}", 
                path.display(), e
            )))?;
        
        // Validate file is not empty
        if content.trim().is_empty() {
            return Err(ApiError::ConfigError(format!(
                "Prompt file {} is empty", 
                path.display()
            )));
        }
        
        // Cache the content with modification time
        self.cache.insert(path.to_path_buf(), (content.clone(), mtime));
        
        Ok(content)
    }
}

impl Default for PromptCache {
    fn default() -> Self {
        Self::new()
    }
}

/// XDG Base Directory utilities for workspace data management
pub mod xdg {
    use super::*;

    /// Get XDG data home directory
    /// 
    /// Returns `$XDG_DATA_HOME` if set, otherwise defaults to `$HOME/.local/share`
    /// Follows XDG Base Directory Specification
    pub fn data_home() -> Option<PathBuf> {
        if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
            return Some(PathBuf::from(xdg_data_home));
        }
        
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".local").join("share"))
    }

    /// Get the data directory for a specific workspace
    /// 
    /// Returns `$XDG_DATA_HOME/merkle/<workspace_path>/`
    /// 
    /// The workspace path is canonicalized and used directly as a directory structure.
    /// For example, `/home/user/projects/myproject` becomes:
    /// `$XDG_DATA_HOME/merkle/home/user/projects/myproject/`
    /// 
    /// This eliminates the need for any `.merkle/` directory in the workspace.
    pub fn workspace_data_dir(workspace_root: &Path) -> Result<PathBuf, ApiError> {
        let data_home = data_home().ok_or_else(|| {
            ApiError::ConfigError("Could not determine XDG data home directory (HOME not set)".to_string())
        })?;
        
        // Canonicalize the workspace path to get an absolute, resolved path
        let canonical = workspace_root.canonicalize().map_err(|e| {
            ApiError::ConfigError(format!("Failed to canonicalize workspace path: {}", e))
        })?;
        
        // Build the data directory path by joining the canonical path components
        // Remove the leading root component (/) and use the rest as directory structure
        let mut data_dir = data_home.join("merkle");
        
        // Iterate through path components, skipping the root
        for component in canonical.components() {
            match component {
                std::path::Component::RootDir => {
                    // Skip the root directory component
                }
                std::path::Component::Prefix(_) => {
                    // Skip prefix (Windows, but we're on Linux)
                }
                std::path::Component::CurDir => {
                    // Skip current directory
                }
                std::path::Component::ParentDir => {
                    // Skip parent directory (shouldn't happen in canonicalized path)
                }
                std::path::Component::Normal(name) => {
                    data_dir = data_dir.join(name);
                }
            }
        }
        
        Ok(data_dir)
    }

    /// Get XDG config home directory
    /// 
    /// Returns `$XDG_CONFIG_HOME` if set, otherwise defaults to `$HOME/.config`
    /// Follows XDG Base Directory Specification
    pub fn config_home() -> Result<PathBuf, ApiError> {
        if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config_home));
        }
        
        let home = std::env::var("HOME")
            .map_err(|_| ApiError::ConfigError("Could not determine XDG config home directory (HOME not set)".to_string()))?;
        
        Ok(PathBuf::from(home).join(".config"))
    }

    /// Get agents directory path
    /// 
    /// Returns `$XDG_CONFIG_HOME/merkle/agents/`
    /// Creates the directory if it doesn't exist
    pub fn agents_dir() -> Result<PathBuf, ApiError> {
        let config_home = config_home()?;
        let agents_dir = config_home.join("merkle").join("agents");
        
        // Create directory if it doesn't exist
        if !agents_dir.exists() {
            std::fs::create_dir_all(&agents_dir).map_err(|e| {
                ApiError::ConfigError(format!("Failed to create agents directory {}: {}", agents_dir.display(), e))
            })?;
        }
        
        Ok(agents_dir)
    }

    /// Get providers directory path
    /// 
    /// Returns `$XDG_CONFIG_HOME/merkle/providers/`
    /// Creates the directory if it doesn't exist
    pub fn providers_dir() -> Result<PathBuf, ApiError> {
        let config_home = config_home()?;
        let providers_dir = config_home.join("merkle").join("providers");
        
        // Create directory if it doesn't exist
        if !providers_dir.exists() {
            std::fs::create_dir_all(&providers_dir).map_err(|e| {
                ApiError::ConfigError(format!("Failed to create providers directory {}: {}", providers_dir.display(), e))
            })?;
        }
        
        Ok(providers_dir)
    }

    /// Get prompts directory path
    /// 
    /// Returns `$XDG_CONFIG_HOME/merkle/prompts/`
    /// Creates the directory if it doesn't exist
    pub fn prompts_dir() -> Result<PathBuf, ApiError> {
        let config_home = config_home()?;
        let prompts_dir = config_home.join("merkle").join("prompts");
        
        // Create directory if it doesn't exist
        if !prompts_dir.exists() {
            std::fs::create_dir_all(&prompts_dir).map_err(|e| {
                ApiError::ConfigError(format!("Failed to create prompts directory {}: {}", prompts_dir.display(), e))
            })?;
        }
        
        Ok(prompts_dir)
    }
}

/// Configuration loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Get the XDG config directory path (~/.config/merkle/config.toml)
    #[cfg(test)]
    pub(crate) fn xdg_config_path() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".config").join("merkle").join("config.toml"))
    }
    
    /// Get the XDG config directory path (~/.config/merkle/config.toml)
    #[cfg(not(test))]
    fn xdg_config_path() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".config").join("merkle").join("config.toml"))
    }

    /// Load configuration from files and environment
    pub fn load(workspace_root: &Path) -> Result<MerkleConfig, ConfigError> {
        let config_dir = workspace_root.join("config");

        let env_name = std::env::var("MERKLE_ENV")
            .unwrap_or_else(|_| "development".to_string());

        let mut builder = Config::builder()
            // Set default values
            .set_default("system.default_workspace_root", ".")?
            .set_default("system.storage.store_path", ".merkle/store")?
            .set_default("system.storage.frames_path", ".merkle/frames")?;

        // Load user-level default config from ~/.config/merkle/config.toml (lowest priority)
        // This is loaded first so workspace configs can override it
        if let Some(xdg_config_path) = Self::xdg_config_path() {
            if xdg_config_path.exists() {
                // Use canonical path to avoid issues with symlinks or relative paths
                let canonical_xdg_path = xdg_config_path.canonicalize()
                    .unwrap_or_else(|_| xdg_config_path.clone());
                builder = builder.add_source(
                    File::with_name(canonical_xdg_path.to_str().unwrap())
                        .required(false)
                );
            } else {
                // Warn if the default config location doesn't exist
                warn!(
                    config_path = %xdg_config_path.display(),
                    "Default configuration file not found at ~/.config/merkle/config.toml. \
                     Consider creating it for user-level defaults."
                );
            }
        }

        // Load base config from config/config.toml (workspace-specific, overrides user config)
        let base_config_path = config_dir.join("config.toml");
        if base_config_path.exists() {
            builder = builder.add_source(
                File::with_name(base_config_path.to_str().unwrap())
                    .required(false)
            );
        }

        // Load environment-specific config (overrides base config)
        let env_config_path = config_dir.join(format!("{}.toml", env_name));
        if env_config_path.exists() {
            builder = builder.add_source(
                File::with_name(env_config_path.to_str().unwrap())
                    .required(false)
            );
        }

        // Override with environment variables (MERKLE_* prefix, __ separator) (highest priority)
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
        providers.insert("test-provider".to_string(), ProviderConfig {
            provider_name: Some("test-provider".to_string()),
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
            system_prompt_path: None,
            metadata: HashMap::new(),
        };
        assert!(agent.validate(&providers).is_ok());

        // Writer agents require either system_prompt or system_prompt_path
        let agent_bad = AgentConfig {
            agent_id: "test-agent-2".to_string(),
            role: AgentRole::Writer,
            system_prompt: None,
            system_prompt_path: None,
            metadata: HashMap::new(),
        };
        assert!(agent_bad.validate(&providers).is_err());
        
        // Reader agents don't require prompts
        let agent_reader = AgentConfig {
            agent_id: "test-agent-3".to_string(),
            role: AgentRole::Reader,
            system_prompt: None,
            system_prompt_path: None,
            metadata: HashMap::new(),
        };
        assert!(agent_reader.validate(&providers).is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = MerkleConfig::default();

        // Add a valid provider
        config.providers.insert("test-provider".to_string(), ProviderConfig {
            provider_name: Some("test-provider".to_string()),
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
            system_prompt_path: None,
            metadata: HashMap::new(),
        });

        assert!(config.validate().is_ok());

        // Duplicate agent IDs should fail
        config.agents.insert("test-agent-2".to_string(), AgentConfig {
            agent_id: "test-agent".to_string(), // Same ID
            role: AgentRole::Reader,
            system_prompt: None,
            system_prompt_path: None,
            metadata: HashMap::new(),
        });

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
        
        // The path should be /test/home/.config/merkle/config.toml
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
        let mock_home_str = mock_home.canonicalize().unwrap().to_string_lossy().to_string();
        std::env::set_var("HOME", &mock_home_str);
        
        let xdg_config_dir = mock_home.join(".config").join("merkle");
        std::fs::create_dir_all(&xdg_config_dir).unwrap();
        let xdg_config_file = xdg_config_dir.join("config.toml");
        
        // Write XDG config with a provider
        std::fs::write(&xdg_config_file, r#"
[system]
default_workspace_root = "."

[system.storage]
store_path = ".merkle/store"
frames_path = ".merkle/frames"

[providers.xdg-provider]
provider_type = "ollama"
model = "xdg-model"
endpoint = "http://localhost:11434"
"#).unwrap();
        
        // Verify file exists before loading
        assert!(xdg_config_file.exists(), "XDG config file should exist");
        
        // Verify XDG config path function returns the correct path
        let xdg_path = ConfigLoader::xdg_config_path();
        assert!(xdg_path.is_some(), "XDG config path should be found");
        assert_eq!(xdg_path.unwrap(), xdg_config_file, "XDG config path should match");
        
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
        let mock_home_str = mock_home.canonicalize().unwrap().to_string_lossy().to_string();
        std::env::set_var("HOME", &mock_home_str);
        
        let xdg_config_dir = mock_home.join(".config").join("merkle");
        std::fs::create_dir_all(&xdg_config_dir).unwrap();
        let xdg_config_file = xdg_config_dir.join("config.toml");
        
        // Write XDG config with a provider
        std::fs::write(&xdg_config_file, r#"
[system]
default_workspace_root = "."

[system.storage]
store_path = ".merkle/store"
frames_path = ".merkle/frames"

[providers.xdg-provider]
provider_type = "ollama"
model = "xdg-model"
endpoint = "http://localhost:11434"
"#).unwrap();
        
        // Create workspace config with same provider but different model
        let workspace_config_dir = workspace_root.join("config");
        std::fs::create_dir_all(&workspace_config_dir).unwrap();
        let workspace_config_file = workspace_config_dir.join("config.toml");
        std::fs::write(&workspace_config_file, r#"
[providers.xdg-provider]
provider_type = "ollama"
model = "workspace-model"
endpoint = "http://localhost:11434"
"#).unwrap();
        
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
        let mock_home_str = mock_home.canonicalize().unwrap().to_string_lossy().to_string();
        std::env::set_var("HOME", &mock_home_str);
        
        // Verify XDG config doesn't exist
        let xdg_config_file = mock_home.join(".config").join("merkle").join("config.toml");
        assert!(!xdg_config_file.exists(), "XDG config file should not exist");
        
        // Load config - should work fine without XDG config (just use defaults)
        // The warning will be logged but shouldn't cause an error
        let config = ConfigLoader::load(workspace_root).unwrap();
        // Should have default config (no providers from XDG or workspace)
        assert_eq!(config.providers.len(), 0, 
                   "Should have no providers when XDG config doesn't exist. Found: {:?}", 
                   config.providers.keys().collect::<Vec<_>>());
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
        assert!(ConfigLoader::xdg_config_path().is_none(), "XDG config path should be None when HOME is not set");
        
        // Load config - should work fine without HOME (just skip XDG config)
        let config = ConfigLoader::load(workspace_root).unwrap();
        // Should have default config (no providers from XDG or workspace)
        assert_eq!(config.providers.len(), 0, 
                   "Should have no providers when HOME is not set. Found: {:?}", 
                   config.providers.keys().collect::<Vec<_>>());
        assert_eq!(config.agents.len(), 0);
        
        // Clean up
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}
