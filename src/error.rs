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

    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("Regeneration failed: {0}")]
    RegenerationFailed(String),

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

    #[error("Path not found in tree: {0}. Run `merkle scan` to update tree or start `merkle watch`.")]
    PathNotInTree(std::path::PathBuf),
}

impl From<config::ConfigError> for ApiError {
    fn from(err: config::ConfigError) -> Self {
        ApiError::ConfigError(err.to_string())
    }
}
