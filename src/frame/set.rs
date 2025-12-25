//! Context Frame Merkle Set
//!
//! Maintains a deterministic set of frames associated with a node
//! using a Merkle set structure.

use crate::error::StorageError;
use crate::types::{FrameID, Hash};
use blake3::Hasher;
use rs_merkle::{Hasher as RsHasher, MerkleTree};
use std::collections::BTreeSet;

/// BLAKE3 algorithm adapter for rs-merkle
///
/// Implements the rs_merkle::Hasher trait to use BLAKE3 for Merkle tree construction.
#[derive(Clone, Debug)]
pub struct Blake3Hasher;

impl RsHasher for Blake3Hasher {
    type Hash = [u8; 32];

    fn hash(data: &[u8]) -> Self::Hash {
        let mut hasher = Hasher::new();
        hasher.update(data);
        *hasher.finalize().as_bytes()
    }
}

/// Frame Merkle Set
///
/// Maintains a deterministic set of frames using a Merkle tree structure.
/// Frames are stored in sorted order (by FrameID) to ensure deterministic
/// root hash computation regardless of insertion order.
pub struct FrameMerkleSet {
    /// Sorted set of FrameIDs (deterministic ordering)
    frames: BTreeSet<FrameID>,
    /// Cached root hash (None for empty set)
    root: Option<Hash>,
}

impl Default for FrameMerkleSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameMerkleSet {
    /// Create a new empty Frame Merkle Set
    pub fn new() -> Self {
        FrameMerkleSet {
            frames: BTreeSet::new(),
            root: Some(compute_empty_set_hash()),
        }
    }

    /// Add a frame to the set
    ///
    /// If the frame already exists, this is a no-op and returns the current root.
    /// Otherwise, adds the frame, rebuilds the Merkle tree, and returns the new root.
    ///
    /// The root hash is deterministic: same set of frames → same root,
    /// regardless of insertion order.
    pub fn add_frame(&mut self, frame_id: FrameID) -> Result<Hash, StorageError> {
        // Check if frame already exists (no-op)
        if self.frames.contains(&frame_id) {
            return Ok(self.root.expect("Root should exist if frames exist"));
        }

        // Add frame to sorted set
        self.frames.insert(frame_id);

        // Rebuild Merkle tree and compute new root
        self.rebuild_tree()
    }

    /// Remove a frame from the set
    ///
    /// If the frame doesn't exist, this is a no-op and returns the current root.
    /// Otherwise, removes the frame, rebuilds the Merkle tree, and returns the new root.
    pub fn remove_frame(&mut self, frame_id: FrameID) -> Result<Hash, StorageError> {
        // Check if frame exists
        if !self.frames.contains(&frame_id) {
            return Ok(self.root.expect("Root should exist"));
        }

        // Remove frame from set
        self.frames.remove(&frame_id);

        // Rebuild Merkle tree and compute new root
        self.rebuild_tree()
    }

    /// Get the current root hash of the Merkle set
    ///
    /// Returns `None` only if the set is in an invalid state (should not happen).
    /// Empty sets return `EMPTY_SET_HASH`.
    pub fn root(&self) -> Option<Hash> {
        self.root
    }

    /// Check if a frame is in the set
    pub fn contains(&self, frame_id: &FrameID) -> bool {
        self.frames.contains(frame_id)
    }

    /// Get the number of frames in the set
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Get all FrameIDs in the set (in sorted order)
    pub fn frame_ids(&self) -> impl Iterator<Item = &FrameID> {
        self.frames.iter()
    }

