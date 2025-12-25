//! Integration tests for tree building determinism

use merkle::tree::builder::TreeBuilder;
use std::fs;
use tempfile::TempDir;

/// Test that the same filesystem produces the same root hash
#[test]
fn test_same_filesystem_same_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    // Create test files
    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();
    fs::create_dir(root.join("dir1")).unwrap();
    fs::write(root.join("dir1").join("file3.txt"), "content3").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let root1 = builder.compute_root().unwrap();
    let root2 = builder.compute_root().unwrap();

    // Same filesystem should produce same root
    assert_eq!(root1, root2);
}

/// Test that file content changes produce different root hashes
#[test]
fn test_file_content_change_different_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("test.txt"), "content1").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let root1 = builder.compute_root().unwrap();

    // Change file content
    fs::write(root.join("test.txt"), "content2").unwrap();

    let root2 = builder.compute_root().unwrap();

    // Different content should produce different root
    assert_ne!(root1, root2);
}

/// Test that file addition produces different root hash
#[test]
fn test_file_addition_different_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file1.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let root1 = builder.compute_root().unwrap();

    // Add another file
    fs::write(root.join("file2.txt"), "content").unwrap();

    let root2 = builder.compute_root().unwrap();

    // Different structure should produce different root
    assert_ne!(root1, root2);
}

/// Test that directory addition produces different root hash
#[test]
fn test_directory_addition_different_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file1.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let root1 = builder.compute_root().unwrap();

    // Add a directory
    fs::create_dir(root.join("dir1")).unwrap();
    fs::write(root.join("dir1").join("file2.txt"), "content").unwrap();

    let root2 = builder.compute_root().unwrap();

    // Different structure should produce different root
    assert_ne!(root1, root2);
}

/// Test that file deletion produces different root hash
#[test]
fn test_file_deletion_different_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let root1 = builder.compute_root().unwrap();

    // Delete a file
    fs::remove_file(root.join("file2.txt")).unwrap();

    let root2 = builder.compute_root().unwrap();

    // Different structure should produce different root
    assert_ne!(root1, root2);
}

/// Test that re-ingestion produces same root hash
#[test]
fn test_reingestion_same_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    // Create filesystem
    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();
    fs::create_dir(root.join("dir1")).unwrap();
    fs::write(root.join("dir1").join("file3.txt"), "content3").unwrap();

    let builder1 = TreeBuilder::new(root.clone());
    let root1 = builder1.compute_root().unwrap();

    // Re-ingest (rebuild tree)
    let builder2 = TreeBuilder::new(root.clone());
    let root2 = builder2.compute_root().unwrap();

    // Should produce same root
    assert_eq!(root1, root2);
}

/// Test that empty directory produces consistent root
#[test]
fn test_empty_directory_consistent() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    // Create empty directory structure
    fs::create_dir(root.join("empty_dir")).unwrap();

    let builder = TreeBuilder::new(root.clone());
    let root1 = builder.compute_root().unwrap();
    let root2 = builder.compute_root().unwrap();

    // Should be consistent
    assert_eq!(root1, root2);
}
