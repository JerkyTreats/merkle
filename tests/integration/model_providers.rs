//! Integration tests for Model Provider Integration

use merkle::agent::{AgentIdentity, AgentRole};
use merkle::provider::{
    ChatMessage, CompletionOptions, MessageRole, ModelProvider, ProviderFactory,
};

#[test]
fn test_agent_with_openai_provider() {
    let mut agent = AgentIdentity::new("openai-agent".to_string(), AgentRole::Writer);

    agent.provider = Some(ModelProvider::OpenAI {
        model: "gpt-4".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
    });

    assert!(agent.provider.is_some());
    let client = ProviderFactory::create_client(agent.provider.as_ref().unwrap()).unwrap();
    assert_eq!(client.provider_name(), "openai");
    assert_eq!(client.model_name(), "gpt-4");
}

#[test]
fn test_agent_with_anthropic_provider() {
    let mut agent = AgentIdentity::new("anthropic-agent".to_string(), AgentRole::Writer);

    agent.provider = Some(ModelProvider::Anthropic {
        model: "claude-3-opus".to_string(),
        api_key: "test-key".to_string(),
    });

    assert!(agent.provider.is_some());
    let client = ProviderFactory::create_client(agent.provider.as_ref().unwrap()).unwrap();
    assert_eq!(client.provider_name(), "anthropic");
    assert_eq!(client.model_name(), "claude-3-opus");
}

#[test]
fn test_agent_with_ollama_provider() {
    let mut agent = AgentIdentity::new("ollama-agent".to_string(), AgentRole::Writer);

    agent.provider = Some(ModelProvider::Ollama {
        model: "llama2".to_string(),
        base_url: None,
    });

    assert!(agent.provider.is_some());
    let client = ProviderFactory::create_client(agent.provider.as_ref().unwrap()).unwrap();
    assert_eq!(client.provider_name(), "ollama");
    assert_eq!(client.model_name(), "llama2");
}

#[test]
fn test_agent_with_custom_local_provider() {
    let mut agent = AgentIdentity::new("local-agent".to_string(), AgentRole::Writer);

    agent.provider = Some(ModelProvider::LocalCustom {
        model: "custom-model".to_string(),
        endpoint: "http://localhost:8080/v1".to_string(),
        api_key: None,
    });

    assert!(agent.provider.is_some());
    let client = ProviderFactory::create_client(agent.provider.as_ref().unwrap()).unwrap();
    assert_eq!(client.provider_name(), "local");
    assert_eq!(client.model_name(), "custom-model");
}

#[test]
fn test_agent_without_provider() {
    let agent = AgentIdentity::new("no-provider-agent".to_string(), AgentRole::Reader);
    assert!(agent.provider.is_none());
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
