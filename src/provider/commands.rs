use crate::error::ApiError;
use crate::provider::diagnostics::ProviderDiagnosticsService;
use crate::provider::profile::{ProviderConfig, ProviderType, ValidationResult};
use crate::provider::ProviderRegistry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub struct ProviderCommandService;

/// Result of provider list command.
#[derive(Debug, Clone)]
pub struct ProviderListResult {
    pub providers: Vec<ProviderConfig>,
}

/// Result of provider show (config plus optional API key status).
#[derive(Debug, Clone)]
pub struct ProviderShowResult {
    pub config: ProviderConfig,
    pub api_key_status: Option<String>,
}

/// One row for provider status (mirrors workspace::ProviderStatusEntry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatusEntryResult {
    pub provider_name: String,
    pub provider_type: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connectivity: Option<String>,
}

/// Result of provider create command.
#[derive(Debug, Clone)]
pub struct ProviderCreateResult {
    pub provider_name: String,
    pub config_path: PathBuf,
}

/// Result of provider remove command.
#[derive(Debug, Clone)]
pub struct ProviderRemoveResult {
    pub provider_name: String,
    pub config_path: PathBuf,
}

/// Result of provider test command.
#[derive(Debug, Clone)]
pub struct ProviderTestResult {
    pub provider_name: String,
    pub model_checked: String,
    pub connectivity_ok: bool,
    pub model_available: bool,
    pub available_models: Vec<String>,
    pub error_message: Option<String>,
}

impl ProviderCommandService {
    pub fn parse_provider_type(type_str: &str) -> Result<ProviderType, ApiError> {
        match type_str {
            "openai" => Ok(ProviderType::OpenAI),
            "anthropic" => Ok(ProviderType::Anthropic),
            "ollama" => Ok(ProviderType::Ollama),
            "local" => Ok(ProviderType::LocalCustom),
            _ => Err(ApiError::ConfigError(format!(
                "Invalid type filter: {}. Must be openai, anthropic, ollama, or local",
                type_str
            ))),
        }
    }

    pub fn default_endpoint(provider_type: ProviderType) -> Option<String> {
        match provider_type {
            ProviderType::OpenAI => Some("https://api.openai.com/v1".to_string()),
            ProviderType::Ollama => Some("http://localhost:11434".to_string()),
            ProviderType::LocalCustom | ProviderType::Anthropic => None,
        }
    }

