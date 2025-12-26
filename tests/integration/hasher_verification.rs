//! Hasher Implementation Verification Tests
//!
//! Tests to verify that our hasher implementation correctly uses BLAKE3
//! and maintains determinism and correctness properties.

use merkle::tree::hasher;
use std::collections::BTreeMap;
use tempfile::TempDir;
use std::fs;

/// Test that content hash matches BLAKE3 directly
#[test]
fn test_content_hash_matches_blake3() {
    use blake3::Hasher as Blake3Hasher;

    let content = b"test content";

    // Our implementation
    let our_hash = hasher::compute_content_hash(content);

    // Direct BLAKE3
    let mut blake3_hasher = Blake3Hasher::new();
    blake3_hasher.update(content);
    let blake3_hash = *blake3_hasher.finalize().as_bytes();

    assert_eq!(our_hash, blake3_hash);
}

/// Test that file NodeID computation is deterministic across runs
#[test]
fn test_file_node_id_determinism() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    let content_hash = hasher::compute_content_hash(b"test content");
    let metadata = BTreeMap::new();

    // Compute multiple times
    let node_id1 = hasher::compute_file_node_id(&test_file, &content_hash, &metadata).unwrap();
    let node_id2 = hasher::compute_file_node_id(&test_file, &content_hash, &metadata).unwrap();
    let node_id3 = hasher::compute_file_node_id(&test_file, &content_hash, &metadata).unwrap();

    // All should be identical
    assert_eq!(node_id1, node_id2);
    assert_eq!(node_id2, node_id3);
}

/// Test that directory NodeID computation is deterministic
#[test]
fn test_directory_node_id_determinism() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("test_dir");
    fs::create_dir(&test_dir).unwrap();

    let children = vec![
        ("a.txt".to_string(), [1u8; 32]),
        ("b.txt".to_string(), [2u8; 32]),
        ("c.txt".to_string(), [3u8; 32]),
    ];
    let metadata = BTreeMap::new();

    // Compute multiple times
    let node_id1 = hasher::compute_directory_node_id(&test_dir, &children, &metadata).unwrap();
    let node_id2 = hasher::compute_directory_node_id(&test_dir, &children, &metadata).unwrap();
    let node_id3 = hasher::compute_directory_node_id(&test_dir, &children, &metadata).unwrap();

    // All should be identical
    assert_eq!(node_id1, node_id2);
    assert_eq!(node_id2, node_id3);
}

/// Test that NodeID changes when content changes
#[test]
fn test_file_node_id_content_sensitivity() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    let content_hash1 = hasher::compute_content_hash(b"content 1");
    let content_hash2 = hasher::compute_content_hash(b"content 2");
    let metadata = BTreeMap::new();

    let node_id1 = hasher::compute_file_node_id(&test_file, &content_hash1, &metadata).unwrap();
    let node_id2 = hasher::compute_file_node_id(&test_file, &content_hash2, &metadata).unwrap();

    // Different content should produce different NodeID
    assert_ne!(node_id1, node_id2);
}

/// Test that NodeID changes when path changes
#[test]
fn test_file_node_id_path_sensitivity() {
    let temp_dir = TempDir::new().unwrap();
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");

    fs::write(&file1, "same content").unwrap();
    fs::write(&file2, "same content").unwrap();

    let content_hash = hasher::compute_content_hash(b"same content");
    let metadata = BTreeMap::new();

    let node_id1 = hasher::compute_file_node_id(&file1, &content_hash, &metadata).unwrap();
    let node_id2 = hasher::compute_file_node_id(&file2, &content_hash, &metadata).unwrap();

    // Different paths should produce different NodeID even with same content
    assert_ne!(node_id1, node_id2);
}

/// Test that metadata affects NodeID
#[test]
fn test_file_node_id_metadata_sensitivity() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    let content_hash = hasher::compute_content_hash(b"test content");

    let mut metadata1 = BTreeMap::new();
    metadata1.insert("key1".to_string(), "value1".to_string());

    let mut metadata2 = BTreeMap::new();
    metadata2.insert("key2".to_string(), "value2".to_string());

    let node_id1 = hasher::compute_file_node_id(&test_file, &content_hash, &metadata1).unwrap();
    let node_id2 = hasher::compute_file_node_id(&test_file, &content_hash, &metadata2).unwrap();

    // Different metadata should produce different NodeID
    assert_ne!(node_id1, node_id2);
}

/// Test that children order affects directory NodeID
#[test]
fn test_directory_node_id_children_order_sensitivity() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("test_dir");
    fs::create_dir(&test_dir).unwrap();

    // Same children, different order
    let children1 = vec![
        ("a.txt".to_string(), [1u8; 32]),
        ("b.txt".to_string(), [2u8; 32]),
    ];

    let children2 = vec![
        ("b.txt".to_string(), [2u8; 32]),
        ("a.txt".to_string(), [1u8; 32]),
    ];

    let metadata = BTreeMap::new();

    let node_id1 = hasher::compute_directory_node_id(&test_dir, &children1, &metadata).unwrap();
    let node_id2 = hasher::compute_directory_node_id(&test_dir, &children2, &metadata).unwrap();

    // Different order should produce different NodeID
    // (This tests that we're hashing in the order provided, not sorting)
    // Note: In practice, children should be sorted before calling this function
    assert_ne!(node_id1, node_id2);
}

/// Test that empty directory produces consistent NodeID
#[test]
fn test_empty_directory_node_id() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("empty_dir");
    fs::create_dir(&test_dir).unwrap();

    let children = vec![];
    let metadata = BTreeMap::new();

    let node_id1 = hasher::compute_directory_node_id(&test_dir, &children, &metadata).unwrap();
    let node_id2 = hasher::compute_directory_node_id(&test_dir, &children, &metadata).unwrap();

    // Empty directory should produce consistent NodeID
    assert_eq!(node_id1, node_id2);
}

/// Test large file content hashing
#[test]
fn test_large_content_hashing() {
    // 10MB of data
    let large_content = vec![42u8; 10_000_000];

    let hash = hasher::compute_content_hash(&large_content);

    // Should complete without error
    assert_eq!(hash.len(), 32);

    // Should be deterministic
    let hash2 = hasher::compute_content_hash(&large_content);
    assert_eq!(hash, hash2);
}

/// Test that hash computation handles Unicode correctly
#[test]
fn test_unicode_content_hashing() {
    let unicode_content = "Hello, 世界! 🌍";

    let hash1 = hasher::compute_content_hash(unicode_content.as_bytes());
    let hash2 = hasher::compute_content_hash(unicode_content.as_bytes());

    // Unicode content should hash deterministically
    assert_eq!(hash1, hash2);
}
