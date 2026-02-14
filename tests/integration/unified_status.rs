//! Integration tests for unified status command (merkle status)

use merkle::agent::AgentRole;
use merkle::config::{xdg, AgentConfig, ProviderConfig, ProviderType};
use merkle::error::ApiError;
use merkle::tooling::cli::{CliContext, Commands};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::integration::with_xdg_env;

/// Create a test agent config file
fn create_test_agent(
    agent_id: &str,
    role: AgentRole,
    prompt_path: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let agents_dir = xdg::agents_dir()?;
    let config_path = agents_dir.join(format!("{}.toml", agent_id));

    let mut agent_config = AgentConfig {
        agent_id: agent_id.to_string(),
        role,
        system_prompt: None,
        system_prompt_path: prompt_path.map(|s| s.to_string()),
        metadata: std::collections::HashMap::new(),
    };

    if role != AgentRole::Reader {
        agent_config.metadata.insert(
            "user_prompt_file".to_string(),
            "Analyze the file at {path}".to_string(),
        );
        agent_config.metadata.insert(
            "user_prompt_directory".to_string(),
            "Analyze the directory at {path}".to_string(),
        );
    }

    let toml_content = toml::to_string_pretty(&agent_config)
        .map_err(|e| ApiError::ConfigError(format!("Failed to serialize: {}", e)))?;

    fs::write(&config_path, toml_content)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write: {}", e)))?;

    Ok(config_path)
}

/// Create a test prompt file
fn create_test_prompt_file(test_dir: &TempDir, filename: &str) -> PathBuf {
    let prompt_dir = test_dir.path().join("prompts");
    fs::create_dir_all(&prompt_dir).unwrap();
    let prompt_path = prompt_dir.join(filename);
    fs::write(&prompt_path, "# Test Prompt\n\nThis is a test prompt file.").unwrap();
    prompt_path
}

/// Create a test provider config file
fn create_test_provider(
    provider_name: &str,
    provider_type: ProviderType,
    model: &str,
    endpoint: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let providers_dir = xdg::providers_dir()?;
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

/// Clear all agent and provider configs
fn clear_configs() {
    if let Ok(agents_dir) = xdg::agents_dir() {
        if agents_dir.exists() {
            for entry in fs::read_dir(&agents_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }
    }
    if let Ok(providers_dir) = xdg::providers_dir() {
        if providers_dir.exists() {
            for entry in fs::read_dir(&providers_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }
    }
}

#[test]
fn test_unified_status_all_sections_text() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-writer",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_provider(
            "test-ollama",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "text".to_string(),
            workspace_only: false,
            agents_only: false,
            providers_only: false,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // Should contain all three sections
        assert!(output.contains("Workspace Status"));
        assert!(output.contains("Agents"));
        assert!(output.contains("Providers"));
        assert!(output.contains("test-writer"));
        assert!(output.contains("test-ollama"));
    });
}

#[test]
fn test_unified_status_all_sections_json() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "json-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_provider(
            "json-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "json".to_string(),
            workspace_only: false,
            agents_only: false,
            providers_only: false,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // Should be valid JSON with all three sections
        assert!(output.contains("\"workspace\""));
        assert!(output.contains("\"agents\""));
        assert!(output.contains("\"providers\""));
        assert!(output.contains("json-agent"));
        assert!(output.contains("json-provider"));
    });
}

#[test]
fn test_unified_status_workspace_only() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "excluded-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_provider(
            "excluded-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "text".to_string(),
            workspace_only: true,
            agents_only: false,
            providers_only: false,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // Should contain only workspace section
        assert!(output.contains("Workspace Status"));
        // Should NOT contain agents or providers sections (text formatting adds the headings)
        assert!(!output.contains("excluded-agent"));
        assert!(!output.contains("excluded-provider"));
    });
}

#[test]
fn test_unified_status_agents_only() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "only-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_provider(
            "excluded-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "text".to_string(),
            workspace_only: false,
            agents_only: true,
            providers_only: false,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // Should contain only agents section
        assert!(output.contains("Agents"));
        assert!(output.contains("only-agent"));
        // Should NOT contain workspace or providers
        assert!(!output.contains("Workspace Status"));
        assert!(!output.contains("excluded-provider"));
    });
}

#[test]
fn test_unified_status_providers_only() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "excluded-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_provider(
            "only-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "text".to_string(),
            workspace_only: false,
            agents_only: false,
            providers_only: true,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // Should contain only providers section
        assert!(output.contains("Providers"));
        assert!(output.contains("only-provider"));
        // Should NOT contain workspace or agents
        assert!(!output.contains("Workspace Status"));
        assert!(!output.contains("excluded-agent"));
    });
}

#[test]
fn test_unified_status_with_breakdown() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "text".to_string(),
            workspace_only: true,
            agents_only: false,
            providers_only: false,
            breakdown: true,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Workspace Status"));
        // Breakdown may or may not appear depending on scan state
    });
}

#[test]
fn test_unified_status_with_test_connectivity() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        create_test_provider(
            "conn-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://127.0.0.1:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "text".to_string(),
            workspace_only: false,
            agents_only: false,
            providers_only: true,
            breakdown: false,
            test_connectivity: true,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Providers"));
        assert!(output.contains("conn-provider"));
        // Connectivity column should be present
        assert!(
            output.contains("Connectivity") || output.contains("OK") || output.contains("Fail")
        );
    });
}

#[test]
fn test_unified_status_empty_configs() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "text".to_string(),
            workspace_only: false,
            agents_only: false,
            providers_only: false,
            breakdown: false,
            test_connectivity: false,
        });

        // Should succeed even with empty configs
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Workspace Status"));
        // Empty agents/providers should show appropriate message
        assert!(output.contains("No agents configured") || output.contains("Agents"));
        assert!(output.contains("No providers configured") || output.contains("Providers"));
    });
}

#[test]
fn test_unified_status_json_workspace_only() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "json".to_string(),
            workspace_only: true,
            agents_only: false,
            providers_only: false,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // JSON should have workspace, but not agents/providers
        assert!(output.contains("\"workspace\""));
        // Optional fields should be omitted in JSON when not present
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("workspace").is_some());
        assert!(parsed.get("agents").is_none());
        assert!(parsed.get("providers").is_none());
    });
}

#[test]
fn test_unified_status_json_agents_only() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "json-only-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "json".to_string(),
            workspace_only: false,
            agents_only: true,
            providers_only: false,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("workspace").is_none());
        assert!(parsed.get("agents").is_some());
        assert!(parsed.get("providers").is_none());
        assert!(output.contains("json-only-agent"));
    });
}

#[test]
fn test_unified_status_json_providers_only() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        clear_configs();
        create_test_provider(
            "json-only-provider",
            ProviderType::Ollama,
            "llama2",
            Some("http://localhost:11434"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = CliContext::new(workspace, None).unwrap();

        let result = cli.execute(&Commands::Status {
            format: "json".to_string(),
            workspace_only: false,
            agents_only: false,
            providers_only: true,
            breakdown: false,
            test_connectivity: false,
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("workspace").is_none());
        assert!(parsed.get("agents").is_none());
        assert!(parsed.get("providers").is_some());
        assert!(output.contains("json-only-provider"));
    });
}
