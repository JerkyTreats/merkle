//! Model Provider Abstraction
//!
//! Unified interface for interacting with multiple LLM providers (OpenAI, Anthropic,
//! local models via Ollama, custom local servers). Provides a consistent API for
//! agent-driven frame generation while maintaining provider-agnostic agent identity.

use crate::error::ApiError;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

/// Model provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelProvider {
    OpenAI {
        model: String,
        api_key: String,
        base_url: Option<String>, // For custom endpoints (e.g., Azure OpenAI)
    },
    Anthropic {
        model: String,
        api_key: String,
    },
    Ollama {
        model: String,
        base_url: Option<String>, // Default: http://localhost:11434
    },
    LocalCustom {
        model: String,
        endpoint: String, // Full endpoint URL (e.g., http://localhost:8080/v1)
        api_key: Option<String>,
    },
}

/// Chat message role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

/// Completion options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionOptions {
    pub temperature: Option<f32>,      // 0.0-2.0, default: 1.0
    pub max_tokens: Option<u32>,       // Maximum tokens to generate
    pub top_p: Option<f32>,           // Nucleus sampling
    pub frequency_penalty: Option<f32>, // -2.0 to 2.0
    pub presence_penalty: Option<f32>,  // -2.0 to 2.0
    pub stop: Option<Vec<String>>,      // Stop sequences
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            temperature: Some(1.0),
            max_tokens: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
        }
    }
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
}

/// Streaming completion type
pub type CompletionStream = Pin<Box<dyn Stream<Item = Result<String, ApiError>> + Send>>;

/// Model provider client trait
#[async_trait]
pub trait ModelProviderClient: Send + Sync {
    /// Generate a completion from a list of messages
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        options: CompletionOptions,
    ) -> Result<CompletionResponse, ApiError>;

    /// Generate a streaming completion
    async fn stream(
        &self,
        messages: Vec<ChatMessage>,
        options: CompletionOptions,
    ) -> Result<CompletionStream, ApiError>;

    /// Get the provider name
    fn provider_name(&self) -> &str;

    /// Get the model name
    fn model_name(&self) -> &str;
}

