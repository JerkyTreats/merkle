//! Integration tests for Branch Synthesis
//!
//! Tests cover:
//! - Synthesis determinism
//! - Bottom-up synthesis enforcement
//! - Multiple synthesis policies
//! - Empty directory handling
//! - Error handling

use merkle::api::ContextApi;
use merkle::agent::{AgentIdentity, AgentRegistry, AgentRole};
use merkle::concurrency::NodeLockManager;
use merkle::error::ApiError;
use merkle::frame::{Basis, Frame, FrameStorage};
use merkle::heads::HeadIndex;
use merkle::store::{NodeRecord, NodeType, SledNodeRecordStore};
use merkle::synthesis::SynthesisPolicy;
use merkle::types::NodeID;
use std::collections::HashMap;
use std::sync::Arc;
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

    let basis_index = Arc::new(parking_lot::RwLock::new(merkle::regeneration::BasisIndex::new()));
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

fn create_test_file_record(node_id: NodeID, path: &str) -> NodeRecord {
    use std::path::PathBuf;

    NodeRecord {
        node_id,
        path: PathBuf::from(path),
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

fn create_test_directory_record(node_id: NodeID, path: &str, children: Vec<NodeID>) -> NodeRecord {
    use std::path::PathBuf;

    NodeRecord {
        node_id,
        path: PathBuf::from(path),
        node_type: NodeType::Directory,
        children,
        parent: None,
        frame_set_root: None,
        metadata: HashMap::new(),
    }
}

#[test]
fn test_synthesize_branch_deterministic() {
    let (api, _temp_dir) = create_test_api();

    // Create directory with two child files
    let dir_id: NodeID = [1u8; 32];
    let file1_id: NodeID = [2u8; 32];
    let file2_id: NodeID = [3u8; 32];

    // Create and store node records
    let dir_record = create_test_directory_record(dir_id, "/test/dir", vec![file1_id, file2_id]);
    api.node_store().put(&dir_record).unwrap();

    let file1_record = create_test_file_record(file1_id, "/test/dir/file1.txt");
    api.node_store().put(&file1_record).unwrap();

    let file2_record = create_test_file_record(file2_id, "/test/dir/file2.txt");
    api.node_store().put(&file2_record).unwrap();

    // Register a synthesis agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
        registry.register(agent);
    }

    // Create frames for child files
    let frame_type = "analysis".to_string();
    let agent_id = "synthesis-1".to_string();

    let frame1 = Frame::new(
        Basis::Node(file1_id),
        b"file1 content".to_vec(),
        frame_type.clone(),
        agent_id.clone(),
        HashMap::new(),
    )
    .unwrap();
    api.put_frame(file1_id, frame1, agent_id.clone()).unwrap();

    let frame2 = Frame::new(
        Basis::Node(file2_id),
        b"file2 content".to_vec(),
        frame_type.clone(),
        agent_id.clone(),
        HashMap::new(),
    )
    .unwrap();
    api.put_frame(file2_id, frame2, agent_id.clone()).unwrap();

    // Synthesize branch twice with same inputs
    let policy = SynthesisPolicy::Concatenation;
    let frame_id1 = api
        .synthesize_branch(dir_id, frame_type.clone(), agent_id.clone(), Some(policy.clone()))
        .unwrap();

    let frame_id2 = api
        .synthesize_branch(dir_id, frame_type.clone(), agent_id.clone(), Some(policy))
        .unwrap();

    // FrameIDs should be identical (deterministic)
    assert_eq!(frame_id1, frame_id2);
}

#[test]
fn test_synthesize_branch_concatenation_policy() {
    let (api, _temp_dir) = create_test_api();

    // Create directory with two child files
    let dir_id: NodeID = [1u8; 32];
    let file1_id: NodeID = [2u8; 32];
    let file2_id: NodeID = [3u8; 32];

    let dir_record = create_test_directory_record(dir_id, "/test/dir", vec![file1_id, file2_id]);
    api.node_store().put(&dir_record).unwrap();

    let file1_record = create_test_file_record(file1_id, "/test/dir/file1.txt");
    api.node_store().put(&file1_record).unwrap();

    let file2_record = create_test_file_record(file2_id, "/test/dir/file2.txt");
    api.node_store().put(&file2_record).unwrap();

    // Register a synthesis agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
        registry.register(agent);
    }

    let frame_type = "analysis".to_string();
    let agent_id = "synthesis-1".to_string();

    // Create frames for child files
    let frame1 = Frame::new(
        Basis::Node(file1_id),
        b"content1".to_vec(),
        frame_type.clone(),
        agent_id.clone(),
        HashMap::new(),
    )
    .unwrap();
    api.put_frame(file1_id, frame1, agent_id.clone()).unwrap();

    let frame2 = Frame::new(
        Basis::Node(file2_id),
        b"content2".to_vec(),
        frame_type.clone(),
        agent_id.clone(),
        HashMap::new(),
    )
    .unwrap();
    api.put_frame(file2_id, frame2, agent_id.clone()).unwrap();

    // Synthesize with concatenation policy
    let policy = SynthesisPolicy::Concatenation;
    let synthesized_frame_id = api
        .synthesize_branch(dir_id, frame_type.clone(), agent_id.clone(), Some(policy))
        .unwrap();

    // Retrieve synthesized frame
    let synthesized_frame = api
        .frame_storage()
        .get(&synthesized_frame_id)
        .unwrap()
        .unwrap();

    // Content should contain both child contents
    let content_str = String::from_utf8_lossy(&synthesized_frame.content);
    assert!(content_str.contains("content1"));
    assert!(content_str.contains("content2"));
}

