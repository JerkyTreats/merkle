//! Error types for the Merkle filesystem state management system.

use crate::types::{FrameID, Hash, NodeID};
use thiserror::Error;

/// Storage-related errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Node not found: {0:?}")]
    NodeNotFound(NodeID),

    #[error("Frame not found: {0:?}")]
    FrameNotFound(FrameID),

    #[error("Hash mismatch: expected {expected:?}, got {actual:?}")]
    HashMismatch { expected: Hash, actual: Hash },

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Storage I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

impl Clone for StorageError {
    fn clone(&self) -> Self {
        match self {
            StorageError::NodeNotFound(node_id) => StorageError::NodeNotFound(*node_id),
            StorageError::FrameNotFound(frame_id) => StorageError::FrameNotFound(*frame_id),
            StorageError::HashMismatch { expected, actual } => StorageError::HashMismatch {
                expected: *expected,
                actual: *actual,
            },
            StorageError::InvalidPath(path) => StorageError::InvalidPath(path.clone()),
            StorageError::IoError(err) => {
                StorageError::IoError(std::io::Error::new(err.kind(), err.to_string()))
            }
        }
    }
}

/// API-related errors for Phase 2
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Node not found: {0:?}")]
    NodeNotFound(NodeID),

    #[error("Frame not found: {0:?}")]
    FrameNotFound(FrameID),

    #[error("Agent unauthorized: {0}")]
    Unauthorized(String),

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Provider not configured: {0}")]
    ProviderNotConfigured(String),

    #[error("Provider request failed: {0}")]
    ProviderRequestFailed(String),

    #[error("Provider authentication failed: {0}")]
    ProviderAuthFailed(String),

    #[error("Provider rate limit exceeded: {0}")]
    ProviderRateLimit(String),

    #[error("Provider model not found: {0}")]
    ProviderModelNotFound(String),

    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Generation failed: {0}")]
    GenerationFailed(String),

    #[error(
        "Path not found in tree: {0}. Run `merkle scan` to update tree or start `merkle watch`."
    )]
    PathNotInTree(std::path::PathBuf),
}

impl Clone for ApiError {
    fn clone(&self) -> Self {
        match self {
            ApiError::NodeNotFound(node_id) => ApiError::NodeNotFound(*node_id),
            ApiError::FrameNotFound(frame_id) => ApiError::FrameNotFound(*frame_id),
            ApiError::Unauthorized(message) => ApiError::Unauthorized(message.clone()),
            ApiError::InvalidFrame(message) => ApiError::InvalidFrame(message.clone()),
            ApiError::ProviderError(message) => ApiError::ProviderError(message.clone()),
            ApiError::ProviderNotConfigured(message) => {
                ApiError::ProviderNotConfigured(message.clone())
            }
            ApiError::ProviderRequestFailed(message) => {
                ApiError::ProviderRequestFailed(message.clone())
            }
            ApiError::ProviderAuthFailed(message) => ApiError::ProviderAuthFailed(message.clone()),
            ApiError::ProviderRateLimit(message) => ApiError::ProviderRateLimit(message.clone()),
            ApiError::ProviderModelNotFound(message) => {
                ApiError::ProviderModelNotFound(message.clone())
            }
            ApiError::StorageError(err) => ApiError::StorageError(err.clone()),
            ApiError::ConfigError(message) => ApiError::ConfigError(message.clone()),
            ApiError::GenerationFailed(message) => ApiError::GenerationFailed(message.clone()),
            ApiError::PathNotInTree(path) => ApiError::PathNotInTree(path.clone()),
        }
    }
}

impl From<config::ConfigError> for ApiError {
    fn from(err: config::ConfigError) -> Self {
        ApiError::ConfigError(err.to_string())
    }
}