// OpenAI-compatible API request/response structures
#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// Helper function to convert MessageRole to string
fn role_to_string(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

// Helper function to map HTTP errors to ApiError
fn map_http_error(error: reqwest::Error) -> ApiError {
    if error.is_status() {
        let status = error.status().unwrap();
        match status.as_u16() {
            401 => ApiError::ProviderAuthFailed(format!("Authentication failed: {}", error)),
            429 => ApiError::ProviderRateLimit(format!("Rate limit exceeded: {}", error)),
            404 => ApiError::ProviderModelNotFound(format!("Model not found: {}", error)),
            _ => ApiError::ProviderRequestFailed(format!("Request failed with status {}: {}", status, error)),
        }
    } else if error.is_timeout() {
        ApiError::ProviderRequestFailed(format!("Request timeout: {}", error))
    } else if error.is_connect() {
        ApiError::ProviderRequestFailed(format!("Connection error: {}", error))
    } else {
        ApiError::ProviderError(format!("HTTP error: {}", error))
    }
}

/// OpenAI provider client
pub struct OpenAIClient {
    client: Client,
    model: String,
    api_key: String,
    base_url: String,
}

impl OpenAIClient {
    pub fn new(model: String, api_key: String, base_url: Option<String>) -> Result<Self, ApiError> {
        let client = Client::new();
        let base_url = base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        Ok(Self {
            client,
            model,
            api_key,
            base_url,
        })
    }
}

#[async_trait]
impl ModelProviderClient for OpenAIClient {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        options: CompletionOptions,
    ) -> Result<CompletionResponse, ApiError> {
        let openai_messages: Vec<OpenAIMessage> = messages
            .into_iter()
            .map(|msg| OpenAIMessage {
                role: role_to_string(msg.role).to_string(),
                content: msg.content,
            })
            .collect();

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: openai_messages,
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            top_p: options.top_p,
            frequency_penalty: options.frequency_penalty,
            presence_penalty: options.presence_penalty,
            stop: options.stop,
            stream: false,
        };

        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 => ApiError::ProviderAuthFailed(format!("Authentication failed: {}", error_text)),
                429 => ApiError::ProviderRateLimit(format!("Rate limit exceeded: {}", error_text)),
                404 => ApiError::ProviderModelNotFound(format!("Model not found: {}", error_text)),
                _ => ApiError::ProviderRequestFailed(format!("Request failed: {}", error_text)),
            });
        }

        let completion: ChatCompletionResponse = response.json().await.map_err(|e| {
            ApiError::ProviderError(format!("Failed to parse response: {}", e))
        })?;

        let choice = completion.choices.first().ok_or_else(|| {
            ApiError::ProviderError("No choices in response".to_string())
        })?;

        let usage = completion.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            content: choice.message.content.clone(),
            model: completion.model,
            usage: TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
        })
    }

    async fn stream(
        &self,
        _messages: Vec<ChatMessage>,
        _options: CompletionOptions,
    ) -> Result<CompletionStream, ApiError> {
        // Streaming implementation would go here
        // For now, return an error indicating it's not implemented
        Err(ApiError::ProviderError("Streaming not yet implemented for OpenAI".to_string()))
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Anthropic provider client (using OpenAI-compatible format via Claude API)
pub struct AnthropicClient {
    client: Client,
    model: String,
    api_key: String,
}

impl AnthropicClient {
    pub fn new(model: String, api_key: String) -> Result<Self, ApiError> {
        Ok(Self {
            client: Client::new(),
            model,
            api_key,
        })
    }
}

#[async_trait]
impl ModelProviderClient for AnthropicClient {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        options: CompletionOptions,
    ) -> Result<CompletionResponse, ApiError> {
        // Anthropic API uses a different format, but we'll map it to OpenAI-compatible
        // For now, we'll use a simplified approach that works with OpenAI-compatible endpoints
        // In a real implementation, we'd use the Anthropic SDK or map their API format

        // Anthropic API endpoint
        let url = "https://api.anthropic.com/v1/messages";

        // Convert messages to Anthropic format
        let system_message = messages.iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone());

        let user_messages: Vec<String> = messages.iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.clone())
            .collect();

        let mut request_body = json!({
            "model": self.model,
            "max_tokens": options.max_tokens.unwrap_or(1024),
        });

        if let Some(system) = system_message {
            request_body["system"] = json!(system);
        }

        if !user_messages.is_empty() {
            request_body["messages"] = json!(user_messages.into_iter().map(|content| {
                json!({"role": "user", "content": content})
            }).collect::<Vec<_>>());
        }

        if let Some(temp) = options.temperature {
            request_body["temperature"] = json!(temp);
        }

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 => ApiError::ProviderAuthFailed(format!("Authentication failed: {}", error_text)),
                429 => ApiError::ProviderRateLimit(format!("Rate limit exceeded: {}", error_text)),
                404 => ApiError::ProviderModelNotFound(format!("Model not found: {}", error_text)),
                _ => ApiError::ProviderRequestFailed(format!("Request failed: {}", error_text)),
            });
        }

        #[derive(Deserialize)]
        struct AnthropicResponse {
            content: Vec<AnthropicContent>,
            model: String,
            usage: Option<AnthropicUsage>,
        }

        #[derive(Deserialize)]
        struct AnthropicContent {
            text: String,
        }

        #[derive(Deserialize)]
        struct AnthropicUsage {
            input_tokens: u32,
            output_tokens: u32,
        }

        let completion: AnthropicResponse = response.json().await.map_err(|e| {
            ApiError::ProviderError(format!("Failed to parse response: {}", e))
        })?;

        let content = completion.content.first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        let usage = completion.usage.unwrap_or(AnthropicUsage {
            input_tokens: 0,
            output_tokens: 0,
        });

        Ok(CompletionResponse {
            content,
            model: completion.model,
            usage: TokenUsage {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
                total_tokens: usage.input_tokens + usage.output_tokens,
            },
            finish_reason: Some("stop".to_string()),
        })
    }

    async fn stream(
        &self,
        _messages: Vec<ChatMessage>,
        _options: CompletionOptions,
    ) -> Result<CompletionStream, ApiError> {
        Err(ApiError::ProviderError("Streaming not yet implemented for Anthropic".to_string()))
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Ollama provider client (local models)
pub struct OllamaClient {
    client: Client,
    model: String,
    base_url: String,
}

impl OllamaClient {
    pub fn new(model: String, base_url: Option<String>) -> Result<Self, ApiError> {
        let base_url = base_url.unwrap_or_else(|| "http://localhost:11434".to_string());

        Ok(Self {
            client: Client::new(),
            model,
            base_url,
        })
    }
}

#[async_trait]
impl ModelProviderClient for OllamaClient {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        options: CompletionOptions,
    ) -> Result<CompletionResponse, ApiError> {
        // Ollama uses OpenAI-compatible API format
        let openai_messages: Vec<OpenAIMessage> = messages
            .into_iter()
            .map(|msg| OpenAIMessage {
                role: role_to_string(msg.role).to_string(),
                content: msg.content,
            })
            .collect();

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: openai_messages,
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            top_p: options.top_p,
            frequency_penalty: options.frequency_penalty,
            presence_penalty: options.presence_penalty,
            stop: options.stop,
            stream: false,
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::ProviderRequestFailed(format!(
                "Request failed with status {}: {}",
                status, error_text
            )));
        }

        let completion: ChatCompletionResponse = response.json().await.map_err(|e| {
            ApiError::ProviderError(format!("Failed to parse response: {}", e))
        })?;

        let choice = completion.choices.first().ok_or_else(|| {
            ApiError::ProviderError("No choices in response".to_string())
        })?;

        let usage = completion.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            content: choice.message.content.clone(),
            model: completion.model,
            usage: TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
        })
    }

    async fn stream(
        &self,
        _messages: Vec<ChatMessage>,
        _options: CompletionOptions,
    ) -> Result<CompletionStream, ApiError> {
        Err(ApiError::ProviderError("Streaming not yet implemented for Ollama".to_string()))
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Custom local provider client (OpenAI-compatible API)
pub struct CustomLocalClient {
    client: Client,
    model: String,
    endpoint: String,
    api_key: Option<String>,
}

impl CustomLocalClient {
    pub fn new(model: String, endpoint: String, api_key: Option<String>) -> Result<Self, ApiError> {
        Ok(Self {
            client: Client::new(),
            model,
            endpoint,
            api_key,
        })
    }
}

#[async_trait]
impl ModelProviderClient for CustomLocalClient {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        options: CompletionOptions,
    ) -> Result<CompletionResponse, ApiError> {
        let openai_messages: Vec<OpenAIMessage> = messages
            .into_iter()
            .map(|msg| OpenAIMessage {
                role: role_to_string(msg.role).to_string(),
                content: msg.content,
            })
            .collect();

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: openai_messages,
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            top_p: options.top_p,
            frequency_penalty: options.frequency_penalty,
            presence_penalty: options.presence_penalty,
            stop: options.stop,
            stream: false,
        };

        let url = format!("{}/chat/completions", self.endpoint);
        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &self.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request_builder
            .json(&request)
            .send()
            .await
            .map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::ProviderRequestFailed(format!(
                "Request failed with status {}: {}",
                status, error_text
            )));
        }

        let completion: ChatCompletionResponse = response.json().await.map_err(|e| {
            ApiError::ProviderError(format!("Failed to parse response: {}", e))
        })?;

        let choice = completion.choices.first().ok_or_else(|| {
            ApiError::ProviderError("No choices in response".to_string())
        })?;

        let usage = completion.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            content: choice.message.content.clone(),
            model: completion.model,
            usage: TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
        })
    }

    async fn stream(
        &self,
        _messages: Vec<ChatMessage>,
        _options: CompletionOptions,
    ) -> Result<CompletionStream, ApiError> {
        Err(ApiError::ProviderError("Streaming not yet implemented for CustomLocal".to_string()))
    }

    fn provider_name(&self) -> &str {
        "local"
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Provider factory for creating provider clients
pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create_client(
        provider: &ModelProvider,
    ) -> Result<Box<dyn ModelProviderClient>, ApiError> {
        match provider {
            ModelProvider::OpenAI { model, api_key, base_url } => {
                Ok(Box::new(OpenAIClient::new(
                    model.clone(),
                    api_key.clone(),
                    base_url.clone(),
                )?))
            }
            ModelProvider::Anthropic { model, api_key } => {
                Ok(Box::new(AnthropicClient::new(
                    model.clone(),
                    api_key.clone(),
                )?))
            }
            ModelProvider::Ollama { model, base_url } => {
                Ok(Box::new(OllamaClient::new(
                    model.clone(),
                    base_url.clone(),
                )?))
            }
            ModelProvider::LocalCustom { model, endpoint, api_key } => {
                Ok(Box::new(CustomLocalClient::new(
                    model.clone(),
                    endpoint.clone(),
                    api_key.clone(),
                )?))
            }
        }
    }
}