#[test]
fn test_synthesize_branch_empty_directory() {
    let (api, _temp_dir) = create_test_api();

    // Create empty directory
    let dir_id: NodeID = [1u8; 32];
    let dir_record = create_test_directory_record(dir_id, "/test/dir", vec![]);
    api.node_store().put(&dir_record).unwrap();

    // Register a synthesis agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
        registry.register(agent);
    }

    let frame_type = "summary".to_string();
    let agent_id = "synthesis-1".to_string();

    // Synthesize empty directory
    let frame_id = api
        .synthesize_branch(dir_id, frame_type.clone(), agent_id.clone(), None)
        .unwrap();

    // Should create a frame (empty directory frame)
    let frame = api.frame_storage().get(&frame_id).unwrap().unwrap();
    assert_eq!(frame.frame_type, frame_type);
    let content_str = String::from_utf8_lossy(&frame.content);
    assert!(content_str.contains("Empty"));
}

#[test]
fn test_synthesize_branch_file_not_directory() {
    let (api, _temp_dir) = create_test_api();

    // Create a file (not a directory)
    let file_id: NodeID = [1u8; 32];
    let file_record = create_test_file_record(file_id, "/test/file.txt");
    api.node_store().put(&file_record).unwrap();

    // Register a synthesis agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
        registry.register(agent);
    }

    let frame_type = "summary".to_string();
    let agent_id = "synthesis-1".to_string();

    // Try to synthesize a file (should fail)
    let result = api.synthesize_branch(file_id, frame_type, agent_id, None);

    assert!(result.is_err());
    match result {
        Err(ApiError::SynthesisFailed(msg)) => {
            assert!(msg.contains("file"));
            assert!(msg.contains("directory"));
        }
        _ => panic!("Expected SynthesisFailed error"),
    }
}

#[test]
fn test_synthesize_branch_unauthorized() {
    let (api, _temp_dir) = create_test_api();

    // Create directory
    let dir_id: NodeID = [1u8; 32];
    let dir_record = create_test_directory_record(dir_id, "/test/dir", vec![]);
    api.node_store().put(&dir_record).unwrap();

    // Register a writer agent (cannot synthesize)
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
        registry.register(agent);
    }

    let frame_type = "summary".to_string();
    let agent_id = "writer-1".to_string();

    // Try to synthesize with writer agent (should fail)
    let result = api.synthesize_branch(dir_id, frame_type, agent_id, None);

    assert!(result.is_err());
    match result {
        Err(ApiError::Unauthorized(_)) => {}
        _ => panic!("Expected Unauthorized error"),
    }
}

