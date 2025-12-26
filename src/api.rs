//! Core Context APIs
//!
//! Provides minimal, stateless API surface for agent interaction with the context engine.
//! Implements GetNode and PutFrame APIs as specified in Phase 2B.

use crate::agent::AgentRegistry;
use crate::composition::{CompositionPolicy, compose_frames};
use crate::concurrency::NodeLockManager;
use crate::error::ApiError;
use crate::frame::{Basis, Frame, FrameMerkleSet, FrameStorage};
use crate::frame::id::compute_basis_hash;
use crate::heads::HeadIndex;
use crate::regeneration::{BasisIndex, RegenerationReport, regenerate_node};
use crate::store::{NodeRecord, NodeRecordStore};
use crate::synthesis::{collect_child_frames, synthesize_content, SynthesisBasis, SynthesisPolicy};
use crate::types::{FrameID, NodeID};
use crate::views::{get_context_view, ViewPolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Context view policy for frame selection
///
/// Wraps ViewPolicy to provide a clean API interface.
/// This is the policy-driven view that determines which frames are selected
/// and how they are ordered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextView {
    /// Maximum number of frames to return
    pub max_frames: usize,
    /// Ordering policy for frame selection
    pub ordering: crate::views::OrderingPolicy,
    /// Filters to apply before ordering
    pub filters: Vec<crate::views::FrameFilter>,
}

impl From<ViewPolicy> for ContextView {
    fn from(policy: ViewPolicy) -> Self {
        ContextView {
            max_frames: policy.max_frames,
            ordering: policy.ordering,
            filters: policy.filters,
        }
    }
}

impl From<ContextView> for ViewPolicy {
    fn from(view: ContextView) -> Self {
        ViewPolicy {
            max_frames: view.max_frames,
            ordering: view.ordering,
            filters: view.filters,
        }
    }
}

/// Node context response
///
/// Contains the node record and selected frames based on the context view policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeContext {
    /// The NodeID for this context
    pub node_id: NodeID,
    /// The node record (metadata, path, children, etc.)
    pub node_record: NodeRecord,
    /// Selected frames based on the view policy
    pub frames: Vec<Frame>,
    /// Total frame count (may exceed view limit)
    pub frame_count: usize,
}

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
    /// Basis index for regeneration (Phase 2D)
    basis_index: Arc<parking_lot::RwLock<BasisIndex>>,
    /// Agent registry for authorization
    agent_registry: Arc<parking_lot::RwLock<AgentRegistry>>,
    /// Lock manager for concurrent access safety
    lock_manager: Arc<NodeLockManager>,
}

impl ContextApi {
    /// Create a new Context API service
    pub fn new(
        node_store: Arc<dyn NodeRecordStore + Send + Sync>,
        frame_storage: Arc<FrameStorage>,
        head_index: Arc<parking_lot::RwLock<HeadIndex>>,
        basis_index: Arc<parking_lot::RwLock<BasisIndex>>,
        agent_registry: Arc<parking_lot::RwLock<AgentRegistry>>,
        lock_manager: Arc<NodeLockManager>,
    ) -> Self {
        Self {
            node_store,
            frame_storage,
            head_index,
            basis_index,
            agent_registry,
            lock_manager,
        }
    }

