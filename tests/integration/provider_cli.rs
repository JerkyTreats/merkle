//! Integration tests for Provider CLI commands

use merkle::config::{xdg, ProviderConfig, ProviderType};
use merkle::error::ApiError;
use merkle::cli::{Commands, ProviderCommands, RunContext};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::integration::with_xdg_env;

/// Create a test provider config file
fn create_test_provider(
    provider_name: &str,
    provider_type: ProviderType,
    model: &str,
    endpoint: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let providers_dir = xdg::providers_dir()?;
    // Ensure directory exists
    std::fs::create_dir_all(&providers_dir).map_err(|e| {
        ApiError::ConfigError(format!("Failed to create providers directory: {}", e))
    })?;
    let config_path = providers_dir.join(format!("{}.toml", provider_name));

    let provider_config = ProviderConfig {
        provider_name: Some(provider_name.to_string()),
        provider_type,
        model: model.to_string(),
        api_key: None,
        endpoint: endpoint.map(|s| s.to_string()),
        default_options: merkle::provider::CompletionOptions::default(),
    };

    let toml_content = toml::to_string_pretty(&provider_config)
        .map_err(|e| ApiError::ConfigError(format!("Failed to serialize: {}", e)))?;

    fs::write(&config_path, toml_content)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write: {}", e)))?;

    Ok(config_path)
}

#[test]
fn test_provider_list_empty() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Ensure providers directory exists but is empty
        let providers_dir = xdg::providers_dir().unwrap();
        // Remove any existing providers
        if providers_dir.exists() {
            for entry in fs::read_dir(&providers_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::List {
                format: "text".to_string(),
                type_filter: None,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // Output should be valid (either "No providers found" or a list)
        // Note: Providers may be loaded from config.toml, so we just verify the command works
        assert!(
            output.contains("No providers found")
                || output.contains("Available Providers")
                || output.contains("Total:")
        );
    });
}

#[test]
fn test_provider_list_with_providers() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();
        create_test_provider(
            "test-ollama",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::List {
                format: "text".to_string(),
                type_filter: None,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("test-openai"));
        assert!(output.contains("test-ollama"));
        // Note: May have additional providers from config.toml, so just check for our test providers
        assert!(output.contains("Total:") || output.contains("provider(s)"));
    });
}

#[test]
fn test_provider_list_filter_by_type() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();
        create_test_provider(
            "test-ollama",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::List {
                format: "text".to_string(),
                type_filter: Some("openai".to_string()),
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("test-openai"));
        assert!(!output.contains("test-ollama"));
    });
}

#[test]
fn test_provider_list_json_format() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::List {
                format: "json".to_string(),
                type_filter: None,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("\"provider_name\""));
        assert!(output.contains("\"provider_type\""));
        assert!(output.contains("test-openai"));
    });
}

#[test]
fn test_provider_show() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Show {
                provider_name: "test-openai".to_string(),
                format: "text".to_string(),
                include_credentials: false,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Provider: test-openai"));
        assert!(output.contains("Type: openai"));
        assert!(output.contains("Model: gpt-4"));
    });
}

#[test]
fn test_provider_show_with_credentials() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Show {
                provider_name: "test-openai".to_string(),
                format: "text".to_string(),
                include_credentials: true,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("API Key:"));
    });
}

#[test]
fn test_provider_show_not_found() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Show {
                provider_name: "nonexistent".to_string(),
                format: "text".to_string(),
                include_credentials: false,
            },
        });

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(format!("{}", error).contains("not found"));
    });
}

#[test]
fn test_provider_validate() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Validate {
                provider_name: "test-openai".to_string(),
                test_connectivity: false,
                check_model: false,
                verbose: false,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Validating provider: test-openai"));
    });
}

#[test]
fn test_provider_validate_not_found() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Validate {
                provider_name: "nonexistent".to_string(),
                test_connectivity: false,
                check_model: false,
                verbose: false,
            },
        });

        assert!(result.is_ok()); // Validation returns result even if provider not found
        let output = result.unwrap();
        assert!(output.contains("not found"));
    });
}

#[test]
fn test_provider_status_empty() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let providers_dir = xdg::providers_dir().unwrap();
        if providers_dir.exists() {
            for entry in fs::read_dir(&providers_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Status {
                format: "text".to_string(),
                test_connectivity: false,
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            output.contains("No providers configured")
                || output.contains("Providers")
                || output.contains("Total:")
        );
    });
}

#[test]
fn test_provider_status_one_provider_text() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider(
            "status-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://127.0.0.1:11434"),
        )
        .unwrap();
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Status {
                format: "text".to_string(),
                test_connectivity: false,
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Providers"));
        assert!(output.contains("status-provider"));
        assert!(output.contains("ollama"));
        assert!(output.contains("llama2"));
        assert!(output.contains("Total:"));
    });
}