#[test]
fn test_synthesize_branch_summarization_policy() {
    let (api, _temp_dir) = create_test_api();

    // Create directory with child files
    let dir_id: NodeID = [1u8; 32];
    let file1_id: NodeID = [2u8; 32];
    let file2_id: NodeID = [3u8; 32];

    let dir_record = create_test_directory_record(dir_id, "/test/dir", vec![file1_id, file2_id]);
    api.node_store().put(&dir_record).unwrap();

    let file1_record = create_test_file_record(file1_id, "/test/dir/file1.txt");
    api.node_store().put(&file1_record).unwrap();

    let file2_record = create_test_file_record(file2_id, "/test/dir/file2.txt");
    api.node_store().put(&file2_record).unwrap();

    // Register a synthesis agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
        registry.register(agent);
    }

    let frame_type = "analysis".to_string();
    let agent_id = "synthesis-1".to_string();

    // Create frames for child files
    let frame1 = Frame::new(
        Basis::Node(file1_id),
        b"content1".to_vec(),
        frame_type.clone(),
        agent_id.clone(),
        HashMap::new(),
    )
    .unwrap();
    api.put_frame(file1_id, frame1, agent_id.clone()).unwrap();

    let frame2 = Frame::new(
        Basis::Node(file2_id),
        b"content2".to_vec(),
        frame_type.clone(),
        agent_id.clone(),
        HashMap::new(),
    )
    .unwrap();
    api.put_frame(file2_id, frame2, agent_id.clone()).unwrap();

    // Synthesize with summarization policy
    let policy = SynthesisPolicy::Summarization;
    let synthesized_frame_id = api
        .synthesize_branch(dir_id, frame_type.clone(), agent_id.clone(), Some(policy))
        .unwrap();

    // Retrieve synthesized frame
    let synthesized_frame = api
        .frame_storage()
        .get(&synthesized_frame_id)
        .unwrap()
        .unwrap();

    // Content should contain summary information
    let content_str = String::from_utf8_lossy(&synthesized_frame.content);
    assert!(content_str.contains("Summary"));
    assert!(content_str.contains("frames"));
}

#[test]
fn test_synthesize_branch_filtering_policy() {
    let (api, _temp_dir) = create_test_api();

    // Create directory with three child files
    let dir_id: NodeID = [1u8; 32];
    let file1_id: NodeID = [2u8; 32];
    let file2_id: NodeID = [3u8; 32];
    let file3_id: NodeID = [4u8; 32];

    let dir_record = create_test_directory_record(
        dir_id,
        "/test/dir",
        vec![file1_id, file2_id, file3_id],
    );
    api.node_store().put(&dir_record).unwrap();

    let file1_record = create_test_file_record(file1_id, "/test/dir/file1.txt");
    api.node_store().put(&file1_record).unwrap();

    let file2_record = create_test_file_record(file2_id, "/test/dir/file2.txt");
    api.node_store().put(&file2_record).unwrap();

    let file3_record = create_test_file_record(file3_id, "/test/dir/file3.txt");
    api.node_store().put(&file3_record).unwrap();

    // Register a synthesis agent
    {
        let mut registry = api.agent_registry().write();
        let agent = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
        registry.register(agent);
    }

    let frame_type = "analysis".to_string();
    let agent_id = "synthesis-1".to_string();

    // Create frames for child files
    for (i, file_id) in [file1_id, file2_id, file3_id].iter().enumerate() {
        let frame = Frame::new(
            Basis::Node(*file_id),
            format!("content{}", i + 1).into_bytes(),
            frame_type.clone(),
            agent_id.clone(),
            HashMap::new(),
        )
        .unwrap();
        api.put_frame(*file_id, frame, agent_id.clone()).unwrap();
    }

    // Synthesize with filtering policy (max 2 frames)
    let policy = SynthesisPolicy::Filtering { max_frames: 2 };
    let synthesized_frame_id = api
        .synthesize_branch(dir_id, frame_type.clone(), agent_id.clone(), Some(policy))
        .unwrap();

    // Retrieve synthesized frame
    let synthesized_frame = api
        .frame_storage()
        .get(&synthesized_frame_id)
        .unwrap()
        .unwrap();

    // Content should contain only first 2 frames
    let content_str = String::from_utf8_lossy(&synthesized_frame.content);
    assert!(content_str.contains("content1"));
    assert!(content_str.contains("content2"));
    assert!(!content_str.contains("content3"));
}
