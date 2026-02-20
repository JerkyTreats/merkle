use super::{AgentStorage, StoredAgentConfig};
use crate::agent::identity::AgentRole;
use crate::agent::profile::AgentConfig;
use crate::agent::prompt::{resolve_prompt_path, PromptCache};
use crate::error::ApiError;
use std::ffi::OsStr;
use std::path::PathBuf;
use toml;

pub struct XdgAgentStorage;

impl XdgAgentStorage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for XdgAgentStorage {
    fn default() -> Self {
        Self::new()
    }
}

fn agents_dir() -> Result<PathBuf, ApiError> {
    let config_home = crate::config::xdg::config_home()?;
    let dir = config_home.join("merkle").join("agents");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to create agents directory {}: {}",
                dir.display(),
                e
            ))
        })?;
    }
    Ok(dir)
}

impl AgentStorage for XdgAgentStorage {
    fn list(&self) -> Result<Vec<StoredAgentConfig>, ApiError> {
        let agents_dir = agents_dir()?;
        if !agents_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&agents_dir).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to read agents directory {}: {}",
                agents_dir.display(),
                e
            ))
        })?;

        let base_dir = crate::config::xdg::config_home()?.join("merkle");
        let mut prompt_cache = PromptCache::new();
        let mut loaded = Vec::new();

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(
                        "Failed to read directory entry in {}: {}",
                        agents_dir.display(),
                        e
                    );
                    continue;
                }
            };

            let path = entry.path();
            if path.extension() != Some(OsStr::new("toml")) {
                continue;
            }

            let agent_id = match path.file_stem().and_then(|s| s.to_str()) {
                Some(id) => id.to_string(),
                None => {
                    tracing::warn!("Invalid agent filename (non-UTF8): {:?}", path);
                    continue;
                }
            };

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to read agent config {}: {}", path.display(), e);
                    continue;
                }
            };

            let agent_config: AgentConfig = match toml::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    tracing::error!("Failed to parse agent config {}: {}", path.display(), e);
                    continue;
                }
            };

            if agent_config.agent_id != agent_id {
                tracing::warn!(
                    "Agent ID mismatch in {}: filename={}, config={}",
                    path.display(),
                    agent_id,
                    agent_config.agent_id
                );
            }

            let resolved_system_prompt =
                if let Some(ref prompt_path) = agent_config.system_prompt_path {
                    match resolve_prompt_path(prompt_path, &base_dir) {
                        Ok(resolved_path) => match prompt_cache.load_prompt(&resolved_path) {
                            Ok(prompt) => Some(prompt),
                            Err(e) => {
                                tracing::error!(
                                    "Failed to load prompt file for agent {} ({}): {}",
                                    agent_id,
                                    prompt_path,
                                    e
                                );
                                continue;
                            }
                        },
                        Err(e) => {
                            tracing::error!(
                                "Failed to resolve prompt path for agent {} ({}): {}",
                                agent_id,
                                prompt_path,
                                e
                            );
                            continue;
                        }
                    }
                } else if let Some(ref prompt) = agent_config.system_prompt {
                    Some(prompt.clone())
                } else {
                    if agent_config.role != AgentRole::Reader {
                        tracing::error!(
                            "Agent {} missing system prompt for non-reader role",
                            agent_id
                        );
                        continue;
                    }
                    None
                };

            loaded.push(StoredAgentConfig {
                agent_id: agent_config.agent_id.clone(),
                config: agent_config,
                path,
                resolved_system_prompt,
            });
        }

        Ok(loaded)
    }

    fn path_for(&self, agent_id: &str) -> Result<PathBuf, ApiError> {
        let dir = agents_dir()?;
        Ok(dir.join(format!("{}.toml", agent_id)))
    }

    fn save(&self, agent_id: &str, config: &AgentConfig) -> Result<(), ApiError> {
        let config_path = self.path_for(agent_id)?;
        let dir = agents_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to create agents directory {}: {}",
                dir.display(),
                e
            ))
        })?;
        let toml_content = toml::to_string_pretty(config).map_err(|e| {
            ApiError::ConfigError(format!("Failed to serialize agent config: {}", e))
        })?;
        std::fs::write(&config_path, toml_content).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to write agent config to {}: {}",
                config_path.display(),
                e
            ))
        })?;
        Ok(())
    }

    fn delete(&self, agent_id: &str) -> Result<(), ApiError> {
        let config_path = self.path_for(agent_id)?;
        if !config_path.exists() {
            return Err(ApiError::ConfigError(format!(
                "Agent config file not found: {}",
                config_path.display()
            )));
        }
        std::fs::remove_file(&config_path).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to delete agent config file {}: {}",
                config_path.display(),
                e
            ))
        })
    }

    fn agents_dir(&self) -> Result<PathBuf, ApiError> {
        agents_dir()
    }
}
