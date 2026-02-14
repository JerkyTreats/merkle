//! Branch Context Synthesis
//!
//! Directory-level aggregation of child node context. Combines context frames
//! from child nodes into a single synthesized frame for the parent directory.
//! Synthesis is deterministic, bottom-up, and limited to explicit subtree scope.

use crate::error::ApiError;
use crate::frame::{Frame, FrameStorage};
use crate::heads::HeadIndex;
use crate::store::NodeRecordStore;
use crate::types::{FrameID, Hash, NodeID};
use blake3::Hasher;
use serde::{Deserialize, Serialize};

/// Synthesis policy for aggregating child frame contents
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SynthesisPolicy {
    /// Simple concatenation of child frame contents
    Concatenation,
    /// Generate summary from child frames (deterministic algorithm)
    Summarization,
    /// Select subset of child frames based on criteria
    Filtering {
        /// Maximum number of frames to include
        max_frames: usize,
    },
}

impl Default for SynthesisPolicy {
    fn default() -> Self {
        SynthesisPolicy::Concatenation
    }
}

/// Synthesis basis information
///
/// Contains the information needed to construct a deterministic basis
/// for a synthesized frame.
#[derive(Debug, Clone)]
pub struct SynthesisBasis {
    /// Directory node ID
    pub node_id: NodeID,
    /// Ordered list of child frame IDs
    pub child_frame_ids: Vec<FrameID>,
    /// Frame type for synthesis
    pub frame_type: String,
    /// Synthesis policy used
    pub synthesis_policy: SynthesisPolicy,
}

impl SynthesisBasis {
    /// Compute the basis hash for this synthesis
    ///
    /// The hash is deterministic: same inputs → same hash.
    /// Format: hash(node_id || sorted_child_frame_ids || frame_type || policy)
    pub fn compute_hash(&self) -> Hash {
        let mut hasher = Hasher::new();

        // Include node_id
        hasher.update(&self.node_id);

        // Include sorted child frame IDs (already sorted)
        for frame_id in &self.child_frame_ids {
            hasher.update(frame_id);
        }

        // Include frame type
        hasher.update(self.frame_type.as_bytes());

        // Include policy identifier
        match &self.synthesis_policy {
            SynthesisPolicy::Concatenation => {
                hasher.update(b"concat");
            }
            SynthesisPolicy::Summarization => {
                hasher.update(b"summarize");
            }
            SynthesisPolicy::Filtering { max_frames } => {
                hasher.update(b"filter");
                hasher.update(&max_frames.to_le_bytes());
            }
        }

        *hasher.finalize().as_bytes()
    }
}

/// Synthesize content from child frames using the specified policy
pub fn synthesize_content(child_frames: &[(NodeID, Frame)], policy: &SynthesisPolicy) -> Vec<u8> {
    match policy {
        SynthesisPolicy::Concatenation => {
            // Simple concatenation: join all child frame contents
            let mut result = Vec::new();
            for (_, frame) in child_frames {
                result.extend_from_slice(&frame.content);
                result.push(b'\n'); // Separator between frames
            }
            result
        }
        SynthesisPolicy::Summarization => {
            // Generate a deterministic summary
            // Format: "Summary of {count} frames:\n{frame_type}: {content_length} bytes\n..."
            let mut result = format!("Summary of {} frames:\n", child_frames.len()).into_bytes();
            for (node_id, frame) in child_frames {
                // Format first 4 bytes of node_id as hex
                let node_id_prefix = format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    node_id[0], node_id[1], node_id[2], node_id[3]
                );
                result.extend_from_slice(
                    format!(
                        "  Node {}: {} ({} bytes)\n",
                        node_id_prefix,
                        frame.frame_type,
                        frame.content.len()
                    )
                    .as_bytes(),
                );
            }
            result
        }
        SynthesisPolicy::Filtering { max_frames } => {
            // Take first max_frames frames and concatenate
            let mut result = Vec::new();
            for (_, frame) in child_frames.iter().take(*max_frames) {
                result.extend_from_slice(&frame.content);
                result.push(b'\n');
            }
            result
        }
    }
}

