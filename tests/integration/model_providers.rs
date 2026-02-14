//! Integration tests for Model Provider Integration

use merkle::agent::{AgentIdentity, AgentRole};
use merkle::config::{MerkleConfig, ProviderConfig, ProviderType};
use merkle::provider::{
    ChatMessage, CompletionOptions, MessageRole, ModelProvider, ProviderRegistry,
};

#[test]
fn test_provider_registry_with_openai() {
    let mut registry = ProviderRegistry::new();
    let mut config = MerkleConfig::default();

    config.providers.insert(
        "test-openai".to_string(),
        ProviderConfig {
            provider_name: Some("test-openai".to_string()),
            provider_type: ProviderType::OpenAI,
            model: "gpt-4".to_string(),
            api_key: Some("test-key".to_string()),
            endpoint: None,
            default_options: CompletionOptions::default(),
        },
    );

    registry.load_from_config(&config).unwrap();
    let client = registry.create_client("test-openai").unwrap();
    assert_eq!(client.provider_name(), "openai");
    assert_eq!(client.model_name(), "gpt-4");
}

#[test]
fn test_provider_registry_with_anthropic() {
    let mut registry = ProviderRegistry::new();
    let mut config = MerkleConfig::default();

    config.providers.insert(
        "test-anthropic".to_string(),
        ProviderConfig {
            provider_name: Some("test-anthropic".to_string()),
            provider_type: ProviderType::Anthropic,
            model: "claude-3-opus".to_string(),
            api_key: Some("test-key".to_string()),
            endpoint: None,
            default_options: CompletionOptions::default(),
        },
    );

    registry.load_from_config(&config).unwrap();
    let client = registry.create_client("test-anthropic").unwrap();
    assert_eq!(client.provider_name(), "anthropic");
    assert_eq!(client.model_name(), "claude-3-opus");
}

#[test]
fn test_provider_registry_with_ollama() {
    let mut registry = ProviderRegistry::new();
    let mut config = MerkleConfig::default();

    config.providers.insert(
        "test-ollama".to_string(),
        ProviderConfig {
            provider_name: Some("test-ollama".to_string()),
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            api_key: None,
            endpoint: None,
            default_options: CompletionOptions::default(),
        },
    );

    registry.load_from_config(&config).unwrap();
    let client = registry.create_client("test-ollama").unwrap();
    assert_eq!(client.provider_name(), "ollama");
    assert_eq!(client.model_name(), "llama2");
}

#[test]
fn test_provider_registry_with_custom_local() {
    let mut registry = ProviderRegistry::new();
    let mut config = MerkleConfig::default();

    config.providers.insert(
        "test-local".to_string(),
        ProviderConfig {
            provider_name: Some("test-local".to_string()),
            provider_type: ProviderType::LocalCustom,
            model: "custom-model".to_string(),
            api_key: None,
            endpoint: Some("http://localhost:8080/v1".to_string()),
            default_options: CompletionOptions::default(),
        },
    );

    registry.load_from_config(&config).unwrap();
    let client = registry.create_client("test-local").unwrap();
    assert_eq!(client.provider_name(), "local");
    assert_eq!(client.model_name(), "custom-model");
}

#[test]
fn test_agent_without_provider() {
    // Agents are now provider-agnostic
    let _agent = AgentIdentity::new("no-provider-agent".to_string(), AgentRole::Reader);
    // Agent no longer has provider field - this is expected
}

#[test]
fn test_provider_serialization() {
    let provider = ModelProvider::Ollama {
        model: "llama2".to_string(),
        base_url: Some("http://localhost:11434".to_string()),
    };

    let serialized = serde_json::to_string(&provider).unwrap();
    let deserialized: ModelProvider = serde_json::from_str(&serialized).unwrap();

    match deserialized {
        ModelProvider::Ollama { model, base_url } => {
            assert_eq!(model, "llama2");
            assert_eq!(base_url, Some("http://localhost:11434".to_string()));
        }
        _ => panic!("Wrong provider type"),
    }
}

#[test]
fn test_chat_message_creation() {
    let system_msg = ChatMessage {
        role: MessageRole::System,
        content: "You are a helpful assistant.".to_string(),
    };

    let user_msg = ChatMessage {
        role: MessageRole::User,
        content: "Hello!".to_string(),
    };

    assert_eq!(system_msg.role, MessageRole::System);
    assert_eq!(user_msg.role, MessageRole::User);
}

#[test]
fn test_completion_options() {
    let options = CompletionOptions {
        temperature: Some(0.7),
        max_tokens: Some(1000),
        top_p: Some(0.9),
        frequency_penalty: None,
        presence_penalty: None,
        stop: Some(vec!["\n".to_string()]),
    };

    assert_eq!(options.temperature, Some(0.7));
    assert_eq!(options.max_tokens, Some(1000));
    assert_eq!(options.top_p, Some(0.9));
    assert_eq!(options.stop, Some(vec!["\n".to_string()]));
}
