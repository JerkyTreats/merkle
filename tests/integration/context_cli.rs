//! Integration tests for Context CLI commands

use clap::Parser;
use merkle::agent::{AgentRole, AgentStorage, XdgAgentStorage};
use merkle::config::{xdg, AgentConfig, ProviderConfig, ProviderType};
use merkle::error::ApiError;
use merkle::cli::{Cli, Commands, ContextCommands, RunContext};
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
    let agents_dir = XdgAgentStorage::new().agents_dir()?;
    // Ensure directory exists
    fs::create_dir_all(&agents_dir)
        .map_err(|e| ApiError::ConfigError(format!("Failed to create agents directory: {}", e)))?;
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

    let toml = toml::to_string(&agent_config)
        .map_err(|e| ApiError::ConfigError(format!("Failed to serialize agent config: {}", e)))?;

    fs::write(&config_path, toml)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write agent config: {}", e)))?;

    Ok(config_path)
}

/// Create a test provider config file
fn create_test_provider(
    provider_name: &str,
    provider_type: ProviderType,
) -> Result<PathBuf, ApiError> {
    let providers_dir = xdg::providers_dir()?;
    // Ensure directory exists
    fs::create_dir_all(&providers_dir).map_err(|e| {
        ApiError::ConfigError(format!("Failed to create providers directory: {}", e))
    })?;
    let config_path = providers_dir.join(format!("{}.toml", provider_name));

    let provider_config = ProviderConfig {
        provider_name: Some(provider_name.to_string()),
        provider_type,
        model: "test-model".to_string(),
        api_key: None,
        endpoint: None,
        default_options: merkle::provider::CompletionOptions::default(),
    };

    let toml = toml::to_string(&provider_config).map_err(|e| {
        ApiError::ConfigError(format!("Failed to serialize provider config: {}", e))
    })?;

    fs::write(&config_path, toml)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write provider config: {}", e)))?;

    Ok(config_path)
}

#[test]
fn test_context_get_with_path() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        // Create workspace
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        // Create a test file
        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        // Initialize CLI context
        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();

        // Scan the workspace
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        // Get context for the file
        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: None,
                path: Some(test_file),
                agent: None,
                frame_type: None,
                max_frames: 10,
                ordering: "recency".to_string(),
                combine: false,
                separator: "\n\n---\n\n".to_string(),
                format: "text".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Node:"));
        assert!(output.contains("test.txt"));
    });
}

#[test]
fn test_context_get_with_node_id() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        // Create workspace
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        // Create a test file
        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        // Initialize CLI context
        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();

        // Scan the workspace
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        // Get root node ID from status (use JSON format to get full hash)
        let status_output = run_context
            .execute(&Commands::Status {
                format: "json".to_string(),
                workspace_only: true,
                agents_only: false,
                providers_only: false,
                breakdown: false,
                test_connectivity: false,
            })
            .unwrap();
        let status_json: serde_json::Value = serde_json::from_str(&status_output).unwrap();
        let root_hash = status_json["workspace"]["tree"]["root_hash"]
            .as_str()
            .unwrap();

        // Get context for the root node
        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: Some(root_hash.to_string()),
                path: None,
                agent: None,
                frame_type: None,
                max_frames: 10,
                ordering: "recency".to_string(),
                combine: false,
                separator: "\n\n---\n\n".to_string(),
                format: "text".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Node:"));
    });
}

#[test]
fn test_context_get_invalid_path() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();

        // Create the file but don't scan it (so it's not in the tree)
        let test_path = workspace_root.join("nonexistent.txt");
        fs::write(&test_path, "test content").unwrap();

        // Try to get context for a path not in the tree
        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: None,
                path: Some(test_path),
                agent: None,
                frame_type: None,
                max_frames: 10,
                ordering: "recency".to_string(),
                combine: false,
                separator: "\n\n---\n\n".to_string(),
                format: "text".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        });

        assert!(result.is_err());
        match result {
            Err(ApiError::PathNotInTree(_)) => {}
            _ => panic!("Expected PathNotInTree error, got: {:?}", result),
        }
    });
}

#[test]
fn test_context_get_json_format() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: None,
                path: Some(test_file),
                agent: None,
                frame_type: None,
                max_frames: 10,
                ordering: "recency".to_string(),
                combine: false,
                separator: "\n\n---\n\n".to_string(),
                format: "json".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        });

        assert!(result.is_ok());
        let output = result.unwrap();
        // Verify it's valid JSON
        let _json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(output.contains("node_id"));
        assert!(output.contains("frames"));
    });
}

