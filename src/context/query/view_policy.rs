//! Context view policy: ordering and filtering for frame selection.
//! Ensures deterministic, bounded context retrieval.

use crate::context::frame::{Frame, FrameMerkleSet, FrameStorage};
use crate::error::StorageError;
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
pub fn get_context_view(
    frame_set: &FrameMerkleSet,
    frame_storage: &FrameStorage,
    policy: &ViewPolicy,
) -> Result<Vec<FrameID>, StorageError> {
    let frame_ids: Vec<FrameID> = frame_set.frame_ids().copied().collect();

    let mut frames_with_metadata: Vec<(FrameID, Frame)> = Vec::new();
    for frame_id in &frame_ids {
        if let Some(frame) = frame_storage.get(frame_id)? {
            frames_with_metadata.push((*frame_id, frame));
        }
    }

    let filtered_frames: Vec<(FrameID, Frame)> = frames_with_metadata
        .into_iter()
        .filter(|(_, frame)| {
            policy.filters.iter().all(|filter| match filter {
                FrameFilter::ByType(filter_type) => frame.frame_type == *filter_type,
                FrameFilter::ByAgent(filter_agent) => frame
                    .metadata
                    .get("agent_id")
                    .map(|a| a == filter_agent)
                    .unwrap_or(false),
            })
        })
        .collect();

    let mut sorted_frames = filtered_frames;
    match policy.ordering {
        OrderingPolicy::Recency => {
            sorted_frames.sort_by(|(_, a), (_, b)| b.timestamp.cmp(&a.timestamp));
        }
        OrderingPolicy::Type => {
            sorted_frames.sort_by(|(_, a), (_, b)| a.frame_type.cmp(&b.frame_type));
        }
        OrderingPolicy::Agent => {
            sorted_frames.sort_by(|(_, a), (_, b)| {
                let agent_a = a.metadata.get("agent_id").map(|s| s.as_str()).unwrap_or("");
                let agent_b = b.metadata.get("agent_id").map(|s| s.as_str()).unwrap_or("");
                agent_a.cmp(agent_b)
            });
        }
    }

    sorted_frames.truncate(policy.max_frames);

    Ok(sorted_frames.into_iter().map(|(frame_id, _)| frame_id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::frame::{Basis, Frame};
    use crate::types::NodeID;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_frame(
        frame_id_base: u8,
        frame_type: &str,
        agent_id: Option<&str>,
    ) -> Frame {
        let node_id: NodeID = [1u8; 32];
        let basis = Basis::Node(node_id);
        let content = format!("content_{}", frame_id_base).into_bytes();
        let metadata = HashMap::new();
        let agent_id = agent_id.unwrap_or("test-agent").to_string();
        Frame::new(basis, content, frame_type.to_string(), agent_id, metadata).unwrap()
    }

    #[test]
    fn test_filter_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();
        let frame1 = create_test_frame(1, "analysis", Some("agent1"));
        let frame2 = create_test_frame(2, "summary", Some("agent1"));
        let frame3 = create_test_frame(3, "analysis", Some("agent2"));
        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();
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
        let mut frames = Vec::new();
        for i in 0..10 {
            let frame = create_test_frame(i, "test", Some("agent1"));
            storage.store(&frame).unwrap();
            frames.push(frame);
        }
        let mut frame_set = FrameMerkleSet::new();
        for frame in &frames {
            frame_set.add_frame(frame.frame_id).unwrap();
        }
        let policy = ViewPolicy {
            max_frames: 3,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };
        let view = get_context_view(&frame_set, &storage, &policy).unwrap();
        assert_eq!(view.len(), 3);
    }

    #[test]
    fn test_ordering_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();
        let frame1 = create_test_frame(1, "zebra", None);
        let frame2 = create_test_frame(2, "alpha", None);
        let frame3 = create_test_frame(3, "beta", None);
        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();
        let policy = ViewPolicy {
            max_frames: 100,
            ordering: OrderingPolicy::Type,
            filters: vec![],
        };
        let view = get_context_view(&frame_set, &storage, &policy).unwrap();
        assert_eq!(view.len(), 3);
        let frame2_idx = view.iter().position(|&id| id == frame2.frame_id).unwrap();
        let frame3_idx = view.iter().position(|&id| id == frame3.frame_id).unwrap();
        let frame1_idx = view.iter().position(|&id| id == frame1.frame_id).unwrap();
        assert!(frame2_idx < frame3_idx);
        assert!(frame3_idx < frame1_idx);
    }

    #[test]
    fn test_deterministic_selection() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();
        let frame1 = create_test_frame(1, "test", Some("agent1"));
        let frame2 = create_test_frame(2, "test", Some("agent2"));
        let frame3 = create_test_frame(3, "test", Some("agent1"));
        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();
        let policy = ViewPolicy {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };
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
        let frame1 = create_test_frame(1, "test", Some("agent1"));
        let frame2 = create_test_frame(2, "test", Some("agent2"));
        let frame3 = create_test_frame(3, "test", Some("agent1"));
        storage.store(&frame1).unwrap();
        storage.store(&frame2).unwrap();
        storage.store(&frame3).unwrap();
        let mut frame_set = FrameMerkleSet::new();
        frame_set.add_frame(frame1.frame_id).unwrap();
        frame_set.add_frame(frame2.frame_id).unwrap();
        frame_set.add_frame(frame3.frame_id).unwrap();
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
