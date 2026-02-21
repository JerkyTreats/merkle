//! Multi-Frame Composition
//!
//! Combining multiple context frames into composite views for agent consumption.
//! Composition happens at read-time, is policy-driven, and produces bounded,
//! deterministic results. No composite state is persisted—composition is computed on-demand.

use crate::error::ApiError;
use crate::context::frame::{Frame, FrameStorage};
use crate::heads::HeadIndex;
use crate::store::NodeRecordStore;
use crate::types::NodeID;
use crate::views::{FrameFilter, OrderingPolicy};
use serde::{Deserialize, Serialize};

/// Composition source for multi-frame composition
///
/// Specifies where to collect frames from for composition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompositionSource {
    /// Current node frames
    CurrentNode,
    /// Parent directory frames
    ParentDirectory,
    /// Sibling node frames
    Siblings,
    /// Related node frames (explicit list)
    RelatedNodes(Vec<NodeID>),
}

/// Composition policy for multi-frame composition
///
/// Defines how to collect, filter, order, and select frames from multiple sources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompositionPolicy {
    /// Maximum number of frames to return
    pub max_frames: usize,
    /// Sources to collect frames from
    pub sources: Vec<CompositionSource>,
    /// Ordering policy for frame selection
    pub ordering: OrderingPolicy,
    /// Filters to apply before ordering
    pub filters: Vec<FrameFilter>,
}

impl Default for CompositionPolicy {
    fn default() -> Self {
        CompositionPolicy {
            max_frames: 100,
            sources: vec![CompositionSource::CurrentNode],
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        }
    }
}

/// Frame scoring for relevance-based ordering
///
/// Computes a relevance score for a frame. Higher scores indicate higher relevance.
/// The scoring algorithm is deterministic.
#[allow(dead_code)]
fn compute_relevance_score(
    frame: &Frame,
    target_node_id: NodeID,
    _context: &CompositionContext,
) -> i64 {
    // Simple relevance scoring algorithm (deterministic)
    // Higher score = more relevant

    let mut score = 0i64;

    // Boost score if frame is directly associated with target node
    match &frame.basis {
        crate::context::frame::Basis::Node(node_id) => {
            if *node_id == target_node_id {
                score += 1000;
            }
        }
        crate::context::frame::Basis::Frame(_) => {
            // Frame-based basis gets medium score
            score += 500;
        }
        crate::context::frame::Basis::Both { node, .. } => {
            if *node == target_node_id {
                score += 1000;
            }
        }
    }

    // Boost score based on frame type (some types are more relevant)
    match frame.frame_type.as_str() {
        "analysis" | "summary" => score += 100,
        "documentation" => score += 50,
        _ => {}
    }

    // Boost score based on recency (more recent = higher score)
    // Use timestamp as a component (newer timestamps are larger)
    // We'll use a simple approach: score += timestamp seconds since epoch / 1000
    // This gives a small boost to newer frames
    if let Ok(duration) = frame.timestamp.duration_since(std::time::UNIX_EPOCH) {
        score += (duration.as_secs() / 1000) as i64;
    }

    score
}

/// Context for composition operations
///
/// Contains information needed for composition decisions.
#[derive(Debug, Clone)]
struct CompositionContext {
    #[allow(dead_code)]
    target_node_id: NodeID,
    parent_node_id: Option<NodeID>,
    sibling_node_ids: Vec<NodeID>,
}

