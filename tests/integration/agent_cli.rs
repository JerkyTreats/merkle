//! Integration tests for Agent CLI commands

use meld::agent::{AgentRole, AgentStorage, XdgAgentStorage};
use meld::config::{xdg, AgentConfig};
use meld::error::ApiError;
use meld::cli::{AgentCommands, Commands, RunContext};
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
        metadata: Default::default(),
    };

    // Add user prompt templates for writer roles
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

#[test]
fn test_agent_list_empty() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Ensure agents directory exists but is empty
        let agents_dir = xdg::agents_dir().unwrap();
        // Remove any existing agents
        if agents_dir.exists() {
            for entry in fs::read_dir(&agents_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::List {
                format: "text".to_string(),
                role: None,
            },
        };

        let output = cli.execute(&command).unwrap();
        // Output should be valid (either "No agents found" or a list)
        // Note: Agents may be loaded from config.toml, so we just verify the command works
        assert!(
            output.contains("Available Agents")
                || output.contains("No agents found")
                || output.contains("Total:")
        );
    });
}

#[test]
fn test_agent_status_empty() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let agents_dir = xdg::agents_dir().unwrap();
        if agents_dir.exists() {
            for entry in fs::read_dir(&agents_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Agent {
            command: AgentCommands::Status {
                format: "text".to_string(),
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("No agents configured") || output.contains("Agents"));
    });
}

#[test]
fn test_agent_status_one_agent_text() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "status-test.md");
        create_test_agent(
            "status-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Agent {
            command: AgentCommands::Status {
                format: "text".to_string(),
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Agents"));
        assert!(output.contains("status-agent"));
        assert!(output.contains("Writer"));
        assert!(output.contains("Valid") || output.contains("Prompt"));
        assert!(output.contains("Total:") && output.contains("agents"));
    });
}

#[test]
fn test_agent_status_one_agent_json() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "status-json.md");
        create_test_agent(
            "status-json-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Agent {
            command: AgentCommands::Status {
                format: "json".to_string(),
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("\"agents\""));
        assert!(output.contains("\"total\""));
        assert!(output.contains("\"valid_count\""));
        assert!(output.contains("status-json-agent"));
        assert!(output.contains("\"agent_id\""));
        assert!(output.contains("\"role\""));
        assert!(output.contains("\"valid\""));
        assert!(output.contains("\"prompt_path_exists\""));
    });
}

#[test]
fn test_agent_status_multiple_agents() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "multi.md");
        create_test_agent(
            "valid-writer",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_agent("reader-agent", AgentRole::Reader, None).unwrap();
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();
        let result = cli.execute(&Commands::Agent {
            command: AgentCommands::Status {
                format: "text".to_string(),
            },
        });
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("valid-writer"));
        assert!(output.contains("reader-agent"));
        assert!(output.contains("Total: 2 agents"));
    });
}

#[test]
fn test_agent_list_text() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-writer",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_agent("test-reader", AgentRole::Reader, None).unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::List {
                format: "text".to_string(),
                role: None,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-writer"));
        assert!(output.contains("test-reader"));
        assert!(output.contains("Writer"));
        assert!(output.contains("Reader"));
    });
}

#[test]
fn test_agent_list_json() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-writer",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::List {
                format: "json".to_string(),
                role: None,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("\"agent_id\""));
        assert!(output.contains("test-writer"));
        assert!(output.contains("\"total\""));
    });
}

#[test]
fn test_agent_list_filtered_by_role() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-writer",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();
        create_test_agent("test-reader", AgentRole::Reader, None).unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::List {
                format: "text".to_string(),
                role: Some("Writer".to_string()),
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-writer"));
        assert!(!output.contains("test-reader"));
    });
}

#[test]
fn test_agent_show_text() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Show {
                agent_id: "test-agent".to_string(),
                format: "text".to_string(),
                include_prompt: false,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-agent"));
        assert!(output.contains("Writer"));
    });
}

#[test]
fn test_agent_show_with_prompt() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Show {
                agent_id: "test-agent".to_string(),
                format: "text".to_string(),
                include_prompt: true,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-agent"));
        assert!(output.contains("Test Prompt"));
    });
}

#[test]
fn test_agent_show_json() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Show {
                agent_id: "test-agent".to_string(),
                format: "json".to_string(),
                include_prompt: false,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("\"agent_id\""));
        assert!(output.contains("test-agent"));
    });
}

#[test]
fn test_agent_show_not_found() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Show {
                agent_id: "nonexistent".to_string(),
                format: "text".to_string(),
                include_prompt: false,
            },
        };

        let result = cli.execute(&command);
        assert!(result.is_err());
    });
}

#[test]
fn test_agent_validate_valid() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Validate {
                agent_id: Some("test-agent".to_string()),
                all: false,
                verbose: false,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-agent"));
        // Should pass validation
        assert!(output.contains("passed") || output.contains("✓"));
    });
}

#[test]
fn test_agent_validate_missing_prompt() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Create agent with non-existent prompt path
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some("/nonexistent/path.md"),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Validate {
                agent_id: Some("test-agent".to_string()),
                all: false,
                verbose: false,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-agent"));
        // Should have validation errors
        assert!(output.contains("error") || output.contains("✗"));
    });
}