/// Collect child frames for synthesis
///
/// Retrieves the head frame of the specified type for each child node.
/// Returns frames ordered deterministically (by NodeID, then FrameID).
pub fn collect_child_frames(
    node_store: &dyn NodeRecordStore,
    frame_storage: &FrameStorage,
    head_index: &HeadIndex,
    directory_node_id: NodeID,
    frame_type: &str,
) -> Result<Vec<(NodeID, Frame)>, ApiError> {
    // Get directory node record
    let dir_record = node_store
        .get(&directory_node_id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NodeNotFound(directory_node_id))?;

    // Verify it's a directory
    match dir_record.node_type {
        crate::store::NodeType::Directory => {}
        crate::store::NodeType::File { .. } => {
            return Err(ApiError::SynthesisFailed(format!(
                "Node {:?} is a file, not a directory",
                directory_node_id
            )));
        }
    }

    // Collect child frames
    let mut child_frames: Vec<(NodeID, Frame)> = Vec::new();

    for child_node_id in &dir_record.children {
        // Get head frame for this child node and frame type
        if let Some(head_frame_id) = head_index
            .get_head(child_node_id, frame_type)
            .map_err(ApiError::from)?
        {
            // Retrieve the frame
            if let Some(frame) = frame_storage.get(&head_frame_id).map_err(ApiError::from)? {
                child_frames.push((*child_node_id, frame));
            }
        }
    }

    // Sort deterministically: by NodeID, then by FrameID
    child_frames.sort_by(|(node_id_a, frame_a), (node_id_b, frame_b)| {
        node_id_a
            .cmp(node_id_b)
            .then_with(|| frame_a.frame_id.cmp(&frame_b.frame_id))
    });

    Ok(child_frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Basis, Frame};
    use std::collections::HashMap;

    #[test]
    fn test_synthesis_basis_hash_deterministic() {
        let node_id: NodeID = [1u8; 32];
        let frame_id1: FrameID = [2u8; 32];
        let frame_id2: FrameID = [3u8; 32];

        let basis1 = SynthesisBasis {
            node_id,
            child_frame_ids: vec![frame_id1, frame_id2],
            frame_type: "test".to_string(),
            synthesis_policy: SynthesisPolicy::Concatenation,
        };

        let basis2 = SynthesisBasis {
            node_id,
            child_frame_ids: vec![frame_id1, frame_id2],
            frame_type: "test".to_string(),
            synthesis_policy: SynthesisPolicy::Concatenation,
        };

        // Same inputs should produce same hash
        assert_eq!(basis1.compute_hash(), basis2.compute_hash());
    }

    #[test]
    fn test_synthesis_basis_hash_different_for_different_inputs() {
        let node_id: NodeID = [1u8; 32];
        let frame_id1: FrameID = [2u8; 32];
        let frame_id2: FrameID = [3u8; 32];

        let basis1 = SynthesisBasis {
            node_id,
            child_frame_ids: vec![frame_id1, frame_id2],
            frame_type: "test".to_string(),
            synthesis_policy: SynthesisPolicy::Concatenation,
        };

        let basis2 = SynthesisBasis {
            node_id,
            child_frame_ids: vec![frame_id2, frame_id1], // Different order
            frame_type: "test".to_string(),
            synthesis_policy: SynthesisPolicy::Concatenation,
        };

        // Different order should produce different hash
        // (Note: In our implementation, we sort by NodeID first, so order matters)
        // Actually, wait - we sort in collect_child_frames, so the order in the basis
        // should already be deterministic. But let's test that different orders produce different hashes
        // to ensure we're not accidentally making it order-independent.
        let hash1 = basis1.compute_hash();
        let hash2 = basis2.compute_hash();

        // They should be different because we include frame IDs in order
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_synthesize_content_concatenation() {
        let node_id1: NodeID = [1u8; 32];
        let node_id2: NodeID = [2u8; 32];

        let frame1 = Frame {
            frame_id: [10u8; 32],
            basis: Basis::Node(node_id1),
            content: b"content1".to_vec(),
            frame_type: "test".to_string(),
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };

        let frame2 = Frame {
            frame_id: [20u8; 32],
            basis: Basis::Node(node_id2),
            content: b"content2".to_vec(),
            frame_type: "test".to_string(),
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };

        let child_frames = vec![(node_id1, frame1), (node_id2, frame2)];
        let content = synthesize_content(&child_frames, &SynthesisPolicy::Concatenation);

        // Should contain both contents separated by newline
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("content1"));
        assert!(content_str.contains("content2"));
    }

    #[test]
    fn test_synthesize_content_summarization() {
        let node_id1: NodeID = [1u8; 32];
        let frame1 = Frame {
            frame_id: [10u8; 32],
            basis: Basis::Node(node_id1),
            content: b"content1".to_vec(),
            frame_type: "test".to_string(),
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };

        let child_frames = vec![(node_id1, frame1)];
        let content = synthesize_content(&child_frames, &SynthesisPolicy::Summarization);

        // Should contain summary information
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("Summary"));
        assert!(content_str.contains("frames"));
    }

    #[test]
    fn test_synthesize_content_filtering() {
        let node_id1: NodeID = [1u8; 32];
        let node_id2: NodeID = [2u8; 32];
        let node_id3: NodeID = [3u8; 32];

        let frame1 = Frame {
            frame_id: [10u8; 32],
            basis: Basis::Node(node_id1),
            content: b"content1".to_vec(),
            frame_type: "test".to_string(),
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };

        let frame2 = Frame {
            frame_id: [20u8; 32],
            basis: Basis::Node(node_id2),
            content: b"content2".to_vec(),
            frame_type: "test".to_string(),
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };

        let frame3 = Frame {
            frame_id: [30u8; 32],
            basis: Basis::Node(node_id3),
            content: b"content3".to_vec(),
            frame_type: "test".to_string(),
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };

        let child_frames = vec![(node_id1, frame1), (node_id2, frame2), (node_id3, frame3)];
        let policy = SynthesisPolicy::Filtering { max_frames: 2 };
        let content = synthesize_content(&child_frames, &policy);

        // Should contain only first 2 frames
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("content1"));
        assert!(content_str.contains("content2"));
        assert!(!content_str.contains("content3"));
    }
}