// Mock provider for testing
#[cfg(test)]
pub struct MockProvider {
    responses: Vec<String>,
    current: std::sync::Arc<std::sync::Mutex<usize>>,
    provider_name: String,
    model_name: String,
}

#[cfg(test)]
impl MockProvider {
    pub fn new(provider_name: String, model_name: String, responses: Vec<String>) -> Self {
        Self {
            responses,
            current: std::sync::Arc::new(std::sync::Mutex::new(0)),
            provider_name,
            model_name,
        }
    }
}

#[cfg(test)]
#[async_trait]
impl ModelProviderClient for MockProvider {
    async fn complete(
        &self,
        _messages: Vec<ChatMessage>,
        _options: CompletionOptions,
    ) -> Result<CompletionResponse, ApiError> {
        let mut idx = self.current.lock().unwrap();
        let response = if *idx < self.responses.len() {
            self.responses[*idx].clone()
        } else {
            "Mock response".to_string()
        };
        *idx += 1;

        Ok(CompletionResponse {
            content: response,
            model: self.model_name.clone(),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            },
            finish_reason: Some("stop".to_string()),
        })
    }

    async fn stream(
        &self,
        _messages: Vec<ChatMessage>,
        _options: CompletionOptions,
    ) -> Result<CompletionStream, ApiError> {
        Err(ApiError::ProviderError("Streaming not implemented for mock".to_string()))
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_provider_serialization() {
        let provider = ModelProvider::OpenAI {
            model: "gpt-4".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
        };

        let serialized = serde_json::to_string(&provider).unwrap();
        let deserialized: ModelProvider = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            ModelProvider::OpenAI { model, .. } => {
                assert_eq!(model, "gpt-4");
            }
            _ => panic!("Wrong provider type"),
        }
    }

    #[test]
    fn test_completion_options_default() {
        let options = CompletionOptions::default();
        assert_eq!(options.temperature, Some(1.0));
        assert_eq!(options.max_tokens, None);
    }

    #[tokio::test]
    async fn test_mock_provider() {
        let mock = MockProvider::new(
            "mock".to_string(),
            "mock-model".to_string(),
            vec!["Response 1".to_string(), "Response 2".to_string()],
        );

        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "Test".to_string(),
        }];

        let response1 = mock.complete(messages.clone(), CompletionOptions::default()).await.unwrap();
        assert_eq!(response1.content, "Response 1");
        assert_eq!(response1.model, "mock-model");

        let response2 = mock.complete(messages, CompletionOptions::default()).await.unwrap();
        assert_eq!(response2.content, "Response 2");
    }

    #[test]
    fn test_provider_factory_openai() {
        let provider = ModelProvider::OpenAI {
            model: "gpt-4".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
        };

        let client = ProviderFactory::create_client(&provider).unwrap();
        assert_eq!(client.provider_name(), "openai");
        assert_eq!(client.model_name(), "gpt-4");
    }

    #[test]
    fn test_provider_factory_anthropic() {
        let provider = ModelProvider::Anthropic {
            model: "claude-3-opus".to_string(),
            api_key: "test-key".to_string(),
        };

        let client = ProviderFactory::create_client(&provider).unwrap();
        assert_eq!(client.provider_name(), "anthropic");
        assert_eq!(client.model_name(), "claude-3-opus");
    }

    #[test]
    fn test_provider_factory_ollama() {
        let provider = ModelProvider::Ollama {
            model: "llama2".to_string(),
            base_url: None,
        };

        let client = ProviderFactory::create_client(&provider).unwrap();
        assert_eq!(client.provider_name(), "ollama");
        assert_eq!(client.model_name(), "llama2");
    }

    #[test]
    fn test_provider_factory_custom_local() {
        let provider = ModelProvider::LocalCustom {
            model: "custom-model".to_string(),
            endpoint: "http://localhost:8080/v1".to_string(),
            api_key: None,
        };

        let client = ProviderFactory::create_client(&provider).unwrap();
        assert_eq!(client.provider_name(), "local");
        assert_eq!(client.model_name(), "custom-model");
    }

    #[test]
    fn test_message_role_serialization() {
        let role = MessageRole::System;
        let serialized = serde_json::to_string(&role).unwrap();
        let deserialized: MessageRole = serde_json::from_str(&serialized).unwrap();
        assert_eq!(role, deserialized);
    }
}
