//! Integration tests for Core Context APIs
//!
//! Tests cover:
//! - GetNode API determinism
//! - PutFrame API determinism
//! - Error handling
//! - Concurrent request handling

use merkle::agent::{AgentIdentity, AgentRegistry, AgentRole};
use merkle::api::{ContextApi, ContextView};
use merkle::concurrency::NodeLockManager;
use merkle::error::ApiError;
use merkle::frame::{Basis, Frame, FrameStorage};
use merkle::heads::HeadIndex;
use merkle::regeneration::BasisIndex;
use merkle::store::{NodeRecord, NodeType, SledNodeRecordStore};
use merkle::types::NodeID;
use merkle::views::OrderingPolicy;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
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
    let provider_registry = Arc::new(parking_lot::RwLock::new(
        merkle::provider::ProviderRegistry::new(),
    ));
    let lock_manager = Arc::new(NodeLockManager::new());

    let api = ContextApi::new(
        node_store,
        frame_storage,
        head_index,
        basis_index,
        agent_registry,
        provider_registry,
        lock_manager,
    );

    (api, temp_dir)
}

fn create_test_node_record(node_id: NodeID) -> NodeRecord {
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
fn test_get_node_deterministic() {
    let (api, _temp_dir) = create_test_api();
    let node_id: NodeID = [1u8; 32];

    // Create and store node record
    let node_record = create_test_node_record(node_id);
    api.node_store().put(&node_record).unwrap();

    // Register a writer agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
        registry.register(agent);
    }

    // Create and put multiple frames
    let basis = Basis::Node(node_id);
    let frame_type = "test".to_string();
    let agent_id = "writer-1".to_string();

    for i in 0..5 {
        let content = format!("content {}", i).into_bytes();
        let metadata = HashMap::new();
        let frame = Frame::new(
            basis.clone(),
            content,
            frame_type.clone(),
            agent_id.clone(),
            metadata,
        )
        .unwrap();
        api.put_frame(node_id, frame, agent_id.clone()).unwrap();
    }

    // Get node context twice with same view
    let view = ContextView {
        max_frames: 100,
        ordering: OrderingPolicy::Recency,
        filters: vec![],
    };

    let context1 = api.get_node(node_id, view.clone()).unwrap();
    let context2 = api.get_node(node_id, view).unwrap();

    // Results should be identical (deterministic)
    assert_eq!(context1.node_id, context2.node_id);
    assert_eq!(context1.frames.len(), context2.frames.len());
    assert_eq!(context1.frame_count, context2.frame_count);

    // Frame IDs should be in same order
    let frame_ids1: Vec<_> = context1.frames.iter().map(|f| f.frame_id).collect();
    let frame_ids2: Vec<_> = context2.frames.iter().map(|f| f.frame_id).collect();
    assert_eq!(frame_ids1, frame_ids2);
}

#[test]
fn test_put_frame_deterministic() {
    let (api, _temp_dir) = create_test_api();
    let node_id: NodeID = [1u8; 32];

    // Create and store node record
    let node_record = create_test_node_record(node_id);
    api.node_store().put(&node_record).unwrap();

    // Register a writer agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
        registry.register(agent);
    }

    let basis = Basis::Node(node_id);
    let content = b"test content".to_vec();
    let frame_type = "test".to_string();
    let agent_id = "writer-1".to_string();
    let metadata = HashMap::new();

    // Create frame with same inputs
    let frame = Frame::new(
        basis.clone(),
        content.clone(),
        frame_type.clone(),
        agent_id.clone(),
        metadata.clone(),
    )
    .unwrap();

    let frame_id1 = frame.frame_id;

    // Create another frame with same inputs
    let frame2 = Frame::new(basis, content, frame_type, agent_id.clone(), metadata).unwrap();

    let frame_id2 = frame2.frame_id;

    // FrameIDs should be identical (deterministic)
    assert_eq!(frame_id1, frame_id2);
}

