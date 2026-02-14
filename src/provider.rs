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
use toml;

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
    pub temperature: Option<f32>,       // 0.0-2.0, default: 1.0
    pub max_tokens: Option<u32>,        // Maximum tokens to generate
    pub top_p: Option<f32>,             // Nucleus sampling
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

    /// List available models from the provider
    async fn list_models(&self) -> Result<Vec<String>, ApiError>;
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
            _ => ApiError::ProviderRequestFailed(format!(
                "Request failed with status {}: {}",
                status, error
            )),
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
        let client = Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| ApiError::ProviderError(format!("Failed to create HTTP client: {}", e)))?;
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 => {
                    ApiError::ProviderAuthFailed(format!("Authentication failed: {}", error_text))
                }
                429 => ApiError::ProviderRateLimit(format!("Rate limit exceeded: {}", error_text)),
                404 => ApiError::ProviderModelNotFound(format!("Model not found: {}", error_text)),
                _ => ApiError::ProviderRequestFailed(format!("Request failed: {}", error_text)),
            });
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ApiError::ProviderError(format!("Failed to parse response: {}", e)))?;

        let choice = completion
            .choices
            .first()
            .ok_or_else(|| ApiError::ProviderError("No choices in response".to_string()))?;

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
        Err(ApiError::ProviderError(
            "Streaming not yet implemented for OpenAI".to_string(),
        ))
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn list_models(&self) -> Result<Vec<String>, ApiError> {
        let url = format!("{}/models", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::ProviderError(format!(
                "Failed to list models: status {} - {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            id: String,
        }

        let models: ModelsResponse = response.json().await.map_err(|e| {
            ApiError::ProviderError(format!("Failed to parse models response: {}", e))
        })?;

        Ok(models.data.into_iter().map(|m| m.id).collect())
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
        let client = Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| ApiError::ProviderError(format!("Failed to create HTTP client: {}", e)))?;
        Ok(Self {
            client,
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
        let system_message = messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone());

        let user_messages: Vec<String> = messages
            .iter()
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
            request_body["messages"] = json!(user_messages
                .into_iter()
                .map(|content| { json!({"role": "user", "content": content}) })
                .collect::<Vec<_>>());
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 => {
                    ApiError::ProviderAuthFailed(format!("Authentication failed: {}", error_text))
                }
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

        let completion: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| ApiError::ProviderError(format!("Failed to parse response: {}", e)))?;

        let content = completion
            .content
            .first()
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
        Err(ApiError::ProviderError(
            "Streaming not yet implemented for Anthropic".to_string(),
        ))
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn list_models(&self) -> Result<Vec<String>, ApiError> {
        // Anthropic doesn't have a public models list endpoint
        // Return an error indicating this isn't supported
        Err(ApiError::ProviderError(
            "Anthropic API does not provide a models list endpoint".to_string(),
        ))
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
        let client = Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| ApiError::ProviderError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::ProviderRequestFailed(format!(
                "Request failed with status {}: {}",
                status, error_text
            )));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ApiError::ProviderError(format!("Failed to parse response: {}", e)))?;

        let choice = completion
            .choices
            .first()
            .ok_or_else(|| ApiError::ProviderError("No choices in response".to_string()))?;

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
        Err(ApiError::ProviderError(
            "Streaming not yet implemented for Ollama".to_string(),
        ))
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn list_models(&self) -> Result<Vec<String>, ApiError> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await.map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::ProviderError(format!(
                "Failed to list models: status {} - {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let tags: TagsResponse = response.json().await.map_err(|e| {
            ApiError::ProviderError(format!("Failed to parse models response: {}", e))
        })?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
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
        let client = Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| ApiError::ProviderError(format!("Failed to create HTTP client: {}", e)))?;
        Ok(Self {
            client,
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
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request_builder
            .json(&request)
            .send()
            .await
            .map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::ProviderRequestFailed(format!(
                "Request failed with status {}: {}",
                status, error_text
            )));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ApiError::ProviderError(format!("Failed to parse response: {}", e)))?;

        let choice = completion
            .choices
            .first()
            .ok_or_else(|| ApiError::ProviderError("No choices in response".to_string()))?;

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
        Err(ApiError::ProviderError(
            "Streaming not yet implemented for CustomLocal".to_string(),
        ))
    }

    fn provider_name(&self) -> &str {
        "local"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn list_models(&self) -> Result<Vec<String>, ApiError> {
        // Try OpenAI-compatible /v1/models endpoint
        let url = format!("{}/models", self.endpoint);
        let mut request_builder = self
            .client
            .get(&url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &self.api_key {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request_builder.send().await.map_err(map_http_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::ProviderError(format!(
                "Failed to list models: status {} - {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            id: String,
        }

        let models: ModelsResponse = response.json().await.map_err(|e| {
            ApiError::ProviderError(format!("Failed to parse models response: {}", e))
        })?;

        Ok(models.data.into_iter().map(|m| m.id).collect())
    }
}

/// Provider factory for creating provider clients
pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create_client(
        provider: &ModelProvider,
    ) -> Result<Box<dyn ModelProviderClient>, ApiError> {
        match provider {
            ModelProvider::OpenAI {
                model,
                api_key,
                base_url,
            } => Ok(Box::new(OpenAIClient::new(
                model.clone(),
                api_key.clone(),
                base_url.clone(),
            )?)),
            ModelProvider::Anthropic { model, api_key } => Ok(Box::new(AnthropicClient::new(
                model.clone(),
                api_key.clone(),
            )?)),
            ModelProvider::Ollama { model, base_url } => Ok(Box::new(OllamaClient::new(
                model.clone(),
                base_url.clone(),
            )?)),
            ModelProvider::LocalCustom {
                model,
                endpoint,
                api_key,
            } => Ok(Box::new(CustomLocalClient::new(
                model.clone(),
                endpoint.clone(),
                api_key.clone(),
            )?)),
        }
    }
}

/// Provider registry for managing provider configurations independently
///
/// Manages provider configurations separately from agents, enabling
/// runtime provider selection and reuse across multiple agents.
pub struct ProviderRegistry {
    providers: std::collections::HashMap<String, crate::config::ProviderConfig>,
}

impl ProviderRegistry {
    /// Create a new empty provider registry
    pub fn new() -> Self {
        Self {
            providers: std::collections::HashMap::new(),
        }
    }

    /// Load providers from configuration
    ///
    /// For Phase 1, loads from MerkleConfig. Phase 2 will add load_from_xdg().
    pub fn load_from_config(
        &mut self,
        config: &crate::config::MerkleConfig,
    ) -> Result<(), ApiError> {
        for (name, provider_config) in &config.providers {
            let mut config_with_name = provider_config.clone();
            // Set provider_name if not already set
            if config_with_name.provider_name.is_none() {
                config_with_name.provider_name = Some(name.clone());
            }
            self.providers.insert(name.clone(), config_with_name);
        }
        Ok(())
    }

    /// Load providers from XDG directory
    ///
    /// Scans `$XDG_CONFIG_HOME/merkle/providers/*.toml` and loads each provider configuration.
    /// Invalid configs are logged but don't stop loading of other providers.
    pub fn load_from_xdg(&mut self) -> Result<(), ApiError> {
        let providers_dir = crate::config::xdg::providers_dir()?;

        if !providers_dir.exists() {
            // Directory doesn't exist yet - that's okay
            return Ok(());
        }

        let entries = std::fs::read_dir(&providers_dir).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to read providers directory {}: {}",
                providers_dir.display(),
                e
            ))
        })?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(
                        "Failed to read directory entry in {}: {}",
                        providers_dir.display(),
                        e
                    );
                    continue;
                }
            };

            let path = entry.path();

            // Only process .toml files
            if path.extension() != Some(std::ffi::OsStr::new("toml")) {
                continue;
            }

            // Extract provider name from filename
            let provider_name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(name) => name,
                None => {
                    tracing::warn!("Invalid provider filename (non-UTF8): {:?}", path);
                    continue;
                }
            };

            // Load and parse TOML
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to read provider config {}: {}", path.display(), e);
                    continue;
                }
            };

            let provider_config: crate::config::ProviderConfig = match toml::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    tracing::error!("Failed to parse provider config {}: {}", path.display(), e);
                    continue;
                }
            };

            // Validate provider_name matches filename
            if let Some(ref config_name) = provider_config.provider_name {
                if config_name != provider_name {
                    tracing::warn!(
                        "Provider name mismatch in {}: filename={}, config={}",
                        path.display(),
                        provider_name,
                        config_name
                    );
                }
            }

            // Set provider_name from filename if not set
            let mut config = provider_config;
            if config.provider_name.is_none() {
                config.provider_name = Some(provider_name.to_string());
            }

            // Validate configuration
            if let Err(e) = config.validate() {
                tracing::error!("Invalid provider config {}: {}", path.display(), e);
                continue; // Skip invalid configs but continue loading others
            }

            // Insert or override (XDG configs override config.toml)
            self.providers.insert(provider_name.to_string(), config);
        }

        Ok(())
    }

    /// Get a provider configuration by name
    pub fn get(&self, provider_name: &str) -> Option<&crate::config::ProviderConfig> {
        self.providers.get(provider_name)
    }

    /// Get a provider configuration by name or return an error
    pub fn get_or_error(
        &self,
        provider_name: &str,
    ) -> Result<&crate::config::ProviderConfig, ApiError> {
        self.get(provider_name).ok_or_else(|| {
            ApiError::ProviderNotConfigured(format!("Provider not found: {}", provider_name))
        })
    }

    /// List all registered providers
    pub fn list_all(&self) -> Vec<&crate::config::ProviderConfig> {
        self.providers.values().collect()
    }

    /// Create a provider client from a provider name
    ///
    /// Looks up the provider configuration, converts it to a ModelProvider,
    /// and creates the appropriate client implementation.
    pub fn create_client(
        &self,
        provider_name: &str,
    ) -> Result<Box<dyn ModelProviderClient>, ApiError> {
        let provider_config = self.get_or_error(provider_name)?;
        let model_provider = provider_config.to_model_provider()?;
        ProviderFactory::create_client(&model_provider)
    }

    /// List providers filtered by type
    pub fn list_by_type(
        &self,
        provider_type: Option<crate::config::ProviderType>,
    ) -> Vec<&crate::config::ProviderConfig> {
        if let Some(filter_type) = provider_type {
            self.providers
                .values()
                .filter(|provider| provider.provider_type == filter_type)
                .collect()
        } else {
            self.list_all()
        }
    }

    /// Get the XDG config file path for a provider
    pub fn get_provider_config_path(provider_name: &str) -> Result<std::path::PathBuf, ApiError> {
        let providers_dir = crate::config::xdg::providers_dir()?;
        Ok(providers_dir.join(format!("{}.toml", provider_name)))
    }

    /// Save provider configuration to XDG directory
    pub fn save_provider_config(
        provider_name: &str,
        config: &crate::config::ProviderConfig,
    ) -> Result<(), ApiError> {
        let config_path = Self::get_provider_config_path(provider_name)?;

        // Ensure providers directory exists
        let providers_dir = crate::config::xdg::providers_dir()?;
        std::fs::create_dir_all(&providers_dir).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to create providers directory {}: {}",
                providers_dir.display(),
                e
            ))
        })?;

        // Ensure provider_name is set in config
        let mut config = config.clone();
        if config.provider_name.is_none() {
            config.provider_name = Some(provider_name.to_string());
        }

        // Serialize config to TOML
        let toml_content = toml::to_string_pretty(&config).map_err(|e| {
            ApiError::ConfigError(format!("Failed to serialize provider config: {}", e))
        })?;

        // Write to file
        std::fs::write(&config_path, toml_content).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to write provider config to {}: {}",
                config_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    /// Delete provider configuration file
    pub fn delete_provider_config(provider_name: &str) -> Result<(), ApiError> {
        let config_path = Self::get_provider_config_path(provider_name)?;

        if !config_path.exists() {
            return Err(ApiError::ConfigError(format!(
                "Provider config file not found: {}",
                config_path.display()
            )));
        }

        std::fs::remove_file(&config_path).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to delete provider config file {}: {}",
                config_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    /// Validate provider configuration
    pub fn validate_provider(&self, provider_name: &str) -> Result<ValidationResult, ApiError> {
        let mut result = ValidationResult::new(provider_name.to_string());

        // Check if provider exists in registry
        let _provider = match self.get(provider_name) {
            Some(p) => p,
            None => {
                result.add_error("Provider not found in registry".to_string());
                return Ok(result);
            }
        };

        // Get config file path
        let config_path = Self::get_provider_config_path(provider_name)?;

        // Check if config file exists
        if !config_path.exists() {
            result.add_error(format!("Config file not found: {}", config_path.display()));
            return Ok(result);
        }

        // Validate provider name matches filename
        let expected_filename = format!("{}.toml", provider_name);
        if config_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == expected_filename)
            .unwrap_or(false)
        {
            result.add_check("Provider name matches filename", true);
        } else {
            result.add_error(format!(
                "Provider name '{}' doesn't match filename '{}'",
                provider_name,
                config_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
            ));
        }

        // Validate provider type is valid (should always be valid if loaded)
        result.add_check("Provider type is valid", true);

        // Load and validate config file
        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) => {
                result.add_error(format!("Failed to read config file: {}", e));
                return Ok(result);
            }
        };

        let provider_config: crate::config::ProviderConfig = match toml::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                result.add_error(format!("Failed to parse config file: {}", e));
                return Ok(result);
            }
        };

        // Validate model is not empty
        if provider_config.model.trim().is_empty() {
            result.add_error("Model name cannot be empty".to_string());
        } else {
            result.add_check("Model is not empty", true);
        }

        // Check API key availability for cloud providers
        match provider_config.provider_type {
            crate::config::ProviderType::OpenAI | crate::config::ProviderType::Anthropic => {
                let api_key_available = provider_config.api_key.is_some()
                    || match provider_config.provider_type {
                        crate::config::ProviderType::OpenAI => {
                            std::env::var("OPENAI_API_KEY").is_ok()
                        }
                        crate::config::ProviderType::Anthropic => {
                            std::env::var("ANTHROPIC_API_KEY").is_ok()
                        }
                        _ => false,
                    };

                if api_key_available {
                    let source = if provider_config.api_key.is_some() {
                        "from config"
                    } else {
                        "from environment"
                    };
                    result.add_check(&format!("API key available ({})", source), true);
                } else {
                    let env_var = match provider_config.provider_type {
                        crate::config::ProviderType::OpenAI => "OPENAI_API_KEY",
                        crate::config::ProviderType::Anthropic => "ANTHROPIC_API_KEY",
                        _ => unreachable!(),
                    };
                    result.add_error(format!(
                        "API key not found (set {} or add to config)",
                        env_var
                    ));
                }
            }
            crate::config::ProviderType::Ollama | crate::config::ProviderType::LocalCustom => {
                result.add_check("API key not required for local provider", true);
            }
        }

        // Validate endpoint URL if provided
        if let Some(ref endpoint) = provider_config.endpoint {
            if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                result.add_check("Endpoint URL is valid", true);
            } else {
                result.add_error(format!("Invalid endpoint URL: {}", endpoint));
            }
        } else {
            // Endpoint is optional for most providers, required for LocalCustom
            if provider_config.provider_type == crate::config::ProviderType::LocalCustom {
                result.add_error("Endpoint is required for local custom provider".to_string());
            } else {
                result.add_check("Endpoint URL (optional)", true);
            }
        }

        // Validate completion options
        if let Some(temp) = provider_config.default_options.temperature {
            if temp >= 0.0 && temp <= 2.0 {
                result.add_check("Temperature is in valid range (0.0-2.0)", true);
            } else {
                result.add_error(format!(
                    "Temperature must be between 0.0 and 2.0, got {}",
                    temp
                ));
            }
        }

        if let Some(max_tokens) = provider_config.default_options.max_tokens {
            if max_tokens > 0 {
                result.add_check("Max tokens is positive", true);
            } else {
                result.add_error("Max tokens must be positive".to_string());
            }
        }

        if let Some(top_p) = provider_config.default_options.top_p {
            if top_p >= 0.0 && top_p <= 1.0 {
                result.add_check("Top-p is in valid range (0.0-1.0)", true);
            } else {
                result.add_error(format!("Top-p must be between 0.0 and 1.0, got {}", top_p));
            }
        }

        Ok(result)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation result for provider configuration
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub provider_name: String,
    pub checks: Vec<(String, bool)>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new(provider_name: String) -> Self {
        Self {
            provider_name,
            checks: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_check(&mut self, description: &str, passed: bool) {
        self.checks.push((description.to_string(), passed));
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn total_checks(&self) -> usize {
        self.checks.len()
    }

    pub fn passed_checks(&self) -> usize {
        self.checks.iter().filter(|(_, passed)| *passed).count()
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
        Err(ApiError::ProviderError(
            "Streaming not implemented for mock".to_string(),
        ))
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn list_models(&self) -> Result<Vec<String>, ApiError> {
        // Mock provider returns empty list for testing
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ProviderType};
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mutex to serialize XDG_CONFIG_HOME environment variable access in tests
    static XDG_CONFIG_MUTEX: Mutex<()> = Mutex::new(());

    /// Helper to set up XDG_CONFIG_HOME for a test with proper cleanup
    fn with_xdg_config_home<F, R>(test_dir: &TempDir, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = XDG_CONFIG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let original_xdg_config = std::env::var("XDG_CONFIG_HOME").ok();
        let test_config_home = test_dir.path().to_path_buf();
        std::env::set_var("XDG_CONFIG_HOME", test_config_home.to_str().unwrap());

        let result = f();

        // Restore original
        if let Some(orig) = original_xdg_config {
            std::env::set_var("XDG_CONFIG_HOME", orig);
        } else {
            std::env::remove_var("XDG_CONFIG_HOME");
        }

        result
    }

    #[test]
    fn test_provider_registry_list_by_type() {
        let mut registry = ProviderRegistry::new();

        let provider1 = ProviderConfig {
            provider_name: Some("test-openai".to_string()),
            provider_type: ProviderType::OpenAI,
            model: "gpt-4".to_string(),
            api_key: None,
            endpoint: None,
            default_options: CompletionOptions::default(),
        };

        let provider2 = ProviderConfig {
            provider_name: Some("test-ollama".to_string()),
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            api_key: None,
            endpoint: Some("http://localhost:11434".to_string()),
            default_options: CompletionOptions::default(),
        };

        let provider3 = ProviderConfig {
            provider_name: Some("test-anthropic".to_string()),
            provider_type: ProviderType::Anthropic,
            model: "claude-3-opus".to_string(),
            api_key: None,
            endpoint: None,
            default_options: CompletionOptions::default(),
        };

        registry
            .providers
            .insert("test-openai".to_string(), provider1);
        registry
            .providers
            .insert("test-ollama".to_string(), provider2);
        registry
            .providers
            .insert("test-anthropic".to_string(), provider3);

        // Test filtering by type
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

        // Test listing all
        let all_providers = registry.list_by_type(None);
        assert_eq!(all_providers.len(), 3);
    }

    #[test]
    fn test_provider_registry_get_provider_config_path() {
        let test_dir = TempDir::new().unwrap();
        with_xdg_config_home(&test_dir, || {
            let path = ProviderRegistry::get_provider_config_path("test-provider").unwrap();
            let providers_dir = crate::config::xdg::providers_dir().unwrap();
            assert_eq!(path, providers_dir.join("test-provider.toml"));
        });
    }

    #[test]
    fn test_provider_registry_save_and_delete() {
        let test_dir = TempDir::new().unwrap();
        with_xdg_config_home(&test_dir, || {
            let provider_config = ProviderConfig {
                provider_name: Some("test-provider".to_string()),
                provider_type: ProviderType::Ollama,
                model: "llama2".to_string(),
                api_key: None,
                endpoint: Some("http://localhost:11434".to_string()),
                default_options: CompletionOptions::default(),
            };

            // Save provider config
            ProviderRegistry::save_provider_config("test-provider", &provider_config).unwrap();

            // Verify file exists
            let config_path = ProviderRegistry::get_provider_config_path("test-provider").unwrap();
            assert!(config_path.exists());

            // Load and verify content
            let content = std::fs::read_to_string(&config_path).unwrap();
            assert!(content.contains("test-provider"));
            assert!(content.contains("ollama"));
            assert!(content.contains("llama2"));

            // Delete provider config
            ProviderRegistry::delete_provider_config("test-provider").unwrap();

            // Verify file is deleted
            assert!(!config_path.exists());
        });
    }

    #[test]
    fn test_provider_registry_validate_provider() {
        let test_dir = TempDir::new().unwrap();
        with_xdg_config_home(&test_dir, || {
            // Create a valid provider
            let provider_config = ProviderConfig {
                provider_name: Some("test-provider".to_string()),
                provider_type: ProviderType::Ollama,
                model: "llama2".to_string(),
                api_key: None,
                endpoint: Some("http://localhost:11434".to_string()),
                default_options: CompletionOptions::default(),
            };

            ProviderRegistry::save_provider_config("test-provider", &provider_config).unwrap();

            // Load registry and validate
            let mut registry = ProviderRegistry::new();
            registry.load_from_xdg().unwrap();

            let result = registry.validate_provider("test-provider").unwrap();

            // Should have some checks
            assert!(result.total_checks() > 0);
            // Should pass basic validation (model not empty, etc.)
            assert!(result
                .checks
                .iter()
                .any(|(desc, _)| desc.contains("Model is not empty")));
        });
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new("test-provider".to_string());

        assert_eq!(result.provider_name, "test-provider");
        assert_eq!(result.total_checks(), 0);
        assert_eq!(result.passed_checks(), 0);
        assert!(result.is_valid());

        result.add_check("Test check 1", true);
        result.add_check("Test check 2", false);
        result.add_error("Test error".to_string());
        result.add_warning("Test warning".to_string());

        assert_eq!(result.total_checks(), 2);
        assert_eq!(result.passed_checks(), 1);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.warnings.len(), 1);
        assert!(!result.is_valid());
    }

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

        let response1 = mock
            .complete(messages.clone(), CompletionOptions::default())
            .await
            .unwrap();
        assert_eq!(response1.content, "Response 1");
        assert_eq!(response1.model, "mock-model");

        let response2 = mock
            .complete(messages, CompletionOptions::default())
            .await
            .unwrap();
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
