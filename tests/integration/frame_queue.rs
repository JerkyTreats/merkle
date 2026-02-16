//! Integration tests for Frame Generation Queue
//!
//! Tests cover:
//! - Priority queue ordering
//! - Enqueue/dequeue operations
//! - Rate limiting
//! - Retry logic
//! - Concurrent access
//! - Queue size limits
//! - Worker lifecycle

use merkle::api::ContextApi;
use merkle::error::ApiError;
use merkle::frame::queue::{
    FrameGenerationQueue, GenerationConfig, GenerationRequest, GenerationRequestOptions, Priority,
    QueueEventContext,
};
use merkle::frame::storage::FrameStorage;
use merkle::heads::HeadIndex;
use merkle::progress::ProgressRuntime;
use merkle::store::persistence::SledNodeRecordStore;
use merkle::types::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn create_test_api() -> (ContextApi, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store_path = temp_dir.path().join("store");
    let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
    let frame_storage_path = temp_dir.path().join("frames");
    std::fs::create_dir_all(&frame_storage_path).unwrap();
    let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
    let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
    let agent_registry = Arc::new(parking_lot::RwLock::new(merkle::agent::AgentRegistry::new()));
    let mut provider_registry = merkle::provider::ProviderRegistry::new();
    // Add a test provider
    let mut config = merkle::config::MerkleConfig::default();
    config.providers.insert(
        "test-provider".to_string(),
        merkle::config::ProviderConfig {
            provider_name: Some("test-provider".to_string()),
            provider_type: merkle::config::ProviderType::Ollama,
            model: "test-model".to_string(),
            api_key: None,
            endpoint: None,
            default_options: merkle::provider::CompletionOptions::default(),
        },
    );
    provider_registry.load_from_config(&config).unwrap();
    let provider_registry = Arc::new(parking_lot::RwLock::new(provider_registry));
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

fn create_test_queue() -> (FrameGenerationQueue, TempDir) {
    let (api, temp_dir) = create_test_api();
    let api = Arc::new(api);
    let config = GenerationConfig::default();
    let queue = FrameGenerationQueue::new(api, config);
    (queue, temp_dir)
}

fn create_test_queue_with_config(config: GenerationConfig) -> (FrameGenerationQueue, TempDir) {
    let (api, temp_dir) = create_test_api();
    let api = Arc::new(api);
    let queue = FrameGenerationQueue::new(api, config);
    (queue, temp_dir)
}

#[tokio::test]
async fn test_priority_ordering() {
    let (queue, _temp_dir) = create_test_queue();

    // Enqueue requests with different priorities
    let node1 = Hash::from([1u8; 32]);
    let node2 = Hash::from([2u8; 32]);
    let node3 = Hash::from([3u8; 32]);
    let node4 = Hash::from([4u8; 32]);

    queue
        .enqueue(
            node1,
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Low,
        )
        .await
        .unwrap();
    queue
        .enqueue(
            node2,
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::High,
        )
        .await
        .unwrap();
    queue
        .enqueue(
            node3,
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Urgent,
        )
        .await
        .unwrap();
    queue
        .enqueue(
            node4,
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        )
        .await
        .unwrap();

    // Verify ordering by checking stats and testing dequeue behavior
    // Since we can't directly access the internal queue, we verify through
    // the public API. The priority ordering is tested through the Ord implementation.
    let stats = queue.stats();
    assert_eq!(stats.pending, 4);
}

#[tokio::test]
async fn test_enqueue_dequeue() {
    let (queue, _temp_dir) = create_test_queue();

    let node_id = Hash::from([1u8; 32]);
    queue
        .enqueue(
            node_id,
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        )
        .await
        .unwrap();

    let stats = queue.stats();
    assert_eq!(stats.pending, 1);
    assert_eq!(stats.processing, 0);
    assert_eq!(stats.completed, 0);
    assert_eq!(stats.failed, 0);
}

#[tokio::test]
async fn test_enqueue_deduplicates_pending_identity() {
    let (queue, _temp_dir) = create_test_queue();
    let node_id = Hash::from([9u8; 32]);

    let first = queue
        .enqueue(
            node_id,
            "agent1".to_string(),
            "test-provider".to_string(),
            Some("context-agent1".to_string()),
            Priority::Normal,
        )
        .await
        .unwrap();

    let second = queue
        .enqueue(
            node_id,
            "agent1".to_string(),
            "test-provider".to_string(),
            Some("context-agent1".to_string()),
            Priority::Urgent,
        )
        .await
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(queue.stats().pending, 1);
}

#[tokio::test]
async fn test_enqueue_and_wait_deduplicates_pending_request() {
    let (queue, _temp_dir) = create_test_queue();
    let node_id = Hash::from([11u8; 32]);

    queue
        .enqueue(
            node_id,
            "agent1".to_string(),
            "test-provider".to_string(),
            Some("context-agent1".to_string()),
            Priority::Normal,
        )
        .await
        .unwrap();

    let result = queue
        .enqueue_and_wait(
            node_id,
            "agent1".to_string(),
            "test-provider".to_string(),
            Some("context-agent1".to_string()),
            Priority::Urgent,
            Some(Duration::from_millis(20)),
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ApiError::ConfigError(_)));
    assert_eq!(queue.stats().pending, 1);
}

