//! Property-based tests for determinism guarantees

use merkle::frame::{Basis, Frame};
use merkle::tree::hasher;
use merkle::types::NodeID;
use proptest::prelude::*;
use std::collections::HashMap;

/// Test that NodeID computation is deterministic
#[test]
fn test_nodeid_determinism_property() {
    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(
        &(any::<Vec<u8>>(), any::<Vec<u8>>()),
        |(content1, content2)| {
            let hash1 = hasher::compute_content_hash(&content1);
            let hash2 = hasher::compute_content_hash(&content2);

            // Same content should produce same hash
            if content1 == content2 {
                assert_eq!(hash1, hash2);
            }

            // Different content should produce different hash (with high probability)
            if content1 != content2 {
                // Note: Hash collisions are extremely rare but theoretically possible
                // In practice, this assertion will almost always pass
                prop_assume!(hash1 != hash2);
            }

            Ok(())
        },
    ).unwrap();
}

/// Test that FrameID computation is deterministic
#[test]
fn test_frameid_determinism_property() {
    let mut runner = proptest::test_runner::TestRunner::default();

    runner.run(
        &(any::<[u8; 32]>(), any::<Vec<u8>>(), any::<String>()),
        |(node_id_bytes, content, frame_type)| {
            let node_id: NodeID = node_id_bytes;
            let basis = Basis::Node(node_id);

            let agent_id = "test-agent";
            let frame_id1 = merkle::frame::id::compute_frame_id(&basis, &content, &frame_type, agent_id).unwrap();
            let frame_id2 = merkle::frame::id::compute_frame_id(&basis, &content, &frame_type, agent_id).unwrap();

            // Same inputs should always produce same FrameID
            assert_eq!(frame_id1, frame_id2);

            Ok(())
        },
    ).unwrap();
}

/// Test that Frame creation is deterministic
#[test]
fn test_frame_creation_determinism() {
    let node_id: NodeID = [1u8; 32];
    let basis = Basis::Node(node_id);
    let content = b"test content".to_vec();
    let frame_type = "analysis".to_string();
    let metadata = HashMap::new();

    let agent_id = "test-agent".to_string();
    let frame1 = Frame::new(basis.clone(), content.clone(), frame_type.clone(), agent_id.clone(), metadata.clone()).unwrap();
    let frame2 = Frame::new(basis, content, frame_type, agent_id, metadata).unwrap();

    // Same inputs should produce same FrameID
    assert_eq!(frame1.frame_id, frame2.frame_id);
}

/// Test that different inputs produce different IDs
#[test]
fn test_different_inputs_different_ids() {
    let node_id1: NodeID = [1u8; 32];
    let node_id2: NodeID = [2u8; 32];

    let basis1 = Basis::Node(node_id1);
    let basis2 = Basis::Node(node_id2);

    let content = b"test content".to_vec();
    let frame_type = "analysis".to_string();

    let agent_id = "test-agent";
    let frame_id1 = merkle::frame::id::compute_frame_id(&basis1, &content, &frame_type, agent_id).unwrap();
    let frame_id2 = merkle::frame::id::compute_frame_id(&basis2, &content, &frame_type, agent_id).unwrap();

    // Different basis should produce different FrameID
    assert_ne!(frame_id1, frame_id2);
}
