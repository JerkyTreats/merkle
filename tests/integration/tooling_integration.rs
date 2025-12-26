//! Integration tests for Tooling & Integrations

use merkle::api::{ContextApi, ContextView};
use merkle::frame::{Basis, Frame};
use merkle::heads::HeadIndex;
use merkle::regeneration::BasisIndex;
use merkle::store::persistence::SledNodeRecordStore;
use merkle::tooling::{AgentAdapter, adapter::ContextApiAdapter, CiIntegration, BatchOperation};
use merkle::types::Hash;
use merkle::views::OrderingPolicy;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_api() -> (ContextApi, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store_path = temp_dir.path().join("store");
    let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
    let frame_storage_path = temp_dir.path().join("frames");
    std::fs::create_dir_all(&frame_storage_path).unwrap();
    let frame_storage = Arc::new(merkle::frame::storage::FrameStorage::new(&frame_storage_path).unwrap());
    let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
    let basis_index = Arc::new(parking_lot::RwLock::new(BasisIndex::new()));
    let agent_registry = Arc::new(parking_lot::RwLock::new(merkle::agent::AgentRegistry::new()));
    let lock_manager = Arc::new(merkle::concurrency::NodeLockManager::new());

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

#[test]
fn test_agent_adapter_interface() {
    let (api, _temp_dir) = create_test_api();
    let adapter = ContextApiAdapter::new(api);
    let node_id = Hash::from([1u8; 32]);

    // Test read_context interface
    let view = ContextView {
        max_frames: 10,
        ordering: OrderingPolicy::Recency,
        filters: vec![],
    };
    let result = adapter.read_context(node_id, view);
    assert!(result.is_err()); // Expected - node doesn't exist, but interface works
}

#[test]
fn test_agent_adapter_synthesize() {
    let (api, _temp_dir) = create_test_api();
    let adapter = ContextApiAdapter::new(api);
    let node_id = Hash::from([2u8; 32]);

    // Synthesize (will fail because node doesn't exist, but tests interface)
    let result = adapter.synthesize(node_id, "test".to_string(), "test-agent".to_string());
    assert!(result.is_err()); // Expected - node doesn't exist, but interface works
}

#[test]
fn test_ci_batch_operation() {
    let (api, _temp_dir) = create_test_api();
    let ci = CiIntegration::new(api);
    let node_ids = vec![Hash::from([3u8; 32]), Hash::from([4u8; 32])];
    let operation = BatchOperation::Regenerate {
        agent_id: "test-agent".to_string(),
        recursive: false,
    };

    let report = ci.batch_process(node_ids, operation).unwrap();
    assert_eq!(report.processed, 2);
    assert_eq!(report.failed, 2); // Both fail because nodes don't exist
    assert_eq!(report.succeeded, 0);
}

#[test]
fn test_ci_validation() {
    let (api, _temp_dir) = create_test_api();
    let ci = CiIntegration::new(api);

    let report = ci.validate_workspace().unwrap();
    assert!(report.valid);
    assert!(report.errors.is_empty());
}

#[test]
fn test_tool_idempotency_put_frame() {
    // Test that putting the same frame twice produces the same result
    // This tests that FrameID generation is deterministic
    let node_id = Hash::from([5u8; 32]);
    let basis = Basis::Node(node_id);
    let content = b"idempotent content".to_vec();
    let frame1 = Frame::new(basis.clone(), content.clone(), "test".to_string(), "test-agent".to_string(), HashMap::new()).unwrap();
    let frame2 = Frame::new(basis, content, "test".to_string(), "test-agent".to_string(), HashMap::new()).unwrap();

    // Both frames should have the same FrameID (deterministic)
    assert_eq!(frame1.frame_id, frame2.frame_id);
}

#[test]
fn test_tool_idempotency_regenerate() {
    // Test that regenerating twice produces the same result
    let (api, _temp_dir) = create_test_api();
    let node_id = Hash::from([6u8; 32]);

    // Register the agent first (regenerate checks for agent existence)
    {
        let mut registry = api.agent_registry().write();
        let agent = merkle::agent::AgentIdentity::new("test-agent".to_string(), merkle::agent::AgentRole::Writer);
        registry.register(agent);
    }

    // Regenerate twice - should be idempotent
    // (Will fail because node doesn't exist, but tests interface)
    let result1 = api.regenerate(node_id, false, "test-agent".to_string());
    let result2 = api.regenerate(node_id, false, "test-agent".to_string());

    // Both should fail the same way (idempotent error handling)
    assert!(result1.is_err());
    assert!(result2.is_err());
    // Both should return the same error (idempotent error handling)
    // We can't easily compare error types, but both should be NodeNotFound
    let err1 = result1.unwrap_err();
    let err2 = result2.unwrap_err();
    // Both should be NodeNotFound errors
    assert!(matches!(err1, merkle::error::ApiError::NodeNotFound(_)));
    assert!(matches!(err2, merkle::error::ApiError::NodeNotFound(_)));
}