#[test]
fn test_concurrent_get_node() {
    let (api, _temp_dir) = create_test_api();
    let node_id: NodeID = [1u8; 32];

    // Create and store node record
    let node_record = create_test_node_record(node_id);
    api.node_store().put(&node_record).unwrap();

    // Register a writer agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
        registry.register(agent);
    }

    // Create and put a frame
    let basis = Basis::Node(node_id);
    let content = b"test content".to_vec();
    let frame_type = "test".to_string();
    let agent_id = "writer-1".to_string();
    let metadata = HashMap::new();

    let frame = Frame::new(basis, content, frame_type, agent_id.clone(), metadata).unwrap();
    api.put_frame(node_id, frame, agent_id).unwrap();

    // Spawn multiple threads that all read the same node
    let api = Arc::new(api);
    let mut handles = vec![];
    let success_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    for _ in 0..10 {
        let api = api.clone();
        let success_count = success_count.clone();
        let handle = thread::spawn(move || {
            let view = ContextView {
                max_frames: 100,
                ordering: OrderingPolicy::Recency,
                filters: vec![],
            };

            let result = api.get_node(node_id, view);
            if result.is_ok() {
                success_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // All reads should have completed successfully
    assert_eq!(success_count.load(std::sync::atomic::Ordering::SeqCst), 10);
}

#[test]
fn test_concurrent_put_frame() {
    let (api, _temp_dir) = create_test_api();
    let node_id: NodeID = [1u8; 32];

    // Create and store node record
    let node_record = create_test_node_record(node_id);
    api.node_store().put(&node_record).unwrap();

    // Register multiple writer agents
    {
        let mut registry = api.agent_registry().write();
        for i in 0..5 {
            let agent = AgentIdentity::new(format!("writer-{}", i), AgentRole::Writer);
            registry.register(agent);
        }
    }

    // Spawn multiple threads that all write to the same node
    let api = Arc::new(api);
    let mut handles = vec![];
    let success_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    for i in 0..5 {
        let api = api.clone();
        let success_count = success_count.clone();
        let agent_id = format!("writer-{}", i);
        let frame_type = format!("test-{}", i); // Use different frame types so each becomes a head
        let handle = thread::spawn(move || {
            let basis = Basis::Node(node_id);
            let content = format!("content from {}", agent_id).into_bytes();
            let metadata = HashMap::new();

            let frame = Frame::new(
                basis,
                content,
                frame_type.clone(),
                agent_id.clone(),
                metadata,
            )
            .unwrap();

            let result = api.put_frame(node_id, frame, agent_id);
            if result.is_ok() {
                success_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // All writes should have completed successfully (serialized by locks)
    assert_eq!(success_count.load(std::sync::atomic::Ordering::SeqCst), 5);

    // Verify all frames were stored
    let view = ContextView {
        max_frames: 100,
        ordering: OrderingPolicy::Recency,
        filters: vec![],
    };

    let context = api.get_node(node_id, view).unwrap();
    assert_eq!(context.frames.len(), 5);
}

#[test]
fn test_error_handling_node_not_found() {
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
fn test_error_handling_unauthorized() {
    let (api, _temp_dir) = create_test_api();
    let node_id: NodeID = [1u8; 32];

    // Create and store node record
    let node_record = create_test_node_record(node_id);
    api.node_store().put(&node_record).unwrap();

    // Register a reader agent (cannot write)
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("reader-1".to_string(), AgentRole::Reader);
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
fn test_error_handling_invalid_frame_basis() {
    let (api, _temp_dir) = create_test_api();
    let node_id: NodeID = [1u8; 32];
    let other_node_id: NodeID = [2u8; 32];

    // Create and store node record
    let node_record = create_test_node_record(node_id);
    api.node_store().put(&node_record).unwrap();

    // Register a writer agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
        registry.register(agent);
    }

    // Create frame with basis pointing to different node
    let basis = Basis::Node(other_node_id);
    let content = b"test content".to_vec();
    let frame_type = "test".to_string();
    let agent_id = "writer-1".to_string();
    let metadata = HashMap::new();

    let frame = Frame::new(basis, content, frame_type, agent_id.clone(), metadata).unwrap();

    let result = api.put_frame(node_id, frame, agent_id);
    assert!(result.is_err());
    match result {
        Err(ApiError::InvalidFrame(_)) => {}
        _ => panic!("Expected InvalidFrame error"),
    }
}
