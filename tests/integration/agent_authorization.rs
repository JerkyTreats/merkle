//! Integration tests for Agent Authorization and Access Control
//!
//! Tests cover:
//! - Agent roles and authorization
//! - Writer append rules (no mutation, only append)
//! - Reader access rules
//! - Concurrent access safety
//! - Agent identity preserved in frames

use merkle::agent::{AgentIdentity, AgentRegistry, AgentRole};
use merkle::concurrency::NodeLockManager;
use merkle::error::ApiError;
use merkle::frame::{Basis, Frame, FrameStorage};
use merkle::types::NodeID;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use tempfile::TempDir;

#[test]
fn test_agent_roles_authorization() {
    // Test that Reader agents cannot write
    let reader = AgentIdentity::new("reader-1".to_string(), AgentRole::Reader);
    assert!(reader.verify_read().is_ok());
    assert!(reader.verify_write().is_err());
    assert!(reader.verify_synthesize().is_err());

    // Test that Writer agents can read and write
    let writer = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);
    assert!(writer.verify_read().is_ok());
    assert!(writer.verify_write().is_ok());
    assert!(writer.verify_synthesize().is_err());

    // Test that Synthesis agents can do everything
    let synthesis = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);
    assert!(synthesis.verify_read().is_ok());
    assert!(synthesis.verify_write().is_ok());
    assert!(synthesis.verify_synthesize().is_ok());
}

#[test]
fn test_agent_registry() {
    let mut registry = AgentRegistry::new();

    let reader = AgentIdentity::new("reader-1".to_string(), AgentRole::Reader);
    let writer = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);

    registry.register(reader);
    registry.register(writer);

    // Test retrieval
    assert!(registry.get("reader-1").is_some());
    assert!(registry.get("writer-1").is_some());
    assert!(registry.get("unknown").is_none());

    // Test error handling
    assert!(registry.get_or_error("reader-1").is_ok());
    assert!(registry.get_or_error("unknown").is_err());
}

#[test]
fn test_agent_identity_preserved_in_frames() {
    let temp_dir = TempDir::new().unwrap();
    let storage = FrameStorage::new(temp_dir.path()).unwrap();

    let node_id: NodeID = [1u8; 32];
    let basis = Basis::Node(node_id);
    let content = b"test content".to_vec();
    let frame_type = "test".to_string();
    let agent_id = "test-agent-123".to_string();
    let metadata = HashMap::new();

    // Create frame with agent_id
    let frame = Frame::new(basis, content, frame_type, agent_id.clone(), metadata).unwrap();

    // Verify agent_id is in metadata
    assert_eq!(frame.metadata.get("agent_id"), Some(&agent_id));

    // Store and retrieve frame
    storage.store(&frame).unwrap();
    let retrieved = storage.get(&frame.frame_id).unwrap().unwrap();

    // Verify agent_id is preserved
    assert_eq!(retrieved.metadata.get("agent_id"), Some(&agent_id));
}

#[test]
fn test_different_agents_produce_different_frame_ids() {
    let node_id: NodeID = [1u8; 32];
    let basis = Basis::Node(node_id);
    let content = b"test content".to_vec();
    let frame_type = "test".to_string();
    let metadata = HashMap::new();

    // Create frames with same content but different agents
    let frame1 = Frame::new(
        basis.clone(),
        content.clone(),
        frame_type.clone(),
        "agent-1".to_string(),
        metadata.clone(),
    )
    .unwrap();

    let frame2 = Frame::new(
        basis.clone(),
        content.clone(),
        frame_type.clone(),
        "agent-2".to_string(),
        metadata.clone(),
    )
    .unwrap();

    // Different agents should produce different FrameIDs (agent identity requirement)
    assert_ne!(frame1.frame_id, frame2.frame_id);
}

