//! Context query service: get_node by view policy.
//! Owns frame selection and retrieval; used by api as the single owner of query behavior.

use crate::context::frame::{Frame, FrameMerkleSet, FrameStorage};
use crate::context::query::view_policy::{get_context_view, ViewPolicy};
use crate::error::ApiError;
use crate::store::{NodeRecord, NodeRecordStore};
use crate::types::{FrameID, NodeID};
use tracing::warn;

/// Query implementation: get node record and selected frames by view policy.
/// Caller supplies frame_ids for the node (e.g. from head index) and builds NodeContext from the result.
pub fn get_node(
    node_store: &dyn NodeRecordStore,
    frame_storage: &FrameStorage,
    frame_ids: &[FrameID],
    node_id: NodeID,
    view_policy: &ViewPolicy,
) -> Result<(NodeRecord, Vec<Frame>, usize), ApiError> {
    let node_record = node_store.get(&node_id).map_err(ApiError::from)?.ok_or_else(|| {
        warn!("Node not found");
        ApiError::NodeNotFound(node_id)
    })?;
    if node_record.tombstoned_at.is_some() {
        return Err(ApiError::NodeNotFound(node_id));
    }

    if frame_ids.is_empty() {
        return Ok((node_record, vec![], 0));
    }

    let frame_set = FrameMerkleSet::from_frame_ids(frame_ids.iter().copied())
        .map_err(ApiError::StorageError)?;

    let selected_frame_ids = get_context_view(&frame_set, frame_storage, view_policy)
        .map_err(ApiError::StorageError)?;

    let mut frames = Vec::new();
    for frame_id in selected_frame_ids {
        if let Some(frame) = frame_storage.get(&frame_id).map_err(ApiError::from)? {
            frames.push(frame);
        }
    }

    let total_frame_count = frame_set.len();
    Ok((node_record, frames, total_frame_count))
}
