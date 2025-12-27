//! Agent Adapter
//!
//! Trait for internal agent integration, providing a simplified interface
//! for agents to interact with the context engine.

use crate::api::{ContextApi, ContextView, NodeContext};
use crate::error::ApiError;
use crate::frame::{Basis, Frame};
use crate::provider::{ChatMessage, CompletionOptions, ModelProviderClient, ProviderFactory};
use crate::types::{FrameID, NodeID};
use async_trait::async_trait;
use std::collections::HashMap;

/// Enhance ProviderModelNotFound errors with available models list
async fn enhance_model_error(
    error: ApiError,
    client: &dyn ModelProviderClient,
    requested_model: &str,
) -> ApiError {
    if let ApiError::ProviderModelNotFound(_) = error {
        // Try to get list of available models
        match client.list_models().await {
            Ok(available_models) => {
                if available_models.is_empty() {
                    ApiError::ProviderModelNotFound(format!(
                        "Model '{}' not found. Unable to retrieve available models list.",
                        requested_model
                    ))
                } else {
                    ApiError::ProviderModelNotFound(format!(
                        "Model '{}' not found. Available models: {}",
                        requested_model,
                        available_models.join(", ")
                    ))
                }
            }
            Err(_) => {
                // If we can't list models, return original error
                error
            }
        }
    } else {
        // Not a model error, return as-is
        error
    }
}

/// Adapter for internal agents to interact with the context engine
///
/// Provides a simplified interface for agents to read and write context,
/// synthesize frames, and optionally generate frames using LLM providers.
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// Read context for a node using a view policy
    fn read_context(
        &self,
        node_id: NodeID,
        view: ContextView,
    ) -> Result<NodeContext, ApiError>;

    /// Write a context frame to a node
    fn write_context(
        &self,
        node_id: NodeID,
        frame: Frame,
        agent_id: String,
    ) -> Result<FrameID, ApiError>;

    /// Synthesize branch context for a directory node
    fn synthesize(
        &self,
        node_id: NodeID,
        frame_type: String,
        agent_id: String,
    ) -> Result<FrameID, ApiError>;

    /// Generate a frame using an LLM provider (optional)
    ///
    /// This method is async because it may make network requests to LLM providers.
    /// If the agent doesn't have a provider configured, this will return an error.
    async fn generate_frame(
        &self,
        node_id: NodeID,
        prompt: String,
        frame_type: String,
        agent_id: String,
    ) -> Result<FrameID, ApiError>;
}

/// Implementation of AgentAdapter for ContextApi
pub struct ContextApiAdapter {
    api: ContextApi,
}

impl ContextApiAdapter {
    /// Create a new adapter wrapping a ContextApi
    pub fn new(api: ContextApi) -> Self {
        Self { api }
    }

    /// Get a reference to the underlying API
    pub fn api(&self) -> &ContextApi {
        &self.api
    }
}

#[async_trait]
impl AgentAdapter for ContextApiAdapter {
    fn read_context(
        &self,
        node_id: NodeID,
        view: ContextView,
    ) -> Result<NodeContext, ApiError> {
        self.api.get_node(node_id, view)
    }

    fn write_context(
        &self,
        node_id: NodeID,
        frame: Frame,
        agent_id: String,
    ) -> Result<FrameID, ApiError> {
        self.api.put_frame(node_id, frame, agent_id)
    }

    fn synthesize(
        &self,
        node_id: NodeID,
        frame_type: String,
        agent_id: String,
    ) -> Result<FrameID, ApiError> {
        self.api.synthesize_branch(node_id, frame_type, agent_id, None)
    }

    async fn generate_frame(
        &self,
        node_id: NodeID,
        prompt: String,
        frame_type: String,
        agent_id: String,
    ) -> Result<FrameID, ApiError> {
        // Get agent to check for provider
        let agent = self.api.get_agent(&agent_id)?;

        // Check if agent has provider configured
        let provider = agent.provider.as_ref()
            .ok_or_else(|| ApiError::ProviderNotConfigured(agent_id.clone()))?;

        // Create provider client
        let client = ProviderFactory::create_client(provider)?;

        // Get node context to build prompt
        let view = ContextView {
            max_frames: 10,
            ordering: crate::views::OrderingPolicy::Recency,
            filters: vec![],
        };
        let context = self.api.get_node(node_id, view)?;

        // Build messages for LLM
        let mut messages = vec![
            ChatMessage {
                role: crate::provider::MessageRole::System,
                content: "You are a helpful assistant that generates context frames.".to_string(),
            },
        ];

        // Add context from existing frames
        if !context.frames.is_empty() {
            let context_text: String = context.frames.iter()
                .map(|f| String::from_utf8_lossy(&f.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            messages.push(ChatMessage {
                role: crate::provider::MessageRole::User,
                content: format!("Context:\n{}\n\nTask: {}", context_text, prompt),
            });
        } else {
            messages.push(ChatMessage {
                role: crate::provider::MessageRole::User,
                content: prompt.clone(),
            });
        }

        // Generate completion with enhanced error handling
        let response = match client.complete(
            messages,
            CompletionOptions {
                temperature: Some(0.7),
                max_tokens: Some(2000),
                ..Default::default()
            },
        ).await {
            Ok(r) => Ok(r),
            Err(e) => Err(enhance_model_error(e, client.as_ref(), client.model_name()).await),
        }?;

        // Create frame with generated content
        let basis = Basis::Node(node_id);
        let content = response.content.into_bytes();
        let mut metadata = HashMap::new();
        metadata.insert("provider".to_string(), client.provider_name().to_string());
        metadata.insert("model".to_string(), client.model_name().to_string());
        metadata.insert("provider_type".to_string(), client.provider_name().to_string());
        metadata.insert("prompt".to_string(), prompt.clone());

        let frame = Frame::new(basis, content, frame_type, agent_id.clone(), metadata)?;

        // Store frame using put_frame
        self.api.put_frame(node_id, frame, agent_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ContextApi;
    use crate::heads::HeadIndex;
    use crate::regeneration::BasisIndex;
    use crate::store::persistence::SledNodeRecordStore;
    use crate::types::Hash;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_api() -> (ContextApi, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage_path = temp_dir.path().join("frames");
        std::fs::create_dir_all(&frame_storage_path).unwrap();
        let frame_storage = Arc::new(
            crate::frame::storage::FrameStorage::new(&frame_storage_path).unwrap()
        );
        let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
        let basis_index = Arc::new(parking_lot::RwLock::new(BasisIndex::new()));
        let agent_registry = Arc::new(parking_lot::RwLock::new(crate::agent::AgentRegistry::new()));
        let lock_manager = Arc::new(crate::concurrency::NodeLockManager::new());

        let api = ContextApi::new(
            node_store,
            frame_storage,
            head_index,
            basis_index,
            agent_registry,
            lock_manager,
        );

        (api, temp_dir)
    }

    #[test]
    fn test_adapter_creation() {
        let (api, _temp_dir) = create_test_api();
        let adapter = ContextApiAdapter::new(api);
        assert!(adapter.api().get_node(
            Hash::from([0u8; 32]),
            crate::api::ContextView {
                max_frames: 10,
                ordering: crate::views::OrderingPolicy::Recency,
                filters: vec![],
            }
        ).is_err()); // Should fail because node doesn't exist, but adapter works
    }
}