#[test]
fn test_agent_validate_all() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path1 = create_test_prompt_file(&test_dir, "test1.md");
        let prompt_path2 = create_test_prompt_file(&test_dir, "test2.md");
        create_test_agent(
            "test-agent-1",
            AgentRole::Writer,
            Some(prompt_path1.to_str().unwrap()),
        )
        .unwrap();
        create_test_agent("test-agent-2", AgentRole::Reader, None).unwrap();
        create_test_agent(
            "test-agent-3",
            AgentRole::Writer,
            Some(prompt_path2.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Validate {
                agent_id: None,
                all: true,
                verbose: false,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("Validating all agents"));
        assert!(output.contains("test-agent-1"));
        assert!(output.contains("test-agent-2"));
        assert!(output.contains("test-agent-3"));
        assert!(output.contains("Summary:"));
    });
}

#[test]
fn test_agent_validate_all_verbose() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Validate {
                agent_id: None,
                all: true,
                verbose: true,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("Validating all agents"));
        assert!(output.contains("test-agent"));
        assert!(output.contains("checks passed") || output.contains("checks"));
    });
}

#[test]
fn test_agent_validate_all_empty() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        // Ensure agents directory exists but is empty
        let agents_dir = xdg::agents_dir().unwrap();
        // Remove any existing agents
        if agents_dir.exists() {
            for entry in fs::read_dir(&agents_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                    fs::remove_file(entry.path()).unwrap();
                }
            }
        }

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Validate {
                agent_id: None,
                all: true,
                verbose: false,
            },
        };

        let output = cli.execute(&command).unwrap();
        // Output should indicate no agents found or show validation results
        // Note: Agents may be loaded from config.toml, so we just verify the command works
        assert!(
            output.contains("No agents found")
                || output.contains("to validate")
                || output.contains("Validating all agents")
        );
    });
}

#[test]
fn test_agent_create_non_interactive() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "new.md");

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Create {
                agent_id: "new-agent".to_string(),
                role: Some("Writer".to_string()),
                prompt_path: Some(prompt_path.to_str().unwrap().to_string()),
                interactive: false,
                non_interactive: true,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("new-agent"));
        assert!(output.contains("created"));

        // Verify agent exists
        let config_path = XdgAgentStorage::new().path_for("new-agent").unwrap();
        assert!(config_path.exists());
    });
}

#[test]
fn test_agent_create_reader() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Create {
                agent_id: "reader-agent".to_string(),
                role: Some("Reader".to_string()),
                prompt_path: None,
                interactive: false,
                non_interactive: true,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("reader-agent"));

        // Verify agent exists
        let config_path = XdgAgentStorage::new().path_for("reader-agent").unwrap();
        assert!(config_path.exists());
    });
}

#[test]
fn test_agent_edit_prompt_path() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "old.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let new_prompt_path = create_test_prompt_file(&test_dir, "new.md");

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Edit {
                agent_id: "test-agent".to_string(),
                prompt_path: Some(new_prompt_path.to_str().unwrap().to_string()),
                role: None,
                editor: None,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-agent"));
        assert!(output.contains("updated"));

        // Verify config was updated
        let config_path = XdgAgentStorage::new().path_for("test-agent").unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("prompts/test-agent.md"));

        // Prompt files are normalized into XDG prompts using agent id filename
        let copied_prompt = xdg::prompts_dir().unwrap().join("test-agent.md");
        assert!(copied_prompt.exists());
    });
}

#[test]
fn test_agent_edit_role() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Edit {
                agent_id: "test-agent".to_string(),
                prompt_path: None,
                role: Some("Reader".to_string()),
                editor: None,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-agent"));

        // Verify config was updated
        let config_path = XdgAgentStorage::new().path_for("test-agent").unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Reader"));
    });
}

#[test]
fn test_agent_remove() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let prompt_path = create_test_prompt_file(&test_dir, "test.md");
        create_test_agent(
            "test-agent",
            AgentRole::Writer,
            Some(prompt_path.to_str().unwrap()),
        )
        .unwrap();

        let config_path = XdgAgentStorage::new().path_for("test-agent").unwrap();
        assert!(config_path.exists());

        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Remove {
                agent_id: "test-agent".to_string(),
                force: true,
            },
        };

        let output = cli.execute(&command).unwrap();
        assert!(output.contains("test-agent"));
        assert!(output.contains("Removed"));

        // Verify config file was deleted
        assert!(!config_path.exists());
    });
}

#[test]
fn test_agent_remove_not_found() {
    let test_dir = TempDir::new().unwrap();
    with_xdg_env(&test_dir, || {
        let workspace = test_dir.path().to_path_buf();
        let cli = RunContext::new(workspace, None).unwrap();

        let command = Commands::Agent {
            command: AgentCommands::Remove {
                agent_id: "nonexistent".to_string(),
                force: true,
            },
        };

        let result = cli.execute(&command);
        assert!(result.is_err());
    });
}