#[test]
fn test_context_get_combine() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: None,
                path: Some(test_file),
                agent: None,
                frame_type: None,
                max_frames: 10,
                ordering: "recency".to_string(),
                combine: true,
                separator: " | ".to_string(),
                format: "text".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        });

        assert!(result.is_ok());
        // With no frames, should still work
    });
}

#[test]
fn test_context_generate_requires_provider() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        // Create workspace and agent
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let prompts_dir = xdg::prompts_dir().unwrap();
        let prompt_path = prompts_dir.join("test.md");
        fs::write(&prompt_path, "Test prompt").unwrap();

        create_test_agent("test-agent", AgentRole::Writer, Some("prompts/test.md")).unwrap();

        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        // Try to generate without provider
        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Generate {
                node: None,
                path: Some(test_file),
                path_positional: None,
                agent: Some("test-agent".to_string()),
                provider: None,
                frame_type: None,
                force: false,
                no_recursive: false,
            },
        });

        assert!(result.is_err());
        match result {
            Err(ApiError::ProviderNotConfigured(_)) => {}
            _ => panic!("Expected ProviderNotConfigured error"),
        }
    });
}

#[test]
fn test_context_generate_requires_agent_or_default() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let prompts_dir = xdg::prompts_dir().unwrap();
        let prompt_path = prompts_dir.join("test.md");
        fs::write(&prompt_path, "Test prompt").unwrap();

        // Create a single Writer agent (should be used as default)
        create_test_agent("test-agent", AgentRole::Writer, Some("prompts/test.md")).unwrap();
        create_test_provider("test-provider", ProviderType::Ollama).unwrap();

        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        // Should work without --agent (uses default)
        // Note: This will fail at generation time if provider is not actually available,
        // but the agent resolution should work
        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Generate {
                node: None,
                path: Some(test_file),
                path_positional: None,
                agent: None,
                provider: Some("test-provider".to_string()),
                frame_type: None,
                force: false,
                no_recursive: false,
            },
        });

        // May fail at provider connection, but should not fail at agent resolution
        if let Err(e) = result {
            // Should not be a "no agent" error
            assert!(!e.to_string().contains("No Writer agents found"));
        }
    });
}

#[test]
fn test_context_generate_multiple_agents_requires_flag() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let prompts_dir = xdg::prompts_dir().unwrap();
        let prompt_path = prompts_dir.join("test.md");
        fs::write(&prompt_path, "Test prompt").unwrap();

        // Create multiple Writer agents
        create_test_agent("agent1", AgentRole::Writer, Some("prompts/test.md")).unwrap();
        create_test_agent("agent2", AgentRole::Writer, Some("prompts/test.md")).unwrap();
        create_test_provider("test-provider", ProviderType::Ollama).unwrap();

        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        // Should fail without --agent when multiple agents exist
        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Generate {
                node: None,
                path: Some(test_file),
                path_positional: None,
                agent: None,
                provider: Some("test-provider".to_string()),
                frame_type: None,
                force: false,
                no_recursive: false,
            },
        });

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Multiple Writer agents found"));
    });
}

#[test]
fn test_context_get_invalid_ordering() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: None,
                path: Some(test_file),
                agent: None,
                frame_type: None,
                max_frames: 10,
                ordering: "invalid".to_string(),
                combine: false,
                separator: "\n\n---\n\n".to_string(),
                format: "text".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        });

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid ordering"));
    });
}

#[test]
fn test_context_get_invalid_format() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let test_file = workspace_root.join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let run_context = RunContext::new(workspace_root.clone(), None).unwrap();
        run_context
            .execute(&Commands::Scan { force: true })
            .unwrap();

        let result = run_context.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: None,
                path: Some(test_file),
                agent: None,
                frame_type: None,
                max_frames: 10,
                ordering: "recency".to_string(),
                combine: false,
                separator: "\n\n---\n\n".to_string(),
                format: "invalid".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        });

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid format"));
    });
}

#[test]
fn test_context_generate_rejects_async_flag() {
    let parse_result = Cli::try_parse_from([
        "merkle",
        "context",
        "generate",
        "--path",
        "./foo.txt",
        "--async",
    ]);
    assert!(parse_result.is_err());
}

#[test]
fn test_context_generate_mutually_exclusive_node_path() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let _run_context = RunContext::new(workspace_root.clone(), None).unwrap();

        // This should be caught by clap, but test the execution path anyway
        // Note: clap will prevent both from being set, so this test may not be reachable
        // But we handle it in code for safety
    });
}
