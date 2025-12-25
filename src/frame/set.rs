//! Context Frame Merkle Set
//!
//! Maintains a deterministic set of frames associated with a node
//! using a Merkle set structure.

use crate::types::{FrameID, Hash};

/// Frame Merkle Set
pub struct FrameMerkleSet {
    // TODO: Implement Merkle set using rs-merkle
}

impl Default for FrameMerkleSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameMerkleSet {
    pub fn new() -> Self {
        // TODO: Implement
        FrameMerkleSet {}
    }

    pub fn add_frame(&mut self, _frame_id: FrameID) -> Result<Hash, crate::error::StorageError> {
        // TODO: Implement frame addition and return new set root
        Ok([0u8; 32])
    }

    pub fn root(&self) -> Option<Hash> {
        // TODO: Implement root hash retrieval
        None
    }
}