    pub fn required_api_key_env_var(provider_type: ProviderType) -> Option<&'static str> {
        match provider_type {
            ProviderType::OpenAI => Some("OPENAI_API_KEY"),
            ProviderType::Anthropic => Some("ANTHROPIC_API_KEY"),
            ProviderType::Ollama | ProviderType::LocalCustom => None,
        }
    }

    pub fn build_provider_config(
        provider_name: &str,
        provider_type: ProviderType,
        model: String,
        endpoint: Option<String>,
        api_key: Option<String>,
        default_options: crate::provider::CompletionOptions,
    ) -> ProviderConfig {
        ProviderConfig {
            provider_name: Some(provider_name.to_string()),
            provider_type,
            model,
            api_key,
            endpoint,
            default_options,
        }
    }

    pub fn provider_config_path(
        registry: &ProviderRegistry,
        provider_name: &str,
    ) -> Result<PathBuf, ApiError> {
        registry.provider_config_path(provider_name)
    }

    pub fn persist_provider_config(
        registry: &mut ProviderRegistry,
        provider_name: &str,
        config: &ProviderConfig,
    ) -> Result<PathBuf, ApiError> {
        let path = registry.provider_config_path(provider_name)?;
        registry.save_provider_config(provider_name, config)?;
        registry.load_from_xdg()?;
        Ok(path)
    }

    pub fn delete_provider_config(
        registry: &mut ProviderRegistry,
        provider_name: &str,
    ) -> Result<PathBuf, ApiError> {
        let path = registry.provider_config_path(provider_name)?;
        registry.delete_provider_config(provider_name)?;
        registry.load_from_xdg()?;
        Ok(path)
    }

    pub fn load_provider_config_from_path(path: &Path) -> Result<ProviderConfig, ApiError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read config: {}", e)))?;
        toml::from_str(&content)
            .map_err(|e| ApiError::ConfigError(format!("Failed to parse config: {}", e)))
    }

    /// List providers, optionally filtered by type.
    pub fn run_list(
        registry: &ProviderRegistry,
        type_filter: Option<&str>,
    ) -> Result<ProviderListResult, ApiError> {
        let provider_type = type_filter.map(Self::parse_provider_type).transpose()?;
        let providers = registry.list_by_type(provider_type);
        Ok(ProviderListResult {
            providers: providers.into_iter().cloned().collect(),
        })
    }

    /// Show one provider; include_credentials controls API key status in result.
    pub fn run_show(
        registry: &ProviderRegistry,
        provider_name: &str,
        include_credentials: bool,
    ) -> Result<ProviderShowResult, ApiError> {
        let config = registry.get_or_error(provider_name)?.clone();
        let api_key_status = if include_credentials {
            Some(ProviderDiagnosticsService::resolve_api_key_status(&config))
        } else {
            None
        };
        Ok(ProviderShowResult {
            config,
            api_key_status,
        })
    }

    /// Status: list all providers with optional connectivity check.
    pub fn run_status(
        registry: &ProviderRegistry,
        test_connectivity: bool,
    ) -> Result<Vec<ProviderStatusEntryResult>, ApiError> {
        let providers = registry.list_all();
        let mut entries = Vec::new();
        for config in providers {
            let provider_name = config
                .provider_name
                .as_deref()
                .unwrap_or("unknown")
                .to_string();
            let type_str = crate::provider::profile::provider_type_slug(config.provider_type);
            let connectivity = if test_connectivity {
                match ProviderDiagnosticsService::list_available_models(registry, &provider_name) {
                    Ok(_) => Some("ok".to_string()),
                    Err(_) => Some("fail".to_string()),
                }
            } else {
                None
            };
            entries.push(ProviderStatusEntryResult {
                provider_name,
                provider_type: type_str.to_string(),
                model: config.model.clone(),
                connectivity,
            });
        }
        Ok(entries)
    }

    /// Validate a single provider; optional connectivity and model check.
    pub fn run_validate(
        registry: &ProviderRegistry,
        provider_name: &str,
        test_connectivity: bool,
        check_model: bool,
    ) -> Result<ValidationResult, ApiError> {
        let mut result = ProviderDiagnosticsService::validate_provider(registry, provider_name)?;
        if test_connectivity || check_model {
            result.add_check("Provider client created", true);
            match ProviderDiagnosticsService::list_available_models(registry, provider_name) {
                Ok(available_models) => {
                    result.add_check("API connectivity: OK", true);
                    if check_model {
                        let provider = registry.get_or_error(provider_name)?;
                        if available_models.iter().any(|m| m == &provider.model) {
                            result.add_check(
                                &format!("Model '{}' is available", provider.model),
                                true,
                            );
                        } else {
                            result.add_error(format!(
                                "Model '{}' not found. Available models: {}",
                                provider.model,
                                available_models.join(", ")
                            ));
                        }
                    }
                }
                Err(e) => {
                    result.add_error(format!("API connectivity failed: {}", e));
                }
            }
        }
        Ok(result)
    }

    /// Create provider and persist; reloads registry.
    pub fn run_create(
        registry: &mut ProviderRegistry,
        provider_name: &str,
        provider_type: ProviderType,
        model: String,
        endpoint: Option<String>,
        api_key: Option<String>,
        default_options: crate::provider::CompletionOptions,
    ) -> Result<ProviderCreateResult, ApiError> {
        let config = Self::build_provider_config(
            provider_name,
            provider_type,
            model,
            endpoint,
            api_key,
            default_options,
        );
        let path = Self::persist_provider_config(registry, provider_name, &config)?;
        Ok(ProviderCreateResult {
            provider_name: provider_name.to_string(),
            config_path: path,
        })
    }

    /// Remove provider and reload registry.
    pub fn run_remove(
        registry: &mut ProviderRegistry,
        provider_name: &str,
    ) -> Result<ProviderRemoveResult, ApiError> {
        let path = Self::delete_provider_config(registry, provider_name)?;
        Ok(ProviderRemoveResult {
            provider_name: provider_name.to_string(),
            config_path: path,
        })
    }

    /// Test provider connectivity and optional model availability.
    pub fn run_test(
        registry: &ProviderRegistry,
        provider_name: &str,
        model_override: Option<&str>,
        timeout_secs: u64,
    ) -> Result<ProviderTestResult, ApiError> {
        let provider = registry.get_or_error(provider_name)?;
        let model_checked = model_override
            .map(String::from)
            .unwrap_or_else(|| provider.model.clone());
        match ProviderDiagnosticsService::list_available_models_with_timeout(
            registry,
            provider_name,
            timeout_secs,
        ) {
            Ok(available_models) => {
                let model_available = available_models.iter().any(|m| m == &model_checked);
                Ok(ProviderTestResult {
                    provider_name: provider_name.to_string(),
                    model_checked: model_checked.clone(),
                    connectivity_ok: true,
                    model_available,
                    available_models,
                    error_message: None,
                })
            }
            Err(e) => Ok(ProviderTestResult {
                provider_name: provider_name.to_string(),
                model_checked,
                connectivity_ok: false,
                model_available: false,
                available_models: Vec::new(),
                error_message: Some(e.to_string()),
            }),
        }
    }

    /// Update provider by flags (model, endpoint, api_key); does not open editor.
    pub fn run_update_flags(
        registry: &mut ProviderRegistry,
        provider_name: &str,
        model: Option<&str>,
        endpoint: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<(), ApiError> {
        registry.get_or_error(provider_name)?;
        let config_path = Self::provider_config_path(registry, provider_name)?;
        let mut provider_config = Self::load_provider_config_from_path(&config_path)?;
        if let Some(m) = model {
            provider_config.model = m.to_string();
        }
        if let Some(e) = endpoint {
            provider_config.endpoint = Some(e.to_string());
        }
        if let Some(k) = api_key {
            provider_config.api_key = Some(k.to_string());
        }
        Self::persist_provider_config(registry, provider_name, &provider_config)?;
        Ok(())
    }
}
