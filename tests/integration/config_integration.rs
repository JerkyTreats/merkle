//! Integration tests for Configuration System

use meld::agent::{AgentRegistry, AgentRole};
use meld::config::{AgentConfig, ConfigLoader, MerkleConfig, ProviderConfig, ProviderType};
use meld::provider::CompletionOptions;
use tempfile::TempDir;

#[test]
fn test_config_loads_agents_into_registry() {
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
system_prompt = "You are a test agent."
"#,
    )
    .unwrap();

    let config = ConfigLoader::load_from_file(&config_file).unwrap();
    assert!(config.validate().is_ok());

    let mut registry = AgentRegistry::new();
    registry.load_from_config(&config).unwrap();

    let agent = registry.get("test-agent").unwrap();
    assert_eq!(agent.agent_id, "test-agent");
    assert_eq!(agent.role, AgentRole::Writer);
    assert_eq!(
        agent.metadata.get("system_prompt"),
        Some(&"You are a test agent.".to_string())
    );
    // Agent no longer has provider field - providers are managed separately
}

#[test]
fn test_config_agent_without_provider() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("test_config.toml");

    std::fs::write(
        &config_file,
        r#"
[agents.reader-agent]
agent_id = "reader-agent"
role = "Reader"
system_prompt = "You are a reader agent."
"#,
    )
    .unwrap();

    let config = ConfigLoader::load_from_file(&config_file).unwrap();
    assert!(config.validate().is_ok());

    let mut registry = AgentRegistry::new();
    registry.load_from_config(&config).unwrap();

    let agent = registry.get("reader-agent").unwrap();
    assert_eq!(agent.agent_id, "reader-agent");
    assert_eq!(agent.role, AgentRole::Reader);
    // Agent no longer has provider field - providers are managed separately
    assert_eq!(
        agent.metadata.get("system_prompt"),
        Some(&"You are a reader agent.".to_string())
    );
}

#[test]
fn test_config_provider_conversion() {
    let mut config = MerkleConfig::default();

    // Add OpenAI provider
    config.providers.insert(
        "openai-test".to_string(),
        ProviderConfig {
            provider_name: Some("openai-test".to_string()),
            provider_type: ProviderType::OpenAI,
            model: "gpt-4".to_string(),
            api_key: Some("test-key-123".to_string()),
            endpoint: None,
            default_options: CompletionOptions::default(),
        },
    );

    // Add Ollama provider
    config.providers.insert(
        "ollama-test".to_string(),
        ProviderConfig {
            provider_name: Some("ollama-test".to_string()),
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            api_key: None,
            endpoint: Some("http://localhost:11434".to_string()),
            default_options: CompletionOptions::default(),
        },
    );

    // Test OpenAI conversion
    let openai_provider = config.providers.get("openai-test").unwrap();
    let model_provider = openai_provider.to_model_provider().unwrap();
    match model_provider {
        meld::provider::ModelProvider::OpenAI { model, api_key, .. } => {
            assert_eq!(model, "gpt-4");
            assert_eq!(api_key, "test-key-123");
        }
        _ => panic!("Expected OpenAI provider"),
    }

    // Test Ollama conversion
    let ollama_provider = config.providers.get("ollama-test").unwrap();
    let model_provider = ollama_provider.to_model_provider().unwrap();
    match model_provider {
        meld::provider::ModelProvider::Ollama {
            model, base_url, ..
        } => {
            assert_eq!(model, "llama2");
            assert_eq!(base_url, Some("http://localhost:11434".to_string()));
        }
        _ => panic!("Expected Ollama provider"),
    }
}

#[test]
fn test_config_validation_errors() {
    let mut config = MerkleConfig::default();

    // Add agent with invalid agent_id (empty string) to trigger validation error
    config.agents.insert(
        "bad-agent".to_string(),
        AgentConfig {
            agent_id: "".to_string(), // Empty agent_id should fail validation
            role: AgentRole::Writer,
            system_prompt: None,
            system_prompt_path: None,
            metadata: Default::default(),
        },
    );

    let validation_result = config.validate();
    assert!(validation_result.is_err());
    let errors = validation_result.unwrap_err();
    assert!(errors.len() > 0);
    assert!(errors
        .iter()
        .any(|e| { matches!(e, meld::config::ValidationError::Agent(_, _)) }));
}

#[test]
fn test_config_default_values() {
    let config = MerkleConfig::default();
    assert_eq!(
        config.system.default_workspace_root,
        std::path::PathBuf::from(".")
    );
    assert_eq!(
        config.system.storage.store_path,
        std::path::PathBuf::from(".meld/store")
    );
    assert_eq!(
        config.system.storage.frames_path,
        std::path::PathBuf::from(".meld/frames")
    );
}

#[test]
fn test_config_agent_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("test_config.toml");

    std::fs::write(
        &config_file,
        r#"
[agents.test-agent]
agent_id = "test-agent"
role = "Writer"
system_prompt = "Test prompt"
[agents.test-agent.metadata]
custom_key = "custom_value"
another_key = "another_value"
"#,
    )
    .unwrap();

    let config = ConfigLoader::load_from_file(&config_file).unwrap();
    let mut registry = AgentRegistry::new();
    registry.load_from_config(&config).unwrap();

    let agent = registry.get("test-agent").unwrap();
    assert_eq!(
        agent.metadata.get("system_prompt"),
        Some(&"Test prompt".to_string())
    );
    assert_eq!(
        agent.metadata.get("custom_key"),
        Some(&"custom_value".to_string())
    );
    assert_eq!(
        agent.metadata.get("another_key"),
        Some(&"another_value".to_string())
    );
}

#[test]
fn test_config_agent_metadata_isolation_from_frame_policy_keys() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("test_config.toml");

    std::fs::write(
        &config_file,
        r#"
[agents.test-agent]
agent_id = "test-agent"
role = "Writer"
system_prompt = "Test prompt"
[agents.test-agent.metadata]
frame_policy_external_key = "agent_domain_value"
"#,
    )
    .unwrap();

    let config = ConfigLoader::load_from_file(&config_file).unwrap();
    let mut registry = AgentRegistry::new();
    registry.load_from_config(&config).unwrap();

    let agent = registry.get("test-agent").unwrap();
    assert_eq!(
        agent.metadata.get("frame_policy_external_key"),
        Some(&"agent_domain_value".to_string())
    );
}

#[test]
fn test_config_completion_options() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("test_config.toml");

    std::fs::write(
        &config_file,
        r#"
[providers.test-provider]
provider_type = "ollama"
model = "llama2"
[providers.test-provider.default_options]
temperature = 0.7
max_tokens = 2000
top_p = 0.9
"#,
    )
    .unwrap();

    let config = ConfigLoader::load_from_file(&config_file).unwrap();
    let provider = config.providers.get("test-provider").unwrap();

    assert_eq!(provider.default_options.temperature, Some(0.7));
    assert_eq!(provider.default_options.max_tokens, Some(2000));
    assert_eq!(provider.default_options.top_p, Some(0.9));
}
