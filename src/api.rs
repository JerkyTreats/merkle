//! Core Context APIs
//!
//! Provides minimal, stateless API surface for agent interaction with the context engine.
//! Implements GetNode and PutFrame APIs as specified in Phase 2B.

use crate::agent::AgentRegistry;
use crate::context::query::{compose_frames, CompositionPolicy};
use crate::concurrency::NodeLockManager;
use crate::context::frame::{Basis, Frame, FrameStorage};
use crate::context::query::get_node_query;
use crate::context::queue::FrameGenerationQueue;
use crate::error::ApiError;
use crate::heads::HeadIndex;
use crate::store::NodeRecordStore;
use crate::types::{FrameID, NodeID};
use crate::views::ViewPolicy;
use hex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, instrument, warn};

pub use crate::context::query::view::{ContextView, ContextViewBuilder, NodeContext};
pub use crate::context::types::{CompactResult, RestoreResult, TombstoneResult};

/// Context API service
///
/// Provides GetNode and PutFrame APIs with proper authorization,
/// concurrency control, and error handling.
pub struct ContextApi {
    /// Node record store for fast node lookups
    node_store: Arc<dyn NodeRecordStore + Send + Sync>,
    /// Frame storage for content-addressed frame storage
    frame_storage: Arc<FrameStorage>,
    /// Head index for O(1) head resolution
    head_index: Arc<parking_lot::RwLock<HeadIndex>>,
    /// Agent registry for authorization
    agent_registry: Arc<parking_lot::RwLock<AgentRegistry>>,
    /// Provider registry for LLM provider management
    provider_registry: Arc<parking_lot::RwLock<crate::provider::ProviderRegistry>>,
    /// Lock manager for concurrent access safety
    lock_manager: Arc<NodeLockManager>,
    /// Workspace root for persistence (optional)
    workspace_root: Option<PathBuf>,
}

impl ContextApi {
    /// Create a new Context API service
    pub fn new(
        node_store: Arc<dyn NodeRecordStore + Send + Sync>,
        frame_storage: Arc<FrameStorage>,
        head_index: Arc<parking_lot::RwLock<HeadIndex>>,
        agent_registry: Arc<parking_lot::RwLock<AgentRegistry>>,
        provider_registry: Arc<parking_lot::RwLock<crate::provider::ProviderRegistry>>,
        lock_manager: Arc<NodeLockManager>,
    ) -> Self {
        Self {
            node_store,
            frame_storage,
            head_index,
            agent_registry,
            provider_registry,
            lock_manager,
            workspace_root: None,
        }
    }