/// Collect frames from a composition source
///
/// Retrieves frames from the specified source for the given node.
fn collect_frames_from_source(
    source: &CompositionSource,
    target_node_id: NodeID,
    context: &CompositionContext,
    _node_store: &dyn NodeRecordStore,
    frame_storage: &FrameStorage,
    head_index: &HeadIndex,
) -> Result<Vec<(NodeID, Frame)>, ApiError> {
    let mut frames = Vec::new();

    match source {
        CompositionSource::CurrentNode => {
            // Get all frame types for current node
            let frame_ids = head_index.get_all_heads_for_node(&target_node_id);
            for frame_id in frame_ids {
                if let Some(frame) = frame_storage.get(&frame_id).map_err(ApiError::from)? {
                    frames.push((target_node_id, frame));
                }
            }
        }
        CompositionSource::ParentDirectory => {
            if let Some(parent_id) = context.parent_node_id {
                let frame_ids = head_index.get_all_heads_for_node(&parent_id);
                for frame_id in frame_ids {
                    if let Some(frame) = frame_storage.get(&frame_id).map_err(ApiError::from)? {
                        frames.push((parent_id, frame));
                    }
                }
            }
        }
        CompositionSource::Siblings => {
            for sibling_id in &context.sibling_node_ids {
                let frame_ids = head_index.get_all_heads_for_node(sibling_id);
                for frame_id in frame_ids {
                    if let Some(frame) = frame_storage.get(&frame_id).map_err(ApiError::from)? {
                        frames.push((*sibling_id, frame));
                    }
                }
            }
        }
        CompositionSource::RelatedNodes(related_ids) => {
            for related_id in related_ids {
                let frame_ids = head_index.get_all_heads_for_node(related_id);
                for frame_id in frame_ids {
                    if let Some(frame) = frame_storage.get(&frame_id).map_err(ApiError::from)? {
                        frames.push((*related_id, frame));
                    }
                }
            }
        }
    }

    Ok(frames)
}

