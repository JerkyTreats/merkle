//! Context Views
//!
//! Selects and orders a bounded set of frames based on policies
//! (recency, type, agent). Ensures deterministic, bounded context retrieval.

use crate::error::StorageError;
use crate::frame::{Frame, FrameMerkleSet, FrameStorage};
use crate::types::FrameID;
use serde::{Deserialize, Serialize};

/// Ordering policy for frame selection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderingPolicy {
    /// Order by recency (most recent first, by timestamp)
    Recency,
    /// Order by frame type (lexicographic)
    Type,
    /// Order by agent ID (lexicographic)
    Agent,
}

/// Frame filter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FrameFilter {
    /// Filter frames by type
    ByType(String),
    /// Filter frames by agent ID
    ByAgent(String),
}

/// Context view policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewPolicy {
    /// Maximum number of frames to return
    pub max_frames: usize,
    /// Ordering policy for frame selection
    pub ordering: OrderingPolicy,
    /// Filters to apply before ordering
    pub filters: Vec<FrameFilter>,
}

/// Get context view for a node given a frame set
///
/// This function takes a FrameMerkleSet directly. In a full implementation,
/// this would be retrieved from storage based on the node_id.
///
/// The function:
/// 1. Retrieves all frames from the frame set
/// 2. Applies filters (by type, agent)
/// 3. Orders frames according to the ordering policy
/// 4. Limits results to max_frames
/// 5. Returns FrameIDs in deterministic order
pub fn get_context_view(
    frame_set: &FrameMerkleSet,
    frame_storage: &FrameStorage,
    policy: &ViewPolicy,
) -> Result<Vec<FrameID>, StorageError> {
    // Step 1: Collect all FrameIDs from the set
    let frame_ids: Vec<FrameID> = frame_set.frame_ids().copied().collect();

    // Step 2: Retrieve frames to access metadata for filtering and ordering
    let mut frames_with_metadata: Vec<(FrameID, Frame)> = Vec::new();
    for frame_id in &frame_ids {
        if let Some(frame) = frame_storage.get(frame_id)? {
            frames_with_metadata.push((*frame_id, frame));
        }
        // If frame not found in storage, skip it (might be corrupted or missing)
    }

    // Step 3: Apply filters
    let filtered_frames: Vec<(FrameID, Frame)> = frames_with_metadata
        .into_iter()
        .filter(|(_, frame)| {
            policy.filters.iter().all(|filter| match filter {
                FrameFilter::ByType(filter_type) => frame.frame_type == *filter_type,
                FrameFilter::ByAgent(filter_agent) => {
                    // Agent ID might be in metadata
                    frame
                        .metadata
                        .get("agent_id")
                        .map(|a| a == filter_agent)
                        .unwrap_or(false)
                }
            })
        })
        .collect();

    // Step 4: Sort by ordering policy
    let mut sorted_frames = filtered_frames;
    match policy.ordering {
        OrderingPolicy::Recency => {
            // Sort by timestamp (most recent first)
            sorted_frames.sort_by(|(_, a), (_, b)| {
                b.timestamp.cmp(&a.timestamp) // Reverse order: newest first
            });
        }
        OrderingPolicy::Type => {
            // Sort by frame type (lexicographic)
            sorted_frames.sort_by(|(_, a), (_, b)| a.frame_type.cmp(&b.frame_type));
        }
        OrderingPolicy::Agent => {
            // Sort by agent ID (from metadata, lexicographic)
            sorted_frames.sort_by(|(_, a), (_, b)| {
                let agent_a = a.metadata.get("agent_id").map(|s| s.as_str()).unwrap_or("");
                let agent_b = b.metadata.get("agent_id").map(|s| s.as_str()).unwrap_or("");
                agent_a.cmp(agent_b)
            });
        }
    }

    // Step 5: Apply max_frames limit
    sorted_frames.truncate(policy.max_frames);

    // Step 6: Extract FrameIDs
    Ok(sorted_frames
        .into_iter()
        .map(|(frame_id, _)| frame_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Basis, Frame};
    use crate::types::NodeID;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_frame(
        frame_id_base: u8,
        frame_type: &str,
        agent_id: Option<&str>,
        _timestamp_offset: u64,
    ) -> Frame {
        let node_id: NodeID = [1u8; 32];
        let basis = Basis::Node(node_id);
        let content = format!("content_{}", frame_id_base).into_bytes();
        let metadata = HashMap::new();

        // Use provided agent_id or default to "test-agent"
        let agent_id = agent_id.unwrap_or("test-agent").to_string();

        let frame = Frame::new(basis, content, frame_type.to_string(), agent_id, metadata).unwrap();

        // In real implementation, timestamps come from SystemTime::now()
        // For testing, we'll use the frame as-is since we can't easily mock SystemTime
        frame
    }

    #[test]
    fn test_filter_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create frames with different types
        let frame1 = create_test_frame(1, "analysis", Some("agent1"), 0);
        let frame2 = create_test_frame(2, "summary", Some("agent1"), 0);
        let frame3 = create_test_frame(3, "analysis", Some("agent2"), 0);

        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();

        // Create frame set
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();

        // Filter by type "analysis"
        let policy = ViewPolicy {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![FrameFilter::ByType("analysis".to_string())],
        };

        let view = get_context_view(&frame_set, &storage, &policy).unwrap();
        assert_eq!(view.len(), 2);
        assert!(view.contains(&frame1.frame_id));
        assert!(view.contains(&frame3.frame_id));
        assert!(!view.contains(&frame2.frame_id));
    }

    #[test]
    fn test_max_frames_limit() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create multiple frames
        let mut frames = Vec::new();
        for i in 0..10 {
            let frame = create_test_frame(i, "test", Some("agent1"), 0);
            storage.store(&frame).unwrap();
            frames.push(frame);
        }

        // Create frame set
        let mut frame_set = FrameMerkleSet::new();
        for frame in &frames {
            frame_set.add_frame(frame.frame_id).unwrap();
        }

        // Request only 3 frames
        let policy = ViewPolicy {
            max_frames: 3,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let view = get_context_view(&frame_set, &storage, &policy).unwrap();
        assert_eq!(view.len(), 3);
        assert!(view.len() <= policy.max_frames);
    }

    #[test]
    fn test_ordering_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create frames with different types
        let frame1 = create_test_frame(1, "zebra", None, 0);
        let frame2 = create_test_frame(2, "alpha", None, 0);
        let frame3 = create_test_frame(3, "beta", None, 0);

        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();

        // Create frame set
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();

        // Order by type
        let policy = ViewPolicy {
            max_frames: 100,
            ordering: OrderingPolicy::Type,
            filters: vec![],
        };

        let view = get_context_view(&frame_set, &storage, &policy).unwrap();
        assert_eq!(view.len(), 3);

        // Should be ordered: alpha, beta, zebra
        // Verify ordering by checking frame types
        let frame2_idx = view.iter().position(|&id| id == frame2.frame_id).unwrap();
        let frame3_idx = view.iter().position(|&id| id == frame3.frame_id).unwrap();
        let frame1_idx = view.iter().position(|&id| id == frame1.frame_id).unwrap();

        assert!(frame2_idx < frame3_idx); // alpha before beta
        assert!(frame3_idx < frame1_idx); // beta before zebra
    }

    #[test]
    fn test_deterministic_selection() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create frames
        let frame1 = create_test_frame(1, "test", Some("agent1"), 0);
        let frame2 = create_test_frame(2, "test", Some("agent2"), 0);
        let frame3 = create_test_frame(3, "test", Some("agent1"), 0);

        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();

        // Create frame set
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();

        let policy = ViewPolicy {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        // Get view twice - should be identical
        let view1 = get_context_view(&frame_set, &storage, &policy).unwrap();
        let view2 = get_context_view(&frame_set, &storage, &policy).unwrap();

        assert_eq!(view1, view2);
    }

    #[test]
    fn test_empty_view() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();
        let frame_set = FrameMerkleSet::new();

        let policy = ViewPolicy {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let view = get_context_view(&frame_set, &storage, &policy).unwrap();
        assert!(view.is_empty());
    }

    #[test]
    fn test_filter_by_agent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create frames with different agents
        let frame1 = create_test_frame(1, "test", Some("agent1"), 0);
        let frame2 = create_test_frame(2, "test", Some("agent2"), 0);
        let frame3 = create_test_frame(3, "test", Some("agent1"), 0);

        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();

        // Create frame set
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();

        // Filter by agent
        let policy = ViewPolicy {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![FrameFilter::ByAgent("agent1".to_string())],
        };

        let view = get_context_view(&frame_set, &storage, &policy).unwrap();
        assert_eq!(view.len(), 2);
        assert!(view.contains(&frame1.frame_id));
        assert!(view.contains(&frame3.frame_id));
        assert!(!view.contains(&frame2.frame_id));
    }
}