    /// Get node context using policy-driven view
    ///
    /// Retrieves the node record and selected frames based on the context view policy.
    /// This is a read-only operation that never triggers writes or synthesis.
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
    /// * Read-only: Never triggers writes or synthesis
    /// * Bounded: Frame count limited by view policy
    pub fn get_node(&self, node_id: NodeID, view: ContextView) -> Result<NodeContext, ApiError> {
        // Verify node exists
        let node_record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;

        // Note: frame_set_root from node_record is not used in Phase 2B MVP
        // In a full implementation, we would use it to retrieve the FrameMerkleSet from storage

        // Get all frame types for this node from head index
        // Note: In a full implementation, we would retrieve the FrameMerkleSet from storage
        // using the frame_set_root. For Phase 2B MVP, we use the head index to find frames.
        let head_index = self.head_index.read();
        let frame_ids = head_index.get_all_heads_for_node(&node_id);
        drop(head_index);

        // If no frames, return empty context
        if frame_ids.is_empty() {
            return Ok(NodeContext {
                node_id,
                node_record,
                frames: vec![],
                frame_count: 0,
            });
        }

        // Create a temporary FrameMerkleSet from collected frame IDs
        // This allows us to use the existing get_context_view function
        let frame_set = FrameMerkleSet::from_frame_ids(frame_ids.iter().copied())
            .map_err(|e| ApiError::StorageError(e))?;

        // Get context view using the policy
        let view_policy: ViewPolicy = view.into();
        let selected_frame_ids = get_context_view(&frame_set, &self.frame_storage, &view_policy)
            .map_err(|e| ApiError::StorageError(e))?;

        // Retrieve full frame objects
        let mut frames = Vec::new();
        for frame_id in selected_frame_ids {
            if let Some(frame) = self
                .frame_storage
                .get(&frame_id)
                .map_err(ApiError::from)?
            {
                frames.push(frame);
            }
        }

        // Get total frame count (we need to count all frames in the set)
        // For now, use the frame_set length
        let total_frame_count = frame_set.len();

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
    pub fn put_frame(
        &self,
        node_id: NodeID,
        frame: Frame,
        agent_id: String,
    ) -> Result<FrameID, ApiError> {
        // Verify agent exists and has write permission
        let agent = {
            let registry = self.agent_registry.read();
            registry
                .get_or_error(&agent_id)?
                .clone() // Clone to release lock
        };

        // Verify agent can write
        agent.verify_write()?;

        // Verify node exists
        let _node_record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;

        // Verify frame basis matches node_id (if basis is Node-based)
        match &frame.basis {
            crate::frame::Basis::Node(basis_node_id) => {
                if *basis_node_id != node_id {
                    return Err(ApiError::InvalidFrame(format!(
                        "Frame basis node_id {:?} does not match requested node_id {:?}",
                        basis_node_id, node_id
                    )));
                }
            }
            crate::frame::Basis::Frame(_) => {
                // Frame-based basis is OK (can reference other frames)
            }
            crate::frame::Basis::Both { node, .. } => {
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
        self.frame_storage
            .store(&frame)
            .map_err(ApiError::from)?;

        // Update frame set (get or create)
        // TODO: In a full implementation, we'd retrieve the FrameMerkleSet from storage
        // and update it. For Phase 2B MVP, we'll track frame sets in memory.
        // For now, we'll just update the head index.

        // Update head index and basis index atomically
        {
            let mut head_index = self.head_index.write();
            head_index
                .update_head(&node_id, &frame.frame_type, &frame.frame_id)
                .map_err(ApiError::from)?;
        }

        // Update basis index (Phase 2D)
        {
            let basis_hash = compute_basis_hash(&frame.basis).map_err(ApiError::from)?;
            let mut basis_index = self.basis_index.write();
            basis_index.add_frame(basis_hash, frame.frame_id);
        }

        // TODO: Update node record's frame_set_root
        // This requires retrieving/updating the FrameMerkleSet and storing it.
        // For Phase 2B MVP, we'll skip this and rely on head index.

        Ok(frame.frame_id)
    }

    /// Synthesize branch: Create directory-level context from child nodes
    ///
    /// Combines context frames from child nodes into a single synthesized frame
    /// for the parent directory. Synthesis is deterministic, bottom-up, and
    /// limited to explicit subtree scope.
    ///
    /// # Arguments
    /// * `node_id` - Directory NodeID to synthesize
    /// * `frame_type` - Type identifier for the synthesized frame
    /// * `agent_id` - Identity of synthesis agent
    /// * `policy` - Optional synthesis policy (default: Concatenation)
    ///
    /// # Returns
    /// * `FrameID` - The generated FrameID for the synthesized frame
    /// * `ApiError` - Error if node not found, not a directory, or synthesis fails
    ///
    /// # Behavior
    /// * Explicit: Only called via API, never implicit
    /// * Bottom-up: Requires child frames to exist
    /// * Deterministic: Same child frames → same synthesized frame
    pub fn synthesize_branch(
        &self,
        node_id: NodeID,
        frame_type: String,
        agent_id: String,
        policy: Option<SynthesisPolicy>,
    ) -> Result<FrameID, ApiError> {
        // Verify agent exists and has synthesize permission
        let agent = {
            let registry = self.agent_registry.read();
            registry
                .get_or_error(&agent_id)?
                .clone() // Clone to release lock
        };

        // Verify agent can synthesize
        agent.verify_synthesize()?;

        // Verify node exists and is a directory
        let dir_record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;

        match dir_record.node_type {
            crate::store::NodeType::Directory => {}
            crate::store::NodeType::File { .. } => {
                return Err(ApiError::SynthesisFailed(format!(
                    "Node {:?} is a file, not a directory",
                    node_id
                )));
            }
        }

        // Use provided policy or default to Concatenation
        let policy = policy.unwrap_or_default();

        // Acquire write lock for this node (atomic operation)
        let lock = self.lock_manager.get_lock(&node_id);
        let _guard = lock.write();

        // Collect child frames
        let head_index = self.head_index.read();
        let child_frames = collect_child_frames(
            self.node_store.as_ref(),
            &self.frame_storage,
            &head_index,
            node_id,
            &frame_type,
        )?;
        drop(head_index);

        // If no child frames, create empty frame
        if child_frames.is_empty() {
            let basis = Basis::Node(node_id);
            let content = b"Empty directory".to_vec();
            let metadata = {
                let mut m = HashMap::new();
                m.insert("synthesis_policy".to_string(), "concatenation".to_string());
                m
            };

            let frame = Frame::new(basis, content, frame_type.clone(), agent_id.clone(), metadata)?;

            // Store frame
            self.frame_storage.store(&frame).map_err(ApiError::from)?;

            // Update head index and basis index
            {
                let mut head_index = self.head_index.write();
                head_index
                    .update_head(&node_id, &frame_type, &frame.frame_id)
                    .map_err(ApiError::from)?;
            }

            {
                let basis_hash = compute_basis_hash(&frame.basis).map_err(ApiError::from)?;
                let mut basis_index = self.basis_index.write();
                basis_index.add_frame(basis_hash, frame.frame_id);
            }

            return Ok(frame.frame_id);
        }

        // Extract child frame IDs for basis construction
        let child_frame_ids: Vec<FrameID> = child_frames.iter().map(|(_, frame)| frame.frame_id).collect();

        // Construct synthesis basis
        let basis_info = SynthesisBasis {
            node_id,
            child_frame_ids: child_frame_ids.clone(),
            frame_type: frame_type.clone(),
            synthesis_policy: policy.clone(),
        };

        let basis_hash = basis_info.compute_hash();

        // Synthesize content using policy
        let synthesized_content = synthesize_content(&child_frames, &policy);

        // Create basis from child frame IDs
        // For synthesis, we use Basis::Both with node_id and a hash of child frame IDs
        let basis = if child_frame_ids.len() == 1 {
            Basis::Frame(child_frame_ids[0])
        } else {
            // For multiple frames, we create a synthetic basis
            // In a full implementation, we'd use Basis::Both, but for now we'll use Node
            // and include the basis hash in metadata
            Basis::Node(node_id)
        };

        // Create frame metadata
        let mut metadata = HashMap::new();
        metadata.insert("synthesis_policy".to_string(), format!("{:?}", policy));
        // Encode basis hash as hex string manually
        let basis_hash_hex: String = basis_hash.iter().map(|b| format!("{:02x}", b)).collect();
        metadata.insert("basis_hash".to_string(), basis_hash_hex);
        metadata.insert("child_frame_count".to_string(), child_frame_ids.len().to_string());

        // Create synthesized frame
        let frame = Frame::new(basis, synthesized_content, frame_type.clone(), agent_id.clone(), metadata)?;

        // Store frame
        self.frame_storage.store(&frame).map_err(ApiError::from)?;

        // Update head index and basis index atomically
        {
            let mut head_index = self.head_index.write();
            head_index
                .update_head(&node_id, &frame_type, &frame.frame_id)
                .map_err(ApiError::from)?;
        }

        // Update basis index (Phase 2D)
        {
            let basis_hash = compute_basis_hash(&frame.basis).map_err(ApiError::from)?;
            let mut basis_index = self.basis_index.write();
            basis_index.add_frame(basis_hash, frame.frame_id);
        }

        Ok(frame.frame_id)
    }

    /// Regenerate frames for a node
    ///
    /// Regenerates all frames whose basis has changed. Regeneration is incremental,
    /// localized, and basis-driven—only frames whose basis has changed are regenerated.
    ///
    /// # Arguments
    /// * `node_id` - NodeID to regenerate frames for
    /// * `recursive` - Whether to regenerate descendant nodes
    /// * `agent_id` - Identity of agent performing regeneration
    ///
    /// # Returns
    /// * `RegenerationReport` - Summary of regenerated frames
    /// * `ApiError` - Error if node not found or regeneration fails
    ///
    /// # Behavior
    /// * Incremental: Only regenerates frames with changed basis
    /// * Idempotent: Re-running produces same result
    /// * Atomic: Regeneration is transactional
    /// * Append-only: Old frames preserved
    pub fn regenerate(
        &self,
        node_id: NodeID,
        recursive: bool,
        agent_id: String,
    ) -> Result<RegenerationReport, ApiError> {
        // Verify agent exists
        let _agent = {
            let registry = self.agent_registry.read();
            registry
                .get_or_error(&agent_id)?
                .clone()
        };

        // Verify node exists
        let _node_record = self
            .node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;

        // Acquire write lock for this node (atomic operation)
        let lock = self.lock_manager.get_lock(&node_id);
        let _guard = lock.write();

        // Regenerate
        let mut basis_index = self.basis_index.write();
        let mut head_index = self.head_index.write();

        let report = regenerate_node(
            node_id,
            recursive,
            &mut basis_index,
            &mut head_index,
            &self.frame_storage,
            self.node_store.as_ref(),
            agent_id,
        )?;

        Ok(report)
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
    pub fn get_head(&self, node_id: &NodeID, frame_type: &str) -> Result<Option<FrameID>, ApiError> {
        let head_index = self.head_index.read();
        head_index.get_head(node_id, frame_type).map_err(ApiError::from)
    }

    /// Get all head frame IDs for a node
    ///
    /// Returns all frame IDs that are heads for the specified node.
    pub fn get_all_heads(&self, node_id: &NodeID) -> Vec<FrameID> {
        let head_index = self.head_index.read();
        head_index.get_all_heads_for_node(node_id)
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

    /// Get access to basis index (for tooling)
    pub fn basis_index(&self) -> &Arc<parking_lot::RwLock<BasisIndex>> {
        &self.basis_index
    }

    /// Get access to agent registry (for tooling)
    pub fn agent_registry(&self) -> &Arc<parking_lot::RwLock<AgentRegistry>> {
        &self.agent_registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentIdentity;
    use crate::frame::{Basis, Frame};
    use crate::store::SledNodeRecordStore;
    use crate::views::OrderingPolicy;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_api() -> (ContextApi, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
        let basis_index = Arc::new(parking_lot::RwLock::new(BasisIndex::new()));
        let agent_registry = Arc::new(parking_lot::RwLock::new(AgentRegistry::new()));
        let lock_manager = Arc::new(NodeLockManager::new());

        let api = ContextApi::new(
            node_store,
            frame_storage,
            head_index,
            basis_index,
            agent_registry,
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

        let frame = Frame::new(basis, content, frame_type.clone(), agent_id.clone(), metadata).unwrap();

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

        let frame = Frame::new(basis, content, frame_type.clone(), agent_id.clone(), metadata).unwrap();
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
    fn test_regenerate_idempotent() {
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

        let frame = Frame::new(basis, content, frame_type.clone(), agent_id.clone(), metadata).unwrap();
        let _frame_id = api.put_frame(node_id, frame, agent_id.clone()).unwrap();

        // Regenerate - should be idempotent (no changes)
        let report = api.regenerate(node_id, false, agent_id.clone()).unwrap();
        assert_eq!(report.regenerated_count, 0, "Regeneration should be idempotent");

        // Regenerate again - should still be idempotent
        let report2 = api.regenerate(node_id, false, agent_id).unwrap();
        assert_eq!(report2.regenerated_count, 0, "Second regeneration should also be idempotent");
    }
}
