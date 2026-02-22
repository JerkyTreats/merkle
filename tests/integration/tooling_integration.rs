//! Integration tests for Tooling & Integrations

use merkle::api::{ContextApi, ContextView};
use merkle::context::frame::{Basis, Frame};
use merkle::heads::HeadIndex;
use merkle::store::persistence::SledNodeRecordStore;
use merkle::agent::{AgentAdapter, ContextApiAdapter};
use merkle::workspace::{BatchOperation, CiIntegration};
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
    let frame_storage =
        Arc::new(merkle::context::frame::storage::FrameStorage::new(&frame_storage_path).unwrap());
    let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
    let agent_registry = Arc::new(parking_lot::RwLock::new(merkle::agent::AgentRegistry::new()));
    let provider_registry = Arc::new(parking_lot::RwLock::new(
        merkle::provider::ProviderRegistry::new(),
    ));
    let lock_manager = Arc::new(merkle::concurrency::NodeLockManager::new());

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
fn test_ci_batch_operation() {
    let (api, _temp_dir) = create_test_api();
    let ci = CiIntegration::new(api);
    let node_ids = vec![Hash::from([3u8; 32]), Hash::from([4u8; 32])];
    let operation = BatchOperation::EnsureNodeExists;

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
    let frame1 = Frame::new(
        basis.clone(),
        content.clone(),
        "test".to_string(),
        "test-agent".to_string(),
        HashMap::new(),
    )
    .unwrap();
    let frame2 = Frame::new(
        basis,
        content,
        "test".to_string(),
        "test-agent".to_string(),
        HashMap::new(),
    )
    .unwrap();

    // Both frames should have the same FrameID (deterministic)
    assert_eq!(frame1.frame_id, frame2.frame_id);
}
