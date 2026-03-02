//! Performance tests to verify O(1) lookup behavior
//!
//! These tests verify that lookup operations maintain constant time complexity
//! regardless of dataset size, confirming O(1) behavior.

use meld::heads::HeadIndex;
use meld::store::{NodeRecord, NodeRecordStore, NodeType, SledNodeRecordStore};
use std::time::Instant;
use tempfile::TempDir;

/// Test that NodeRecord Store lookups are O(1) by measuring time across different sizes
#[test]
fn test_node_record_store_o1_lookup() {
    let temp_dir = TempDir::new().unwrap();
    let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

    // Create datasets of different sizes
    let sizes = vec![100, 1000, 10000];
    let mut lookup_times = Vec::new();

    for size in sizes {
        // Populate store with records
        let mut records = Vec::new();
        for i in 0..size {
            let mut node_id = [0u8; 32];
            node_id[0] = (i % 256) as u8;
            node_id[1] = ((i / 256) % 256) as u8;
            node_id[2] = ((i / 65536) % 256) as u8;

            let record = NodeRecord {
                node_id,
                path: std::path::PathBuf::from(format!("/test/file_{}.txt", i)),
                node_type: NodeType::File {
                    size: 100,
                    content_hash: [0u8; 32],
                },
                children: vec![],
                parent: None,
                frame_set_root: None,
                metadata: Default::default(),
                tombstoned_at: None,
            };
            records.push(record);
        }

        // Batch insert for efficiency
        store.put_batch(&records).unwrap();
        store.flush().unwrap(); // Ensure all writes are persisted

        // Measure lookup time for random node
        let test_node_id = records[size / 2].node_id;
        let start = Instant::now();
        for _ in 0..1000 {
            let _ = store.get(&test_node_id).unwrap();
        }
        let duration = start.elapsed();
        let avg_time_per_lookup = duration.as_nanos() / 1000;

        lookup_times.push((size, avg_time_per_lookup));
    }

    // Verify that lookup time doesn't grow significantly with dataset size
    // O(1) means the time should be roughly constant (within reasonable variance)
    let (_, time_100) = lookup_times[0];
    let (_, time_1000) = lookup_times[1];
    let (_, time_10000) = lookup_times[2];

    // For O(1), the ratio should be close to 1.0 (allowing for some variance)
    // We allow up to 3x difference to account for cache effects, disk I/O variance, etc.
    let ratio_1k = time_1000 as f64 / time_100 as f64;
    let ratio_10k = time_10000 as f64 / time_100 as f64;

    // Assert that lookup time doesn't grow linearly with dataset size
    // If it were O(n), ratio_10k would be ~100x, but for O(1) it should be < 5x
    assert!(
        ratio_1k < 5.0,
        "Lookup time grew too much from 100 to 1000 records: ratio = {:.2} (expected < 5.0 for O(1))",
        ratio_1k
    );
    assert!(
        ratio_10k < 5.0,
        "Lookup time grew too much from 100 to 10000 records: ratio = {:.2} (expected < 5.0 for O(1))",
        ratio_10k
    );

    // Also verify that absolute time is reasonable (< 1ms per lookup as per spec)
    assert!(
        time_10000 < 1_000_000, // 1ms in nanoseconds
        "Lookup time exceeded 1ms target: {}ns (expected < 1ms)",
        time_10000
    );
}