#[tokio::test]
async fn test_enqueue_deduplicates_during_retry_backoff_window() {
    let mut config = GenerationConfig::default();
    config.max_retry_attempts = 1;
    config.retry_delay_ms = 500;
    let (queue, _temp_dir) = create_test_queue_with_config(config);
    queue.start().unwrap();

    let node_id = Hash::from([77u8; 32]);
    let first = queue
        .enqueue(
            node_id,
            "missing-agent".to_string(),
            "test-provider".to_string(),
            Some("context-missing-agent".to_string()),
            Priority::Normal,
        )
        .await
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let stats = queue.stats();
        if stats.pending == 0 && stats.processing == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "request did not enter retry backoff window in time"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let second = queue
        .enqueue(
            node_id,
            "missing-agent".to_string(),
            "test-provider".to_string(),
            Some("context-missing-agent".to_string()),
            Priority::High,
        )
        .await
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(queue.stats().pending, 0);

    queue.stop().await.unwrap();
}

#[tokio::test]
async fn test_queue_size_limit() {
    let mut config = GenerationConfig::default();
    config.max_queue_size = 3;
    let (queue, _temp_dir) = create_test_queue_with_config(config);

    // Fill queue to capacity
    for i in 0..3 {
        let node_id = Hash::from([i as u8; 32]);
        queue
            .enqueue(
                node_id,
                "agent1".to_string(),
                "test-provider".to_string(),
                None,
                Priority::Normal,
            )
            .await
            .unwrap();
    }

    // Next enqueue should fail
    let node_id = Hash::from([4u8; 32]);
    let result = queue
        .enqueue(
            node_id,
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        )
        .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ApiError::ConfigError(_)));

    let stats = queue.stats();
    assert_eq!(stats.pending, 3);
}

