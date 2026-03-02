//! Context Frames
//!
//! Immutable containers for context information associated with filesystem nodes.
//! Each frame is content-addressed and append-only.

pub mod id;
pub mod set;
pub mod storage;

pub use set::FrameMerkleSet;
pub use storage::FrameStorage;

use crate::error::StorageError;
use crate::metadata::frame_types::FrameMetadata;
use crate::types::{FrameID, NodeID};
use std::path::Path;

/// Open frame storage at the given path. Use this instead of reaching into `storage` internals.
pub fn open_storage(path: &Path) -> Result<FrameStorage, StorageError> {
    storage::FrameStorage::new(path)
}
use serde::{Deserialize, Serialize};

/// Basis for a context frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Basis {
    Node(NodeID),
    Frame(FrameID),
    Both { node: NodeID, frame: FrameID },
}

/// Context frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub frame_id: FrameID,
    pub basis: Basis,
    pub content: Vec<u8>,                  // Blob
    pub frame_type: String,                // Frame type identifier
    pub metadata: FrameMetadata, // Non-hashed
    pub timestamp: std::time::SystemTime,
}

impl Frame {
    /// Create a new frame with computed FrameID
    ///
    /// The FrameID is computed deterministically from the basis, agent_id, content, and frame_type.
    /// The agent_id is included in both the FrameID computation and the metadata (Phase 2A requirement).
    pub fn new(
        basis: Basis,
        content: Vec<u8>,
        frame_type: String,
        agent_id: String,
        metadata: impl Into<FrameMetadata>,
    ) -> Result<Self, crate::error::StorageError> {
        // Ensure agent_id is in metadata (Phase 2A: agent identity preserved in all frames)
        let mut metadata: FrameMetadata = metadata.into();
        metadata.insert("agent_id".to_string(), agent_id.clone());

        // Compute FrameID with agent_id included in hash
        let frame_id = id::compute_frame_id(&basis, &content, &frame_type, &agent_id)?;

        Ok(Frame {
            frame_id,
            basis,
            content,
            frame_type,
            metadata,
            timestamp: std::time::SystemTime::now(),
        })
    }

    /// Get content as UTF-8 string
    ///
    /// Returns an error if the content is not valid UTF-8.
    pub fn text_content(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.content.clone())
    }

    /// Parse content as JSON
    ///
    /// Attempts to deserialize the frame content as JSON into the specified type.
    pub fn json_content<T>(&self) -> Result<T, serde_json::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_slice(&self.content)
    }

    /// Get agent ID from metadata
    ///
    /// Returns the agent_id stored in the frame's metadata, if present.
    pub fn agent_id(&self) -> Option<&str> {
        self.metadata.get("agent_id").map(|s| s.as_str())
    }

    /// Get metadata value by key
    ///
    /// Returns the metadata value for the given key, if present.
    pub fn metadata_value(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }

    /// Check if this frame is marked as deleted.
    pub fn is_deleted(&self) -> bool {
        self.metadata_value("deleted") == Some("true")
    }

    /// Check if frame matches the specified type
    ///
    /// Returns true if the frame's type matches the given frame_type.
    pub fn is_type(&self, frame_type: &str) -> bool {
        self.frame_type == frame_type
    }
}