    /// Create a new Context API service with workspace root for persistence
    pub fn with_workspace_root(
        node_store: Arc<dyn NodeRecordStore + Send + Sync>,
        frame_storage: Arc<FrameStorage>,
        head_index: Arc<parking_lot::RwLock<HeadIndex>>,
        agent_registry: Arc<parking_lot::RwLock<AgentRegistry>>,
        provider_registry: Arc<parking_lot::RwLock<crate::provider::ProviderRegistry>>,
        lock_manager: Arc<NodeLockManager>,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            node_store,
            frame_storage,
            head_index,
            agent_registry,
            provider_registry,
            lock_manager,
            workspace_root: Some(workspace_root),
        }
    }

    /// Persist indices to disk if workspace root is configured
    fn persist_indices(&self) -> Result<(), ApiError> {
        if let Some(ref workspace_root) = self.workspace_root {
            // Persist head index
            {
                let head_index = self.head_index.read();
                let path = HeadIndex::persistence_path(workspace_root);
                head_index
                    .save_to_disk(&path)
                    .map_err(|e| ApiError::StorageError(e))?;
            }
        }
        Ok(())
    }

    /// Get node context using policy-driven view
    ///
    /// Retrieves the node record and selected frames based on the context view policy.
    /// This is a read-only operation that never triggers writes.
    ///
    /// # Arguments
    /// * `node_id` - The NodeID to retrieve context for
    /// * `view` - ContextView policy specifying frame selection and ordering
    ///
    /// # Returns
    /// * `NodeContext` - Node record plus selected frames
    /// * `ApiError` - Error if node not found or invalid request
    ///
    /// # Behavior
    /// * Deterministic: Same inputs → same outputs
    /// * Read-only: Never triggers writes
    /// * Bounded: Frame count limited by view policy
    #[instrument(skip(self), fields(node_id = %hex::encode(node_id)))]
    pub fn get_node(&self, node_id: NodeID, view: ContextView) -> Result<NodeContext, ApiError> {
        let start = Instant::now();
        debug!("Retrieving node context");

        let frame_ids = {
            let head_index = self.head_index.read();
            head_index.get_all_heads_for_node(&node_id)
        };

        let view_policy: ViewPolicy = view.into();

        let (node_record, frames, total_frame_count) = get_node_query(
            self.node_store.as_ref(),
            &self.frame_storage,
            &frame_ids,
            node_id,
            &view_policy,
        )?;

        let duration = start.elapsed();
        debug!(
            frame_count = frames.len(),
            total_frames = total_frame_count,
            duration_ms = duration.as_millis(),
            "Node context retrieved"
        );

        Ok(NodeContext {
            node_id,
            node_record,
            frames,
            frame_count: total_frame_count,
        })
    }

    /// Put frame: Append new frame to node's frame set
    ///
    /// Creates a new frame and appends it to the node's frame set.
    /// This is an append-only operation that never mutates existing frames.
    ///
    /// # Arguments
    /// * `node_id` - The NodeID to attach frame to
    /// * `frame` - Frame content (basis, content, metadata)
    /// * `agent_id` - Identity of agent creating the frame
    ///
    /// # Returns
    /// * `FrameID` - The generated FrameID for the new frame
    /// * `ApiError` - Error if node not found, agent unauthorized, or invalid frame
    ///
    /// # Behavior
    /// * Append-only: Creates new frame, never mutates existing
    /// * Atomic: Frame creation and head update are transactional
    /// * Deterministic: Same inputs → same FrameID
    #[instrument(skip(self), fields(node_id = %hex::encode(node_id), agent_id = %agent_id, frame_type = %frame.frame_type))]
    pub fn put_frame(
        &self,
        node_id: NodeID,
        frame: Frame,
        agent_id: String,
    ) -> Result<FrameID, ApiError> {
        let start = Instant::now();
        debug!("Creating frame");

        // Verify agent exists and has write permission
        let agent = {
            let registry = self.agent_registry.read();
            registry.get_or_error(&agent_id)?.clone() // Clone to release lock
        };

        // Verify agent can write
        agent.verify_write()?;

        // Verify node exists and is not tombstoned
        let _node_record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
        if _node_record.tombstoned_at.is_some() {
            return Err(ApiError::NodeNotFound(node_id));
        }

        // Verify frame basis matches node_id (if basis is Node-based)
        match &frame.basis {
            crate::context::frame::Basis::Node(basis_node_id) => {
                if *basis_node_id != node_id {
                    return Err(ApiError::InvalidFrame(format!(
                        "Frame basis node_id {:?} does not match requested node_id {:?}",
                        basis_node_id, node_id
                    )));
                }
            }
            crate::context::frame::Basis::Frame(_) => {
                // Frame-based basis is OK (can reference other frames)
            }
            crate::context::frame::Basis::Both { node, .. } => {
                if *node != node_id {
                    return Err(ApiError::InvalidFrame(format!(
                        "Frame basis node_id {:?} does not match requested node_id {:?}",
                        node, node_id
                    )));
                }
            }
        }

        // Verify agent_id in frame metadata matches provided agent_id
        if let Some(frame_agent_id) = frame.metadata.get("agent_id") {
            if frame_agent_id != &agent_id {
                return Err(ApiError::InvalidFrame(format!(
                    "Frame metadata agent_id '{}' does not match provided agent_id '{}'",
                    frame_agent_id, agent_id
                )));
            }
        } else {
            return Err(ApiError::InvalidFrame(
                "Frame missing agent_id in metadata".to_string(),
            ));
        }

        // Acquire write lock for this node (atomic operation)
        let lock = self.lock_manager.get_lock(&node_id);
        let _guard = lock.write();

        // Store frame
        self.frame_storage.store(&frame).map_err(ApiError::from)?;

        // Update frame set (get or create)
        // TODO: In a full implementation, we'd retrieve the FrameMerkleSet from storage
        // and update it. For Phase 2B MVP, we'll track frame sets in memory.
        // For now, we'll just update the head index.

        // Update head index.
        {
            let mut head_index = self.head_index.write();
            head_index
                .update_head(&node_id, &frame.frame_type, &frame.frame_id)
                .map_err(ApiError::from)?;
        }

        // Persist indices to disk
        self.persist_indices()?;

        // TODO: Update node record's frame_set_root
        // This requires retrieving/updating the FrameMerkleSet and storing it.
        // For Phase 2B MVP, we'll skip this and rely on head index.

        let duration = start.elapsed();
        info!(
            frame_id = %hex::encode(frame.frame_id),
            duration_ms = duration.as_millis(),
            "Frame created"
        );

        Ok(frame.frame_id)
    }

    /// Collect node_id and all descendant node IDs (BFS from record.children).
    pub fn collect_subtree_node_ids(&self, node_id: NodeID) -> Result<HashSet<NodeID>, ApiError> {
        let mut set = HashSet::new();
        set.insert(node_id);
        let mut queue = VecDeque::new();
        let record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
        for &child_id in &record.children {
            queue.push_back(child_id);
        }
        while let Some(nid) = queue.pop_front() {
            if !set.insert(nid) {
                continue;
            }
            if let Some(rec) = self.node_store.get(&nid).map_err(ApiError::from)? {
                for &child_id in &rec.children {
                    queue.push_back(child_id);
                }
            }
        }
        Ok(set)
    }

    /// Tombstone a node and all descendants. Marks records in node store and head index.
    /// Frame blobs are not affected.
    pub fn tombstone_node(&self, node_id: NodeID) -> Result<TombstoneResult, ApiError> {
        let record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
        if record.tombstoned_at.is_some() {
            return Ok(TombstoneResult {
                nodes_tombstoned: 0,
                head_entries_tombstoned: 0,
            });
        }
        let to_tombstone = self.collect_subtree_node_ids(node_id)?;
        let mut nodes_tombstoned = 0u64;
        let mut head_entries_tombstoned = 0u64;
        for &nid in &to_tombstone {
            self.node_store.tombstone(&nid).map_err(ApiError::from)?;
            nodes_tombstoned += 1;
            let mut head_index = self.head_index.write();
            let before = head_index.get_all_heads_for_node(&nid).len();
            head_index.tombstone_heads_for_node(&nid);
            head_entries_tombstoned += before as u64;
        }
        self.persist_indices()?;
        Ok(TombstoneResult {
            nodes_tombstoned,
            head_entries_tombstoned,
        })
    }

    /// Restore a tombstoned node and all descendants.
    pub fn restore_node(&self, node_id: NodeID) -> Result<RestoreResult, ApiError> {
        let record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
        if record.tombstoned_at.is_none() {
            return Ok(RestoreResult {
                nodes_restored: 0,
                head_entries_restored: 0,
            });
        }
        let to_restore = self.collect_subtree_node_ids(node_id)?;
        let mut nodes_restored = 0u64;
        let mut head_entries_restored = 0u64;
        for &nid in &to_restore {
            self.node_store.restore(&nid).map_err(ApiError::from)?;
            nodes_restored += 1;
            let mut head_index = self.head_index.write();
            let before = head_index.get_all_heads_for_node(&nid).len();
            head_index.restore_heads_for_node(&nid);
            head_entries_restored += before as u64;
        }
        self.persist_indices()?;
        Ok(RestoreResult {
            nodes_restored,
            head_entries_restored,
        })
    }

    /// Compact tombstoned records older than TTL. Optionally purge frame blobs.
    pub fn compact(&self, ttl_seconds: u64, purge_frames: bool) -> Result<CompactResult, ApiError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ApiError::ConfigError(e.to_string()))?
            .as_secs();
        let cutoff = now.saturating_sub(ttl_seconds);
        let node_ids = self
            .node_store
            .list_tombstoned(Some(cutoff))
            .map_err(ApiError::from)?;
        let mut nodes_purged = 0u64;
        let mut frames_purged = 0u64;
        for &nid in &node_ids {
            if purge_frames {
                let frame_ids = self.head_index.read().get_all_heads_for_node(&nid);
                for frame_id in frame_ids {
                    self.frame_storage
                        .purge(&frame_id)
                        .map_err(ApiError::from)?;
                    frames_purged += 1;
                }
            }
            self.node_store
                .purge(&nid, cutoff)
                .map_err(ApiError::from)?;
            nodes_purged += 1;
        }
        let head_before = self.head_index.read().heads.len();
        self.head_index.write().purge_tombstoned(cutoff);
        let head_after = self.head_index.read().heads.len();
        let head_entries_purged = (head_before - head_after) as u64;
        self.persist_indices()?;
        Ok(CompactResult {
            nodes_purged,
            head_entries_purged,
            frames_purged,
        })
    }

    /// Compose frames from multiple sources
    ///
    /// Combines context frames from multiple sources (current node, parent, siblings, related)
    /// into a composite view. Composition is read-time only, policy-driven, and produces
    /// bounded, deterministic results.
    ///
    /// # Arguments
    /// * `node_id` - NodeID to compose context for
    /// * `policy` - Composition policy specifying sources, filters, ordering, and bounds
    ///
    /// # Returns
    /// * `Vec<Frame>` - Composed frames in policy-determined order
    /// * `ApiError` - Error if node not found or composition fails
    ///
    /// # Behavior
    /// * Read-only: Never triggers writes
    /// * Deterministic: Same inputs → same outputs
    /// * Bounded: Never exceeds max_frames
    /// * Graceful: Missing frames are skipped, not errors
    pub fn compose(
        &self,
        node_id: NodeID,
        policy: CompositionPolicy,
    ) -> Result<Vec<Frame>, ApiError> {
        // Verify node exists
        let _node_record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;

        // Compose frames
        let head_index = self.head_index.read();
        let composed = compose_frames(
            node_id,
            &policy,
            self.node_store.as_ref(),
            &self.frame_storage,
            &head_index,
        )?;
        drop(head_index);

        Ok(composed)
    }

    /// Check if a contextframe exists for a specific agent and node
    ///
    /// Returns true if any frame for the node has the specified agent_id in its metadata.
    /// Uses the default frame type "context" for the agent.
    pub fn has_agent_frame(&self, node_id: &NodeID, agent_id: &str) -> Result<bool, ApiError> {
        // Get all head frames for this node
        let frame_ids = self.get_all_heads(node_id);

        // Check if any frame has this agent_id in metadata
        for frame_id in frame_ids {
            if let Some(frame) = self.frame_storage.get(&frame_id).map_err(ApiError::from)? {
                if let Some(frame_agent_id) = frame.metadata.get("agent_id") {
                    if frame_agent_id == agent_id {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    /// Create a default contextframe for an agent if it doesn't exist
    ///
    /// Creates a simple context frame with basic node information.
    /// Only creates frames for agents with write capability.
    /// If a generation queue is provided and the agent has a provider configured,
    /// the frame generation will be queued for asynchronous LLM-based generation.
    ///
    /// # Arguments
    /// * `node_id` - NodeID to create frame for
    /// * `agent_id` - Agent ID to create frame for
    /// * `frame_type` - Frame type (defaults to "context" if None)
    /// * `generation_queue` - Optional generation queue for LLM-based frame generation
    ///
    /// # Returns
    /// * `Option<FrameID>` - FrameID if frame was created synchronously, None if queued or already existed
    pub fn ensure_agent_frame(
        &self,
        node_id: NodeID,
        agent_id: String,
        frame_type: Option<String>,
        _generation_queue: Option<Arc<FrameGenerationQueue>>,
    ) -> Result<Option<FrameID>, ApiError> {
        // Check if frame already exists
        if self.has_agent_frame(&node_id, &agent_id)? {
            return Ok(None);
        }

        // Verify agent exists and can write
        let agent = {
            let registry = self.agent_registry.read();
            registry.get_or_error(&agent_id)?.clone()
        };

        // Only create frames for agents that can write
        if !agent.can_write() {
            return Ok(None);
        }

        // Use provided frame_type or default to "context-{agent_id}" to ensure uniqueness per agent
        let frame_type = frame_type.unwrap_or_else(|| format!("context-{}", agent_id));

        // Get node record for context
        let node_record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;

        // Note: Generation with providers now requires explicit provider_name parameter
        // This method only creates metadata frames. Use ContextApiAdapter::generate_frame
        // with explicit provider_name for LLM-based generation.

        // Fallback: create metadata frame
        // Create default frame content
        let content = format!(
            "Node: {}\nPath: {:?}\nType: {:?}",
            hex::encode(node_id),
            node_record.path,
            node_record.node_type
        )
        .into_bytes();

        // Create frame
        let basis = Basis::Node(node_id);
        let metadata = HashMap::new();
        let frame = Frame::new(
            basis,
            content,
            frame_type.clone(),
            agent_id.clone(),
            metadata,
        )
        .map_err(|e| ApiError::StorageError(e))?;

        // Store frame
        let frame_id = self.put_frame(node_id, frame, agent_id)?;

        Ok(Some(frame_id))
    }

    /// Get agent identity by ID
    ///
    /// This is a helper method for adapters and tooling that need to access
    /// agent information, particularly for provider configuration.
    pub fn get_agent(&self, agent_id: &str) -> Result<crate::agent::AgentIdentity, ApiError> {
        let registry = self.agent_registry.read();
        registry.get_or_error(agent_id).map(|a| a.clone())
    }

    /// Get head frame ID for a node and frame type
    ///
    /// Returns the latest frame ID for the given node and frame type.
    pub fn get_head(
        &self,
        node_id: &NodeID,
        frame_type: &str,
    ) -> Result<Option<FrameID>, ApiError> {
        let head_index = self.head_index.read();
        head_index
            .get_head(node_id, frame_type)
            .map_err(ApiError::from)
    }

    /// Get all head frame IDs for a node
    ///
    /// Returns all frame IDs that are heads for the specified node.
    pub fn get_all_heads(&self, node_id: &NodeID) -> Vec<FrameID> {
        let head_index = self.head_index.read();
        head_index.get_all_heads_for_node(node_id)
    }

    /// Get latest context (most recent frame)
    ///
    /// Convenience method that retrieves the most recent frame for a node.
    /// Equivalent to `get_node()` with a view requesting 1 frame ordered by recency.
    pub fn latest_context(&self, node_id: NodeID) -> Result<NodeContext, ApiError> {
        let view = ContextView {
            max_frames: 1,
            ordering: crate::views::OrderingPolicy::Recency,
            filters: vec![],
        };
        self.get_node(node_id, view)
    }

    /// Get context filtered by frame type
    ///
    /// Retrieves frames matching the specified type, ordered by recency.
    ///
    /// # Arguments
    /// * `node_id` - The NodeID to retrieve context for
    /// * `frame_type` - The frame type to filter by
    /// * `max_frames` - Maximum number of frames to return
    pub fn context_by_type(
        &self,
        node_id: NodeID,
        frame_type: &str,
        max_frames: usize,
    ) -> Result<NodeContext, ApiError> {
        let view = ContextView {
            max_frames,
            ordering: crate::views::OrderingPolicy::Recency,
            filters: vec![crate::views::FrameFilter::ByType(frame_type.to_string())],
        };
        self.get_node(node_id, view)
    }

    /// Get context filtered by agent
    ///
    /// Retrieves frames created by the specified agent, ordered by recency.
    ///
    /// # Arguments
    /// * `node_id` - The NodeID to retrieve context for
    /// * `agent_id` - The agent ID to filter by
    /// * `max_frames` - Maximum number of frames to return
    pub fn context_by_agent(
        &self,
        node_id: NodeID,
        agent_id: &str,
        max_frames: usize,
    ) -> Result<NodeContext, ApiError> {
        let view = ContextView {
            max_frames,
            ordering: crate::views::OrderingPolicy::Recency,
            filters: vec![crate::views::FrameFilter::ByAgent(agent_id.to_string())],
        };
        self.get_node(node_id, view)
    }

    /// Get combined text content directly
    ///
    /// Retrieves context and returns the combined text content of all frames.
    /// Filters out frames with invalid UTF-8 content.
    ///
    /// # Arguments
    /// * `node_id` - The NodeID to retrieve context for
    /// * `separator` - String to use as separator between frame contents
    /// * `view` - ContextView policy for frame selection
    pub fn combined_context_text(
        &self,
        node_id: NodeID,
        separator: &str,
        view: ContextView,
    ) -> Result<String, ApiError> {
        let context = self.get_node(node_id, view)?;
        Ok(context.combined_text(separator))
    }

    /// Get access to frame storage (for tooling)
    pub fn frame_storage(&self) -> &FrameStorage {
        &self.frame_storage
    }

    /// Get access to node store (for tooling)
    pub fn node_store(&self) -> &Arc<dyn NodeRecordStore + Send + Sync> {
        &self.node_store
    }

    /// Get access to head index (for tooling)
    pub fn head_index(&self) -> &Arc<parking_lot::RwLock<HeadIndex>> {
        &self.head_index
    }

    /// Get access to agent registry (for tooling)
    pub fn agent_registry(&self) -> &Arc<parking_lot::RwLock<AgentRegistry>> {
        &self.agent_registry
    }

    /// Get a reference to the provider registry
    pub fn provider_registry(
        &self,
    ) -> &Arc<parking_lot::RwLock<crate::provider::ProviderRegistry>> {
        &self.provider_registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentIdentity;
    use crate::context::frame::{Basis, Frame};
    use crate::store::{NodeRecord, SledNodeRecordStore};
    use crate::views::{FrameFilter, OrderingPolicy};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_api() -> (ContextApi, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
        let agent_registry = Arc::new(parking_lot::RwLock::new(AgentRegistry::new()));
        let lock_manager = Arc::new(NodeLockManager::new());

        let provider_registry = Arc::new(parking_lot::RwLock::new(
            crate::provider::ProviderRegistry::new(),
        ));
        let api = ContextApi::new(
            node_store,
            frame_storage,
            head_index,
            agent_registry,
            provider_registry,
            lock_manager,
        );

        (api, temp_dir)
    }

    fn create_test_node_record(node_id: NodeID) -> NodeRecord {
        use crate::store::NodeType;
        use std::path::PathBuf;

        NodeRecord {
            node_id,
            path: PathBuf::from("/test/file.txt"),
            node_type: NodeType::File {
                size: 100,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: HashMap::new(),
            tombstoned_at: None,
        }
    }

    #[test]
    fn test_get_node_not_found() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];
        let view = ContextView {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let result = api.get_node(node_id, view);
        assert!(result.is_err());
        match result {
            Err(ApiError::NodeNotFound(id)) => assert_eq!(id, node_id),
            _ => panic!("Expected NodeNotFound error"),
        }
    }

    #[test]
    fn test_get_node_empty_context() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        let view = ContextView {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let context = api.get_node(node_id, view).unwrap();
        assert_eq!(context.node_id, node_id);
        assert_eq!(context.frames.len(), 0);
        assert_eq!(context.frame_count, 0);
    }

    #[test]
    fn test_put_frame_node_not_found() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        let basis = Basis::Node(node_id);
        let content = b"test content".to_vec();
        let frame_type = "test".to_string();
        let agent_id = "writer-1".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(basis, content, frame_type, agent_id.clone(), metadata).unwrap();

        let result = api.put_frame(node_id, frame, agent_id);
        assert!(result.is_err());
        match result {
            Err(ApiError::NodeNotFound(id)) => assert_eq!(id, node_id),
            _ => panic!("Expected NodeNotFound error"),
        }
    }

    #[test]
    fn test_put_frame_unauthorized() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a reader agent (cannot write)
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("reader-1".to_string(), crate::agent::AgentRole::Reader);
            registry.register(agent);
        }

        let basis = Basis::Node(node_id);
        let content = b"test content".to_vec();
        let frame_type = "test".to_string();
        let agent_id = "reader-1".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(basis, content, frame_type, agent_id.clone(), metadata).unwrap();

        let result = api.put_frame(node_id, frame, agent_id);
        assert!(result.is_err());
        match result {
            Err(ApiError::Unauthorized(_)) => {}
            _ => panic!("Expected Unauthorized error"),
        }
    }

    #[test]
    fn test_put_frame_success() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        let basis = Basis::Node(node_id);
        let content = b"test content".to_vec();
        let frame_type = "test".to_string();
        let agent_id = "writer-1".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(
            basis,
            content,
            frame_type.clone(),
            agent_id.clone(),
            metadata,
        )
        .unwrap();

        let frame_id = api.put_frame(node_id, frame, agent_id).unwrap();

        // Verify frame was stored
        assert!(api.frame_storage.exists(&frame_id).unwrap());

        // Verify head was updated
        let head_index = api.head_index.read();
        let head = head_index.get_head(&node_id, &frame_type).unwrap();
        assert_eq!(head, Some(frame_id));
    }

    #[test]
    fn test_get_node_with_frames() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Create and put a frame
        let basis = Basis::Node(node_id);
        let content = b"test content".to_vec();
        let frame_type = "test".to_string();
        let agent_id = "writer-1".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(
            basis,
            content,
            frame_type.clone(),
            agent_id.clone(),
            metadata,
        )
        .unwrap();
        let frame_id = api.put_frame(node_id, frame.clone(), agent_id).unwrap();

        // Get node context
        let view = ContextView {
            max_frames: 100,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let context = api.get_node(node_id, view).unwrap();
        assert_eq!(context.node_id, node_id);
        assert_eq!(context.frames.len(), 1);
        assert_eq!(context.frames[0].frame_id, frame_id);
    }

    #[test]
    fn test_has_agent_frame() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Initially, no frame exists
        assert!(!api.has_agent_frame(&node_id, "writer-1").unwrap());

        // Create a frame
        let basis = Basis::Node(node_id);
        let content = b"test content".to_vec();
        let frame_type = "test".to_string();
        let agent_id = "writer-1".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(basis, content, frame_type, agent_id.clone(), metadata).unwrap();
        api.put_frame(node_id, frame, agent_id.clone()).unwrap();

        // Now frame should exist
        assert!(api.has_agent_frame(&node_id, "writer-1").unwrap());
        assert!(!api.has_agent_frame(&node_id, "writer-2").unwrap());
    }

    #[test]
    fn test_ensure_agent_frame_creates_when_missing() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Ensure frame - should create it
        let frame_id = api
            .ensure_agent_frame(node_id, "writer-1".to_string(), None, None)
            .unwrap();
        assert!(frame_id.is_some());

        // Verify frame exists
        assert!(api.has_agent_frame(&node_id, "writer-1").unwrap());

        // Ensure again - should return None (already exists)
        let frame_id2 = api
            .ensure_agent_frame(node_id, "writer-1".to_string(), None, None)
            .unwrap();
        assert!(frame_id2.is_none());
    }

    #[test]
    fn test_ensure_agent_frame_skips_reader_agents() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a reader agent (cannot write)
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("reader-1".to_string(), crate::agent::AgentRole::Reader);
            registry.register(agent);
        }

        // Ensure frame - should return None (reader can't write)
        let frame_id = api
            .ensure_agent_frame(node_id, "reader-1".to_string(), None, None)
            .unwrap();
        assert!(frame_id.is_none());

        // Verify no frame was created
        assert!(!api.has_agent_frame(&node_id, "reader-1").unwrap());
    }

    #[test]
    fn test_ensure_agent_frame_with_custom_frame_type() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Ensure frame with custom frame type
        let frame_id = api
            .ensure_agent_frame(
                node_id,
                "writer-1".to_string(),
                Some("custom-type".to_string()),
                None,
            )
            .unwrap();
        assert!(frame_id.is_some());

        // Verify frame exists and has correct type
        assert!(api.has_agent_frame(&node_id, "writer-1").unwrap());
        let head = api.get_head(&node_id, "custom-type").unwrap();
        assert!(head.is_some());
    }

    #[test]
    fn test_ensure_agent_frame_node_not_found() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Try to ensure frame for non-existent node
        let result = api.ensure_agent_frame(node_id, "writer-1".to_string(), None, None);
        assert!(result.is_err());
        match result {
            Err(ApiError::NodeNotFound(id)) => assert_eq!(id, node_id),
            _ => panic!("Expected NodeNotFound error"),
        }
    }

    // Tests for ergonomic convenience methods

    #[test]
    fn test_frame_text_content() {
        let frame = {
            let node_id: NodeID = [1u8; 32];
            let basis = Basis::Node(node_id);
            let content = b"Hello, world!".to_vec();
            Frame::new(
                basis,
                content,
                "test".to_string(),
                "agent-1".to_string(),
                HashMap::new(),
            )
            .unwrap()
        };

        assert_eq!(frame.text_content().unwrap(), "Hello, world!");
    }

    #[test]
    fn test_frame_agent_id() {
        let frame = {
            let node_id: NodeID = [1u8; 32];
            let basis = Basis::Node(node_id);
            let content = b"test".to_vec();
            Frame::new(
                basis,
                content,
                "test".to_string(),
                "agent-123".to_string(),
                HashMap::new(),
            )
            .unwrap()
        };

        assert_eq!(frame.agent_id(), Some("agent-123"));
    }

    #[test]
    fn test_frame_is_type() {
        let frame = {
            let node_id: NodeID = [1u8; 32];
            let basis = Basis::Node(node_id);
            let content = b"test".to_vec();
            Frame::new(
                basis,
                content,
                "analysis".to_string(),
                "agent-1".to_string(),
                HashMap::new(),
            )
            .unwrap()
        };

        assert!(frame.is_type("analysis"));
        assert!(!frame.is_type("summary"));
    }

    #[test]
    fn test_node_context_text_contents() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Create frames with different types so we get multiple frames
        let agent_id = "writer-1".to_string();
        let basis = Basis::Node(node_id);
        let frame1 = Frame::new(
            basis.clone(),
            b"Frame content 0".to_vec(),
            "type1".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            basis.clone(),
            b"Frame content 1".to_vec(),
            "type2".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame3 = Frame::new(
            basis.clone(),
            b"Frame content 2".to_vec(),
            "type3".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        api.put_frame(node_id, frame1, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame2, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame3, agent_id.clone()).unwrap();

        let view = ContextView {
            max_frames: 10,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let context = api.get_node(node_id, view).unwrap();
        let texts = context.text_contents();

        // With different frame types, we get all frames
        assert_eq!(texts.len(), 3);
        assert!(texts.iter().any(|t| t.contains("Frame content 0")));
        assert!(texts.iter().any(|t| t.contains("Frame content 1")));
        assert!(texts.iter().any(|t| t.contains("Frame content 2")));
    }

    #[test]
    fn test_node_context_combined_text() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Create frames with different types
        let agent_id = "writer-1".to_string();
        let basis = Basis::Node(node_id);
        let frame1 = Frame::new(
            basis.clone(),
            b"First".to_vec(),
            "type1".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            basis.clone(),
            b"Second".to_vec(),
            "type2".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        api.put_frame(node_id, frame1, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame2, agent_id.clone()).unwrap();

        let view = ContextView {
            max_frames: 10,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let context = api.get_node(node_id, view).unwrap();
        let combined = context.combined_text(" | ");

        // With different frame types, we get both frames
        assert!(combined.contains("First"));
        assert!(combined.contains("Second"));
        assert!(combined.contains(" | "));
    }

    #[test]
    fn test_node_context_latest_frame_of_type() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Create frames of different types
        let agent_id = "writer-1".to_string();
        let basis = Basis::Node(node_id);
        let frame1 = Frame::new(
            basis.clone(),
            b"analysis1".to_vec(),
            "analysis".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            basis.clone(),
            b"summary1".to_vec(),
            "summary".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame3 = Frame::new(
            basis.clone(),
            b"analysis2".to_vec(),
            "analysis".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();

        api.put_frame(node_id, frame1, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame2, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame3, agent_id.clone()).unwrap();

        let view = ContextView {
            max_frames: 10,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let context = api.get_node(node_id, view).unwrap();
        let latest_analysis = context.latest_frame_of_type("analysis");

        // Note: Head index only tracks latest frame per type
        // So we get the latest analysis frame (analysis2)
        assert!(latest_analysis.is_some());
        assert_eq!(latest_analysis.unwrap().frame_type, "analysis");
        assert_eq!(
            latest_analysis.unwrap().text_content().unwrap(),
            "analysis2"
        );
    }

    #[test]
    fn test_node_context_frames_by_agent() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register multiple agents
        {
            let mut registry = api.agent_registry.write();
            registry.register(AgentIdentity::new(
                "agent-1".to_string(),
                crate::agent::AgentRole::Writer,
            ));
            registry.register(AgentIdentity::new(
                "agent-2".to_string(),
                crate::agent::AgentRole::Writer,
            ));
        }

        // Create frames from different agents with different types
        let basis = Basis::Node(node_id);
        let frame1 = Frame::new(
            basis.clone(),
            b"content1".to_vec(),
            "type1".to_string(),
            "agent-1".to_string(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            basis.clone(),
            b"content2".to_vec(),
            "type2".to_string(),
            "agent-2".to_string(),
            HashMap::new(),
        )
        .unwrap();
        let frame3 = Frame::new(
            basis.clone(),
            b"content3".to_vec(),
            "type3".to_string(),
            "agent-1".to_string(),
            HashMap::new(),
        )
        .unwrap();

        api.put_frame(node_id, frame1, "agent-1".to_string())
            .unwrap();
        api.put_frame(node_id, frame2, "agent-2".to_string())
            .unwrap();
        api.put_frame(node_id, frame3, "agent-1".to_string())
            .unwrap();

        let view = ContextView {
            max_frames: 10,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let context = api.get_node(node_id, view).unwrap();
        let agent1_frames = context.frames_by_agent("agent-1");

        // With different frame types, we get both agent-1 frames
        assert_eq!(agent1_frames.len(), 2);
        assert!(agent1_frames
            .iter()
            .all(|f| f.agent_id() == Some("agent-1")));
    }

    #[test]
    fn test_context_view_builder() {
        let view = ContextView::builder()
            .max_frames(50)
            .recent()
            .by_type("analysis")
            .by_agent("agent-1")
            .build();

        assert_eq!(view.max_frames, 50);
        assert_eq!(view.ordering, OrderingPolicy::Recency);
        assert_eq!(view.filters.len(), 2);
        assert!(matches!(view.filters[0], FrameFilter::ByType(_)));
        assert!(matches!(view.filters[1], FrameFilter::ByAgent(_)));
    }

    #[test]
    fn test_context_view_builder_defaults() {
        let view = ContextView::builder().build();

        assert_eq!(view.max_frames, 100); // Default
        assert_eq!(view.ordering, OrderingPolicy::Recency); // Default
        assert!(view.filters.is_empty());
    }

    #[test]
    fn test_context_api_latest_context() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Create multiple frames
        let agent_id = "writer-1".to_string();
        let basis = Basis::Node(node_id);
        for i in 0..5 {
            let content = format!("Frame {}", i).into_bytes();
            let frame = Frame::new(
                basis.clone(),
                content,
                "test".to_string(),
                agent_id.clone(),
                HashMap::new(),
            )
            .unwrap();
            api.put_frame(node_id, frame, agent_id.clone()).unwrap();
        }

        let context = api.latest_context(node_id).unwrap();
        assert_eq!(context.frames.len(), 1); // Should only return latest
        assert_eq!(context.frames[0].text_content().unwrap(), "Frame 4");
    }

    #[test]
    fn test_context_api_context_by_type() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Create frames of different types
        let agent_id = "writer-1".to_string();
        let basis = Basis::Node(node_id);
        let frame1 = Frame::new(
            basis.clone(),
            b"analysis1".to_vec(),
            "analysis".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            basis.clone(),
            b"summary1".to_vec(),
            "summary".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame3 = Frame::new(
            basis.clone(),
            b"analysis2".to_vec(),
            "analysis".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();

        api.put_frame(node_id, frame1, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame2, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame3, agent_id.clone()).unwrap();

        let context = api.context_by_type(node_id, "analysis", 10).unwrap();
        // Note: Head index only tracks latest frame per type
        // So we get 1 analysis frame (the latest)
        assert_eq!(context.frames.len(), 1);
        assert!(context.frames.iter().all(|f| f.is_type("analysis")));
    }

    #[test]
    fn test_context_api_combined_context_text() {
        let (api, _temp_dir) = create_test_api();
        let node_id: NodeID = [1u8; 32];

        // Create and store node record
        let node_record = create_test_node_record(node_id);
        api.node_store.put(&node_record).unwrap();

        // Register a writer agent
        {
            let mut registry = api.agent_registry.write();
            let agent = AgentIdentity::new("writer-1".to_string(), crate::agent::AgentRole::Writer);
            registry.register(agent);
        }

        // Create frames with different types
        let agent_id = "writer-1".to_string();
        let basis = Basis::Node(node_id);
        let frame1 = Frame::new(
            basis.clone(),
            b"First".to_vec(),
            "type1".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        let frame2 = Frame::new(
            basis.clone(),
            b"Second".to_vec(),
            "type2".to_string(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        api.put_frame(node_id, frame1, agent_id.clone()).unwrap();
        api.put_frame(node_id, frame2, agent_id.clone()).unwrap();

        let view = ContextView {
            max_frames: 10,
            ordering: OrderingPolicy::Recency,
            filters: vec![],
        };

        let combined = api.combined_context_text(node_id, " | ", view).unwrap();
        // With different frame types, we get both frames
        assert!(combined.contains("First"));
        assert!(combined.contains("Second"));
        assert!(combined.contains(" | "));
    }
}