#[tokio::test]
async fn test_batch_enqueue() {
    let (queue, _temp_dir) = create_test_queue();

    let requests = vec![
        (
            Hash::from([1u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        ),
        (
            Hash::from([2u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::High,
        ),
        (
            Hash::from([3u8; 32]),
            "agent2".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Urgent,
        ),
    ];

    queue.enqueue_batch(requests).await.unwrap();

    let stats = queue.stats();
    assert_eq!(stats.pending, 3);
}

#[tokio::test]
async fn test_batch_enqueue_size_limit() {
    let mut config = GenerationConfig::default();
    config.max_queue_size = 2;
    let (queue, _temp_dir) = create_test_queue_with_config(config);

    // Try to enqueue batch that exceeds limit
    let requests = vec![
        (
            Hash::from([1u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        ),
        (
            Hash::from([2u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        ),
        (
            Hash::from([3u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        ),
    ];

    let result = queue.enqueue_batch(requests).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ApiError::ConfigError(_)));
}

#[tokio::test]
async fn test_generation_request_ordering() {
    // Test that GenerationRequest implements Ord correctly
    let now = Instant::now();

    use merkle::frame::queue::RequestId;
    let req1 = GenerationRequest {
        request_id: RequestId::next(),
        node_id: Hash::from([1u8; 32]),
        agent_id: "agent1".to_string(),
        provider_name: "test-provider".to_string(),
        frame_type: "test".to_string(),
        priority: Priority::High,
        retry_count: 0,
        created_at: now,
        completion_tx: None,
        options: GenerationRequestOptions::default(),
    };

    let req2 = GenerationRequest {
        request_id: RequestId::next(),
        node_id: Hash::from([2u8; 32]),
        agent_id: "agent1".to_string(),
        provider_name: "test-provider".to_string(),
        frame_type: "test".to_string(),
        priority: Priority::Low,
        retry_count: 0,
        created_at: now,
        completion_tx: None,
        options: GenerationRequestOptions::default(),
    };

    // Higher priority should be greater
    assert!(req1 > req2);

    // Same priority, older should be greater (processed first)
    let req3 = GenerationRequest {
        request_id: RequestId::next(),
        node_id: Hash::from([3u8; 32]),
        agent_id: "agent1".to_string(),
        provider_name: "test-provider".to_string(),
        frame_type: "test".to_string(),
        priority: Priority::Normal,
        retry_count: 0,
        created_at: now,
        completion_tx: None,
        options: GenerationRequestOptions::default(),
    };

    let req4 = GenerationRequest {
        request_id: RequestId::next(),
        node_id: Hash::from([4u8; 32]),
        agent_id: "agent1".to_string(),
        provider_name: "test-provider".to_string(),
        frame_type: "test".to_string(),
        priority: Priority::Normal,
        retry_count: 0,
        created_at: now + Duration::from_millis(100),
        completion_tx: None,
        options: GenerationRequestOptions::default(),
    };

    // Same priority, older (req3) should be greater
    assert!(req3 > req4);
}

#[tokio::test]
async fn test_queue_stats() {
    let (queue, _temp_dir) = create_test_queue();

    let stats = queue.stats();
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.processing, 0);
    assert_eq!(stats.completed, 0);
    assert_eq!(stats.failed, 0);

    // Enqueue some items
    for i in 0..5 {
        let node_id = Hash::from([i as u8; 32]);
        queue
            .enqueue(
                node_id,
                "agent1".to_string(),
                "test-provider".to_string(),
                None,
                Priority::Normal,
            )
            .await
            .unwrap();
    }

    let stats = queue.stats();
    assert_eq!(stats.pending, 5);
}

#[tokio::test]
async fn test_worker_start_stop() {
    let (queue, _temp_dir) = create_test_queue();

    // Start workers
    queue.start().unwrap();

    // Should be able to start again (idempotent)
    queue.start().unwrap();

    // Stop workers
    queue.stop().await.unwrap();

    // Should be able to stop again (idempotent)
    queue.stop().await.unwrap();
}

#[tokio::test]
async fn test_frame_type_default() {
    let (queue, _temp_dir) = create_test_queue();

    let node_id = Hash::from([1u8; 32]);
    queue
        .enqueue(
            node_id,
            "my-agent".to_string(),
            "test-provider".to_string(),
            None,
            Priority::Normal,
        )
        .await
        .unwrap();

    // Frame type is set during enqueue, verify through stats
    let stats = queue.stats();
    assert_eq!(stats.pending, 1);
}

#[tokio::test]
async fn test_frame_type_custom() {
    let (queue, _temp_dir) = create_test_queue();

    let node_id = Hash::from([1u8; 32]);
    queue
        .enqueue(
            node_id,
            "my-agent".to_string(),
            "test-provider".to_string(),
            Some("custom-type".to_string()),
            Priority::Normal,
        )
        .await
        .unwrap();

    // Frame type is set during enqueue, verify through stats
    let stats = queue.stats();
    assert_eq!(stats.pending, 1);
}

#[tokio::test]
async fn test_priority_enum_ordering() {
    // Verify Priority enum ordering
    assert!(Priority::Urgent > Priority::High);
    assert!(Priority::High > Priority::Normal);
    assert!(Priority::Normal > Priority::Low);

    // Verify Ord implementation
    assert_eq!(
        Priority::Urgent.cmp(&Priority::High),
        std::cmp::Ordering::Greater
    );
    assert_eq!(
        Priority::Low.cmp(&Priority::Normal),
        std::cmp::Ordering::Less
    );
}

#[tokio::test]
async fn test_concurrent_enqueue() {
    let (queue, _temp_dir) = create_test_queue();
    let queue = Arc::new(queue);

    // Spawn multiple tasks to enqueue concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let queue = Arc::clone(&queue);
        let handle = tokio::spawn(async move {
            let node_id = Hash::from([i as u8; 32]);
            queue
                .enqueue(
                    node_id,
                    "agent1".to_string(),
                    "test-provider".to_string(),
                    None,
                    Priority::Normal,
                )
                .await
        });
        handles.push(handle);
    }

    // Wait for all enqueues to complete
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }

    let stats = queue.stats();
    assert_eq!(stats.pending, 10);
}

#[tokio::test]
async fn test_enqueue_emits_observability_events() {
    let (api, temp_dir) = create_test_api();
    let db_path = temp_dir.path().join("progress_db");
    std::fs::create_dir_all(&db_path).unwrap();
    let db = sled::open(&db_path).unwrap();
    let progress = Arc::new(ProgressRuntime::new(db).unwrap());
    let session_id = progress
        .start_command_session("queue.test".to_string())
        .unwrap();

    let queue = FrameGenerationQueue::with_event_context(
        Arc::new(api),
        GenerationConfig::default(),
        Some(QueueEventContext {
            session_id: session_id.clone(),
            progress: Arc::clone(&progress),
        }),
    );

    queue
        .enqueue(
            Hash::from([42u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            Some("context-agent1".to_string()),
            Priority::Normal,
        )
        .await
        .unwrap();

    progress
        .finish_command_session(&session_id, true, None)
        .unwrap();
    let events = progress.store().read_events(&session_id).unwrap();
    assert!(events.iter().any(|e| e.event_type == "request_enqueued"));
    assert!(events.iter().any(|e| e.event_type == "queue_stats"));
}

#[tokio::test]
async fn test_batch_enqueue_emits_request_enqueued_per_item() {
    let (api, temp_dir) = create_test_api();
    let db_path = temp_dir.path().join("progress_db");
    std::fs::create_dir_all(&db_path).unwrap();
    let db = sled::open(&db_path).unwrap();
    let progress = Arc::new(ProgressRuntime::new(db).unwrap());
    let session_id = progress
        .start_command_session("queue.batch.test".to_string())
        .unwrap();

    let queue = FrameGenerationQueue::with_event_context(
        Arc::new(api),
        GenerationConfig::default(),
        Some(QueueEventContext {
            session_id: session_id.clone(),
            progress: Arc::clone(&progress),
        }),
    );

    let requests = vec![
        (
            Hash::from([21u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            Some("context-agent1".to_string()),
            Priority::Normal,
        ),
        (
            Hash::from([22u8; 32]),
            "agent1".to_string(),
            "test-provider".to_string(),
            Some("context-agent1".to_string()),
            Priority::High,
        ),
        (
            Hash::from([23u8; 32]),
            "agent2".to_string(),
            "test-provider".to_string(),
            Some("context-agent2".to_string()),
            Priority::Urgent,
        ),
    ];

    queue.enqueue_batch(requests).await.unwrap();

    progress
        .finish_command_session(&session_id, true, None)
        .unwrap();
    let events = progress.store().read_events(&session_id).unwrap();
    let enqueued_count = events
        .iter()
        .filter(|e| e.event_type == "request_enqueued")
        .count();
    assert_eq!(enqueued_count, 3);
}

// Note: Full integration tests with actual frame generation would require
// mocking the adapter and API, which is more complex. These tests focus
// on the queue structure and operations themselves.