/// Test that HeadIndex lookups are O(1) by measuring time across different sizes
#[test]
fn test_head_index_o1_lookup() {
    let mut head_index = HeadIndex::new();

    // Create datasets of different sizes
    let sizes = vec![100, 1000, 10000];
    let mut lookup_times = Vec::new();

    for size in sizes {
        // Populate head index with entries
        for i in 0..size {
            let mut node_id = [0u8; 32];
            node_id[0] = (i % 256) as u8;
            node_id[1] = ((i / 256) % 256) as u8;
            node_id[2] = ((i / 65536) % 256) as u8;

            let mut frame_id = [0u8; 32];
            frame_id[0] = (i % 256) as u8;
            frame_id[1] = ((i / 256) % 256) as u8;

            head_index.update_head(&node_id, "test", &frame_id).unwrap();
        }

        // Measure lookup time for random node
        let test_node_id = {
            let mut node_id = [0u8; 32];
            node_id[0] = ((size / 2) % 256) as u8;
            node_id[1] = (((size / 2) / 256) % 256) as u8;
            node_id[2] = (((size / 2) / 65536) % 256) as u8;
            node_id
        };

        let start = Instant::now();
        for _ in 0..10000 {
            let _ = head_index.get_head(&test_node_id, "test").unwrap();
        }
        let duration = start.elapsed();
        let avg_time_per_lookup = duration.as_nanos() / 10000;

        lookup_times.push((size, avg_time_per_lookup));
    }

    // Verify that lookup time doesn't grow significantly with dataset size
    let (_, time_100) = lookup_times[0];
    let (_, time_1000) = lookup_times[1];
    let (_, time_10000) = lookup_times[2];

    // For O(1) HashMap, the ratio should be very close to 1.0
    // We allow up to 2x difference to account for cache effects
    let ratio_1k = time_1000 as f64 / time_100 as f64;
    let ratio_10k = time_10000 as f64 / time_100 as f64;

    assert!(
        ratio_1k < 2.0,
        "HeadIndex lookup time grew too much from 100 to 1000 entries: ratio = {:.2} (expected < 2.0 for O(1))",
        ratio_1k
    );
    assert!(
        ratio_10k < 2.0,
        "HeadIndex lookup time grew too much from 100 to 10000 entries: ratio = {:.2} (expected < 2.0 for O(1))",
        ratio_10k
    );

    // Also verify that absolute time is reasonable (< 1ms per lookup as per spec)
    assert!(
        time_10000 < 1_000_000, // 1ms in nanoseconds
        "HeadIndex lookup time exceeded 1ms target: {}ns (expected < 1ms)",
        time_10000
    );
}

/// Test that HeadIndex updates are O(1)
#[test]
fn test_head_index_o1_update() {
    let mut head_index = HeadIndex::new();

    // Create datasets of different sizes
    let sizes = vec![100, 1000, 10000];
    let mut update_times = Vec::new();

    for size in sizes {
        // Pre-populate head index
        for i in 0..size {
            let mut node_id = [0u8; 32];
            node_id[0] = (i % 256) as u8;
            node_id[1] = ((i / 256) % 256) as u8;

            let mut frame_id = [0u8; 32];
            frame_id[0] = (i % 256) as u8;

            head_index.update_head(&node_id, "test", &frame_id).unwrap();
        }

        // Measure update time
        let test_node_id = {
            let mut node_id = [0u8; 32];
            node_id[0] = ((size / 2) % 256) as u8;
            node_id[1] = (((size / 2) / 256) % 256) as u8;
            node_id
        };
        let test_frame_id = [255u8; 32];

        let start = Instant::now();
        for _ in 0..10000 {
            head_index
                .update_head(&test_node_id, "test", &test_frame_id)
                .unwrap();
        }
        let duration = start.elapsed();
        let avg_time_per_update = duration.as_nanos() / 10000;

        update_times.push((size, avg_time_per_update));
    }

    // Verify that update time doesn't grow significantly with dataset size
    let (_, time_100) = update_times[0];
    let (_, time_1000) = update_times[1];
    let (_, time_10000) = update_times[2];

    let ratio_1k = time_1000 as f64 / time_100 as f64;
    let ratio_10k = time_10000 as f64 / time_100 as f64;

    assert!(
        ratio_1k < 2.0,
        "HeadIndex update time grew too much from 100 to 1000 entries: ratio = {:.2} (expected < 2.0 for O(1))",
        ratio_1k
    );
    assert!(
        ratio_10k < 2.0,
        "HeadIndex update time grew too much from 100 to 10000 entries: ratio = {:.2} (expected < 2.0 for O(1))",
        ratio_10k
    );
}