    /// Rebuild the Merkle tree from the current set of frames
    ///
    /// This is called after any modification to the set.
    /// For Phase 1, we do a full rebuild (O(n log n)).
    /// Phase 2 will optimize this with incremental updates.
    fn rebuild_tree(&mut self) -> Result<Hash, StorageError> {
        // Handle empty set
        if self.frames.is_empty() {
            let empty_hash = compute_empty_set_hash();
            self.root = Some(empty_hash);
            return Ok(empty_hash);
        }

        // Convert FrameIDs to leaf hashes
        // Each leaf is hashed with a prefix to distinguish from internal nodes
        let leaves: Vec<[u8; 32]> = self
            .frames
            .iter()
            .map(|frame_id| {
                // Hash with "frame_leaf" prefix for leaf nodes
                let mut hasher = Hasher::new();
                hasher.update(b"frame_leaf");
                hasher.update(frame_id);
                *hasher.finalize().as_bytes()
            })
            .collect();

        // Build Merkle tree from leaves
        let tree = MerkleTree::<Blake3Hasher>::from_leaves(&leaves);

        // Get root hash
        let root_opt = tree.root();
        let root = root_opt.ok_or_else(|| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to compute Merkle tree root",
            ))
        })?;

        self.root = Some(root);
        Ok(root)
    }

    /// Create a FrameMerkleSet from a collection of FrameIDs
    ///
    /// Useful for reconstructing a set from stored FrameIDs.
    pub fn from_frame_ids<I>(frame_ids: I) -> Result<Self, StorageError>
    where
        I: IntoIterator<Item = FrameID>,
    {
        let mut set = FrameMerkleSet {
            frames: BTreeSet::new(),
            root: None,
        };

        for frame_id in frame_ids {
            set.frames.insert(frame_id);
        }

        // Rebuild tree to compute root
        set.rebuild_tree()?;

        Ok(set)
    }
}

/// Compute the empty set root hash
///
/// This is a stable hash for an empty frame set.
/// Computed as: BLAKE3("empty_frame_set")
fn compute_empty_set_hash() -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"empty_frame_set");
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_set() {
        let set = FrameMerkleSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert!(set.root().is_some());
    }

    #[test]
    fn test_add_frame() {
        let mut set = FrameMerkleSet::new();
        let frame_id: FrameID = [1u8; 32];

        let root1 = set.add_frame(frame_id).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains(&frame_id));
        assert!(root1 != compute_empty_set_hash());

        // Adding same frame again should be no-op
        let root2 = set.add_frame(frame_id).unwrap();
        assert_eq!(root1, root2);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_deterministic_root() {
        let mut set1 = FrameMerkleSet::new();
        let mut set2 = FrameMerkleSet::new();

        let frame_id1: FrameID = [1u8; 32];
        let frame_id2: FrameID = [2u8; 32];
        let frame_id3: FrameID = [3u8; 32];

        // Add frames in different orders
        set1.add_frame(frame_id1).unwrap();
        set1.add_frame(frame_id2).unwrap();
        set1.add_frame(frame_id3).unwrap();

        set2.add_frame(frame_id3).unwrap();
        set2.add_frame(frame_id1).unwrap();
        set2.add_frame(frame_id2).unwrap();

        // Roots should be identical (deterministic ordering)
        assert_eq!(set1.root(), set2.root());
    }

    #[test]
    fn test_remove_frame() {
        let mut set = FrameMerkleSet::new();
        let frame_id1: FrameID = [1u8; 32];
        let frame_id2: FrameID = [2u8; 32];

        set.add_frame(frame_id1).unwrap();
        set.add_frame(frame_id2).unwrap();
        assert_eq!(set.len(), 2);

        let root_before = set.root().unwrap();

        // Remove frame
        let root_after = set.remove_frame(frame_id1).unwrap();
        assert_eq!(set.len(), 1);
        assert!(!set.contains(&frame_id1));
        assert!(set.contains(&frame_id2));
        assert_ne!(root_before, root_after);

        // Removing non-existent frame should be no-op
        let root_unchanged = set.remove_frame(frame_id1).unwrap();
        assert_eq!(root_after, root_unchanged);
    }

    #[test]
    fn test_from_frame_ids() {
        let frame_ids = vec![
            [1u8; 32],
            [2u8; 32],
            [3u8; 32],
        ];

        let set = FrameMerkleSet::from_frame_ids(frame_ids).unwrap();
        assert_eq!(set.len(), 3);
        assert!(set.root().is_some());
    }

    #[test]
    fn test_empty_set_root_stable() {
        let set1 = FrameMerkleSet::new();
        let set2 = FrameMerkleSet::new();

        // Empty sets should have the same root
        assert_eq!(set1.root(), set2.root());
    }
}