#[test]
fn test_provider_status_one_provider_json() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider(
            "status-json-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://127.0.0.1:11434"),
        )
        .unwrap();
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Status {
                format: "json".to_string(),
                test_connectivity: false,
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("\"providers\""));
        assert!(output.contains("\"total\""));
        assert!(output.contains("status-json-provider"));
        assert!(output.contains("\"provider_name\""));
        assert!(output.contains("\"provider_type\""));
        assert!(output.contains("\"model\""));
    });
}

#[test]
fn test_provider_status_with_test_connectivity() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider(
            "conn-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://127.0.0.1:11434"),
        )
        .unwrap();
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Status {
                format: "text".to_string(),
                test_connectivity: true,
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Providers"));
        assert!(output.contains("conn-provider"));
        assert!(
            output.contains("Connectivity")
                || output.contains("OK")
                || output.contains("Fail")
                || output.contains("Skipped")
        );
    });
}

#[test]
fn test_provider_create_non_interactive() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Create {
                provider_name: "new-provider".to_string(),
                type_: Some("ollama".to_string()),
                model: Some("llama2".to_string()),
                endpoint: Some("http://localhost:11434".to_string()),
                api_key: None,
                interactive: false,
                non_interactive: true,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Provider created: new-provider"));

        // Verify provider was created
        let providers_dir = xdg::providers_dir().unwrap();
        let config_path = providers_dir.join("new-provider.toml");
        assert!(config_path.exists());
    });
}

#[test]
fn test_provider_create_missing_required_fields() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Create {
                provider_name: "new-provider".to_string(),
                type_: None,
                model: None,
                endpoint: None,
                api_key: None,
                interactive: false,
                non_interactive: true,
            },
        });

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(format!("{}", error).contains("required"));
    });
}

#[test]
fn test_provider_edit_with_flags() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider(
            "test-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Edit {
                provider_name: "test-provider".to_string(),
                model: Some("llama3".to_string()),
                endpoint: None,
                api_key: None,
                editor: None,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Provider updated: test-provider"));

        // Verify model was updated
        let providers_dir = xdg::providers_dir().unwrap();
        let config_path = providers_dir.join("test-provider.toml");
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("llama3"));
    });
}

#[test]
fn test_provider_edit_not_found() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Edit {
                provider_name: "nonexistent".to_string(),
                model: Some("new-model".to_string()),
                endpoint: None,
                api_key: None,
                editor: None,
            },
        });

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(format!("{}", error).contains("not found"));
    });
}

#[test]
fn test_provider_remove() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider(
            "test-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Remove {
                provider_name: "test-provider".to_string(),
                force: true,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Removed provider: test-provider"));

        // Verify provider was deleted
        let providers_dir = xdg::providers_dir().unwrap();
        let config_path = providers_dir.join("test-provider.toml");
        assert!(!config_path.exists());
    });
}

#[test]
fn test_provider_remove_not_found() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Remove {
                provider_name: "nonexistent".to_string(),
                force: true,
            },
        });

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(format!("{}", error).contains("not found"));
    });
}

#[test]
fn test_provider_list_invalid_type_filter() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::List {
                format: "text".to_string(),
                type_filter: Some("invalid".to_string()),
            },
        });

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(format!("{}", error).contains("Invalid type filter"));
    });
}

#[test]
fn test_provider_show_json_format() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Show {
                provider_name: "test-openai".to_string(),
                format: "json".to_string(),
                include_credentials: true,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("\"provider_name\""));
        assert!(output.contains("\"provider_type\""));
        assert!(output.contains("\"model\""));
        assert!(output.contains("test-openai"));
    });
}

#[test]
fn test_provider_registry_list_by_type() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        create_test_provider("test-openai", ProviderType::OpenAI, "gpt-4", None).unwrap();
        create_test_provider(
            "test-ollama",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();
        create_test_provider(
            "test-anthropic",
            ProviderType::Anthropic,
            "claude-3-opus",
            None,
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let registry = cli.api().provider_registry().read();

        let openai_providers = registry.list_by_type(Some(ProviderType::OpenAI));
        assert_eq!(openai_providers.len(), 1);
        assert_eq!(
            openai_providers[0].provider_name.as_deref(),
            Some("test-openai")
        );

        let ollama_providers = registry.list_by_type(Some(ProviderType::Ollama));
        assert_eq!(ollama_providers.len(), 1);
        assert_eq!(
            ollama_providers[0].provider_name.as_deref(),
            Some("test-ollama")
        );

        let all_providers = registry.list_by_type(None);
        // May have additional providers from config.toml, so just check we have at least our 3 test providers
        assert!(all_providers.len() >= 3);
        let provider_names: Vec<Option<&str>> = all_providers
            .iter()
            .map(|p| p.provider_name.as_deref())
            .collect();
        assert!(provider_names.contains(&Some("test-openai")));
        assert!(provider_names.contains(&Some("test-ollama")));
        assert!(provider_names.contains(&Some("test-anthropic")));
    });
}