/// Compose frames from multiple sources
///
/// Collects frames from multiple sources, applies filters, orders them according to policy,
/// and returns a bounded list of frames.
///
/// # Arguments
/// * `target_node_id` - The node to compose context for
/// * `policy` - Composition policy specifying sources, filters, ordering, and bounds
/// * `node_store` - Node record store for accessing node relationships
/// * `frame_storage` - Frame storage for retrieving frames
/// * `head_index` - Head index for O(1) head resolution
///
/// # Returns
/// * `Vec<Frame>` - Composed frames in policy-determined order
/// * `ApiError` - Error if composition fails
///
/// # Behavior
/// * Read-only: Never triggers writes
/// * Deterministic: Same inputs → same outputs
/// * Bounded: Never exceeds max_frames
/// * Graceful: Missing frames are skipped, not errors
pub fn compose_frames(
    target_node_id: NodeID,
    policy: &CompositionPolicy,
    node_store: &dyn NodeRecordStore,
    frame_storage: &FrameStorage,
    head_index: &HeadIndex,
) -> Result<Vec<Frame>, ApiError> {
    // Build composition context
    let target_record = node_store
        .get(&target_node_id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NodeNotFound(target_node_id))?;

    let parent_node_id = target_record.parent;

    // Get sibling node IDs from parent
    let sibling_node_ids = if let Some(parent_id) = parent_node_id {
        if let Some(parent_record) = node_store.get(&parent_id).map_err(ApiError::from)? {
            parent_record
                .children
                .iter()
                .filter(|&&child_id| child_id != target_node_id)
                .copied()
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let context = CompositionContext {
        target_node_id,
        parent_node_id,
        sibling_node_ids,
    };

    // Step 1: Collect candidate frames from all sources
    let mut candidate_frames: Vec<(NodeID, Frame)> = Vec::new();
    for source in &policy.sources {
        let source_frames = collect_frames_from_source(
            source,
            target_node_id,
            &context,
            node_store,
            frame_storage,
            head_index,
        )?;
        candidate_frames.extend(source_frames);
    }

    // Deduplicate frames by FrameID (same frame might appear from multiple sources)
    let mut seen_frame_ids = std::collections::HashSet::new();
    candidate_frames.retain(|(_, frame)| {
        if seen_frame_ids.contains(&frame.frame_id) {
            false
        } else {
            seen_frame_ids.insert(frame.frame_id);
            true
        }
    });

    // Step 2: Apply filters
    let filtered_frames: Vec<(NodeID, Frame)> = candidate_frames
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

    // Step 3: Score and order frames (policy-driven)
    let mut scored_frames: Vec<(i64, NodeID, Frame)> = filtered_frames
        .into_iter()
        .map(|(node_id, frame)| {
            let score = match policy.ordering {
                OrderingPolicy::Recency => {
                    // Use timestamp as score (newer = higher)
                    frame
                        .timestamp
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0)
                }
                OrderingPolicy::Type => {
                    // Use frame type hash as score (deterministic)
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    frame.frame_type.hash(&mut hasher);
                    hasher.finish() as i64
                }
                OrderingPolicy::Agent => {
                    // Use agent ID hash as score (deterministic)
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    frame
                        .metadata
                        .get("agent_id")
                        .unwrap_or(&String::new())
                        .hash(&mut hasher);
                    hasher.finish() as i64
                }
            };
            (score, node_id, frame)
        })
        .collect();

    // Sort by score (descending for Recency, ascending for Type/Agent to maintain lexicographic order)
    match policy.ordering {
        OrderingPolicy::Recency => {
            scored_frames.sort_by(|(score_a, _, _), (score_b, _, _)| score_b.cmp(score_a));
        }
        OrderingPolicy::Type | OrderingPolicy::Agent => {
            // For Type and Agent, we want lexicographic order, so we sort by the actual values
            // after sorting by score. But since we're using hash, we'll just sort by score.
            // Actually, let's sort by the actual values for Type and Agent
            scored_frames.sort_by(|(_, _, frame_a), (_, _, frame_b)| match policy.ordering {
                OrderingPolicy::Type => frame_a.frame_type.cmp(&frame_b.frame_type),
                OrderingPolicy::Agent => {
                    let agent_a = frame_a
                        .metadata
                        .get("agent_id")
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let agent_b = frame_b
                        .metadata
                        .get("agent_id")
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    agent_a.cmp(agent_b)
                }
                _ => unreachable!(),
            });
        }
    }

    // Step 4: Select top N frames (bounded by max_frames)
    scored_frames.truncate(policy.max_frames);

    // Step 5: Extract frames
    Ok(scored_frames
        .into_iter()
        .map(|(_, _, frame)| frame)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::frame::storage::FrameStorage;
    use crate::context::frame::{Basis, Frame};
    use crate::heads::HeadIndex;
    use crate::store::{NodeRecord, NodeRecordStore, NodeType, SledNodeRecordStore};
    use crate::types::FrameID;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_node_record(
        node_id: NodeID,
        parent: Option<NodeID>,
        children: Vec<NodeID>,
    ) -> NodeRecord {
        NodeRecord {
            node_id,
            path: PathBuf::from(format!("/test/node_{:02x}", node_id[0])),
            node_type: NodeType::File {
                size: 100,
                content_hash: [0u8; 32],
            },
            children,
            parent,
            frame_set_root: None,
            metadata: HashMap::new(),
            tombstoned_at: None,
        }
    }

    #[test]
    fn test_compose_current_node_only() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let mut head_index = HeadIndex::new();

        let node_id: NodeID = [1u8; 32];
        let node_record = create_test_node_record(node_id, None, vec![]);
        node_store.put(&node_record).unwrap();

        // Create frames for the node with different types
        let frame1 = Frame::new(
            Basis::Node(node_id),
            b"content1".to_vec(),
            "type1".to_string(),
            "agent1".to_string(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            Basis::Node(node_id),
            b"content2".to_vec(),
            "type2".to_string(),
            "agent2".to_string(),
            HashMap::new(),
        )
        .unwrap();

        frame_storage.store(&frame1).unwrap();
        frame_storage.store(&frame2).unwrap();

        head_index
            .update_head(&node_id, "type1", &frame1.frame_id)
            .unwrap();
        head_index
            .update_head(&node_id, "type2", &frame2.frame_id)
            .unwrap();

        let policy = CompositionPolicy {
            max_frames: 10,
            sources: vec![CompositionSource::CurrentNode],
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let composed = compose_frames(
            node_id,
            &policy,
            node_store.as_ref(),
            &frame_storage,
            &head_index,
        )
        .unwrap();

        assert_eq!(composed.len(), 2);
    }

    #[test]
    fn test_compose_bounded_output() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let mut head_index = HeadIndex::new();

        let node_id: NodeID = [1u8; 32];
        let node_record = create_test_node_record(node_id, None, vec![]);
        node_store.put(&node_record).unwrap();

        // Create many frames with different types
        for i in 0..20 {
            let frame = Frame::new(
                Basis::Node(node_id),
                format!("content{}", i).into_bytes(),
                format!("type{}", i),
                "agent1".to_string(),
                HashMap::new(),
            )
            .unwrap();
            frame_storage.store(&frame).unwrap();
            head_index
                .update_head(&node_id, &format!("type{}", i), &frame.frame_id)
                .unwrap();
        }

        let policy = CompositionPolicy {
            max_frames: 5,
            sources: vec![CompositionSource::CurrentNode],
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let composed = compose_frames(
            node_id,
            &policy,
            node_store.as_ref(),
            &frame_storage,
            &head_index,
        )
        .unwrap();

        assert_eq!(composed.len(), 5);
        assert!(composed.len() <= policy.max_frames);
    }

    #[test]
    fn test_compose_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let mut head_index = HeadIndex::new();

        let node_id: NodeID = [1u8; 32];
        let node_record = create_test_node_record(node_id, None, vec![]);
        node_store.put(&node_record).unwrap();

        let frame1 = Frame::new(
            Basis::Node(node_id),
            b"content1".to_vec(),
            "test".to_string(),
            "agent1".to_string(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            Basis::Node(node_id),
            b"content2".to_vec(),
            "test".to_string(),
            "agent2".to_string(),
            HashMap::new(),
        )
        .unwrap();

        frame_storage.store(&frame1).unwrap();
        frame_storage.store(&frame2).unwrap();

        head_index
            .update_head(&node_id, "test", &frame1.frame_id)
            .unwrap();
        head_index
            .update_head(&node_id, "test", &frame2.frame_id)
            .unwrap();

        let policy = CompositionPolicy {
            max_frames: 10,
            sources: vec![CompositionSource::CurrentNode],
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let composed1 = compose_frames(
            node_id,
            &policy,
            node_store.as_ref(),
            &frame_storage,
            &head_index,
        )
        .unwrap();

        let composed2 = compose_frames(
            node_id,
            &policy,
            node_store.as_ref(),
            &frame_storage,
            &head_index,
        )
        .unwrap();

        assert_eq!(composed1.len(), composed2.len());
        assert_eq!(
            composed1.iter().map(|f| f.frame_id).collect::<Vec<_>>(),
            composed2.iter().map(|f| f.frame_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_compose_empty_result() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let head_index = HeadIndex::new();

        let node_id: NodeID = [1u8; 32];
        let node_record = create_test_node_record(node_id, None, vec![]);
        node_store.put(&node_record).unwrap();

        let policy = CompositionPolicy {
            max_frames: 10,
            sources: vec![CompositionSource::CurrentNode],
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let composed = compose_frames(
            node_id,
            &policy,
            node_store.as_ref(),
            &frame_storage,
            &head_index,
        )
        .unwrap();

        assert!(composed.is_empty());
    }

    #[test]
    fn test_compose_multi_source() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let mut head_index = HeadIndex::new();

        // Create parent and child nodes
        let parent_id: NodeID = [1u8; 32];
        let child_id: NodeID = [2u8; 32];

        let parent_record = create_test_node_record(parent_id, None, vec![child_id]);
        let child_record = create_test_node_record(child_id, Some(parent_id), vec![]);

        node_store.put(&parent_record).unwrap();
        node_store.put(&child_record).unwrap();

        // Create frames for both
        let parent_frame = Frame::new(
            Basis::Node(parent_id),
            b"parent content".to_vec(),
            "test".to_string(),
            "agent1".to_string(),
            HashMap::new(),
        )
        .unwrap();

        let child_frame = Frame::new(
            Basis::Node(child_id),
            b"child content".to_vec(),
            "test".to_string(),
            "agent1".to_string(),
            HashMap::new(),
        )
        .unwrap();

        frame_storage.store(&parent_frame).unwrap();
        frame_storage.store(&child_frame).unwrap();

        head_index
            .update_head(&parent_id, "test", &parent_frame.frame_id)
            .unwrap();
        head_index
            .update_head(&child_id, "test", &child_frame.frame_id)
            .unwrap();

        // Compose from current node and parent
        let policy = CompositionPolicy {
            max_frames: 10,
            sources: vec![
                CompositionSource::CurrentNode,
                CompositionSource::ParentDirectory,
            ],
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let composed = compose_frames(
            child_id,
            &policy,
            node_store.as_ref(),
            &frame_storage,
            &head_index,
        )
        .unwrap();

        assert_eq!(composed.len(), 2);
        let frame_ids: Vec<FrameID> = composed.iter().map(|f| f.frame_id).collect();
        assert!(frame_ids.contains(&child_frame.frame_id));
        assert!(frame_ids.contains(&parent_frame.frame_id));
    }
}