#[test]
fn test_writer_append_only_no_mutation() {
    let temp_dir = TempDir::new().unwrap();
    let storage = FrameStorage::new(temp_dir.path()).unwrap();

    let node_id: NodeID = [1u8; 32];
    let basis = Basis::Node(node_id);
    let content1 = b"content 1".to_vec();
    let content2 = b"content 2".to_vec();
    let frame_type = "test".to_string();
    let agent_id = "writer-1".to_string();
    let metadata = HashMap::new();

    // Create and store first frame
    let frame1 = Frame::new(
        basis.clone(),
        content1,
        frame_type.clone(),
        agent_id.clone(),
        metadata.clone(),
    )
    .unwrap();
    let frame1_id = frame1.frame_id;
    storage.store(&frame1).unwrap();

    // Create and store second frame (append, not mutation)
    let frame2 = Frame::new(
        basis.clone(),
        content2,
        frame_type.clone(),
        agent_id.clone(),
        metadata.clone(),
    )
    .unwrap();
    let frame2_id = frame2.frame_id;
    storage.store(&frame2).unwrap();

    // Both frames should exist (append-only, no mutation)
    assert!(storage.exists(&frame1_id).unwrap());
    assert!(storage.exists(&frame2_id).unwrap());
    assert_ne!(frame1_id, frame2_id);

    // Verify both frames can be retrieved
    let retrieved1 = storage.get(&frame1_id).unwrap().unwrap();
    let retrieved2 = storage.get(&frame2_id).unwrap().unwrap();
    assert_eq!(retrieved1.content, b"content 1");
    assert_eq!(retrieved2.content, b"content 2");
}

#[test]
fn test_concurrent_reads_safe() {
    let manager = Arc::new(NodeLockManager::new());
    let node_id: NodeID = [1u8; 32];
    let success_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Spawn multiple threads that all read-lock the same node
    let mut handles = vec![];
    for _ in 0..10 {
        let manager = manager.clone();
        let success_count = success_count.clone();
        let handle = thread::spawn(move || {
            let lock = manager.get_lock(&node_id);
            let _guard = lock.read();
            // Simulate read operation
            success_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
fn test_concurrent_writes_serialized() {
    let manager = Arc::new(NodeLockManager::new());
    let node_id: NodeID = [1u8; 32];
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Spawn multiple threads that all write-lock the same node
    let mut handles = vec![];
    for _ in 0..5 {
        let manager = manager.clone();
        let counter = counter.clone();
        let handle = thread::spawn(move || {
            let lock = manager.get_lock(&node_id);
            let _guard = lock.write();
            // Simulate write operation
            let current = counter.load(std::sync::atomic::Ordering::SeqCst);
            thread::yield_now(); // Give other threads a chance
            counter.store(current + 1, std::sync::atomic::Ordering::SeqCst);
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // All writes should have completed sequentially (no lost updates)
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 5);
}

#[test]
fn test_different_nodes_concurrent_access() {
    let manager = Arc::new(NodeLockManager::new());
    let node_id1: NodeID = [1u8; 32];
    let node_id2: NodeID = [2u8; 32];
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Spawn threads that lock different nodes
    let mut handles = vec![];
    for i in 0..10 {
        let manager = manager.clone();
        let counter = counter.clone();
        let node_id = if i % 2 == 0 { node_id1 } else { node_id2 };
        let handle = thread::spawn(move || {
            let lock = manager.get_lock(&node_id);
            let _guard = lock.write();
            // Simulate operation
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // All operations should complete (different nodes don't block each other)
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 10);
}

#[test]
fn test_reader_cannot_write_via_authorization() {
    let reader = AgentIdentity::new("reader-1".to_string(), AgentRole::Reader);

    // Reader should fail authorization check for write
    let result = reader.verify_write();
    assert!(result.is_err());
    match result {
        Err(ApiError::Unauthorized(msg)) => {
            assert!(msg.contains("reader-1"));
            assert!(msg.contains("cannot write"));
        }
        _ => panic!("Expected Unauthorized error"),
    }
}

#[test]
fn test_writer_can_read_and_write() {
    let writer = AgentIdentity::new("writer-1".to_string(), AgentRole::Writer);

    // Writer should pass both read and write authorization
    assert!(writer.verify_read().is_ok());
    assert!(writer.verify_write().is_ok());
}

#[test]
fn test_synthesis_agent_full_capabilities() {
    let synthesis = AgentIdentity::new("synthesis-1".to_string(), AgentRole::Synthesis);

    // Synthesis agent should pass all authorization checks
    assert!(synthesis.verify_read().is_ok());
    assert!(synthesis.verify_write().is_ok());
    assert!(synthesis.verify_synthesize().is_ok());
}
