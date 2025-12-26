//! Integration tests for tree structure correctness

use merkle::tree::builder::TreeBuilder;
use merkle::tree::node::MerkleNode;
use std::fs;
use tempfile::TempDir;

/// Test that tree contains all files
#[test]
fn test_tree_contains_all_files() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Count file nodes
    let file_count = tree
        .nodes
        .values()
        .filter(|node| matches!(node, MerkleNode::File(_)))
        .count();

    assert_eq!(file_count, 2);
}

/// Test that tree contains all directories
#[test]
fn test_tree_contains_all_directories() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::create_dir(root.join("dir1")).unwrap();
    fs::create_dir(root.join("dir2")).unwrap();
    fs::write(root.join("dir1").join("file.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Count directory nodes (should include root + dir1 + dir2)
    let dir_count = tree
        .nodes
        .values()
        .filter(|node| matches!(node, MerkleNode::Directory(_)))
        .count();

    assert!(dir_count >= 3); // At least root + dir1 + dir2
}

/// Test that root node exists and is a directory
#[test]
fn test_root_is_directory() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Root should exist
    let root_node = tree.nodes.get(&tree.root_id).unwrap();
    assert!(matches!(root_node, MerkleNode::Directory(_)));
}

/// Test that directory children are correctly linked
#[test]
fn test_directory_children_linked() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Get root directory node
    let root_node = tree.nodes.get(&tree.root_id).unwrap();
    if let MerkleNode::Directory(dir_node) = root_node {
        // Root should have 2 children (file1.txt and file2.txt)
        assert_eq!(dir_node.children.len(), 2);

        // All children should exist in tree
        for (_, child_node_id) in &dir_node.children {
            assert!(tree.nodes.contains_key(child_node_id));
        }
    } else {
        panic!("Root should be a directory");
    }
}

/// Test that nested directory structure is correct
#[test]
fn test_nested_directory_structure() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::create_dir(root.join("dir1")).unwrap();
    fs::write(root.join("dir1").join("file.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Find dir1 node
    let dir1_node_id = tree
        .nodes
        .iter()
        .find_map(|(id, node)| {
            if let MerkleNode::Directory(dir) = node {
                if dir.path.ends_with("dir1") {
                    return Some(*id);
                }
            }
            None
        })
        .unwrap();

    // dir1 should have 1 child (file.txt)
    let dir1_node = tree.nodes.get(&dir1_node_id).unwrap();
    if let MerkleNode::Directory(dir_node) = dir1_node {
        assert_eq!(dir_node.children.len(), 1);
    } else {
        panic!("dir1 should be a directory");
    }
}

/// Test that file nodes have correct content hashes
#[test]
fn test_file_content_hashes() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    let content = b"test content";
    fs::write(root.join("test.txt"), content).unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Find file node
    let file_node = tree
        .nodes
        .values()
        .find_map(|node| {
            if let MerkleNode::File(file) = node {
                if file.path.ends_with("test.txt") {
                    return Some(file);
                }
            }
            None
        })
        .unwrap();

    // Content hash should match
    let expected_hash = merkle::tree::hasher::compute_content_hash(content);
    assert_eq!(file_node.content_hash, expected_hash);
    assert_eq!(file_node.size, content.len() as u64);
}

/// Test that children are sorted by name
#[test]
fn test_children_sorted_by_name() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    // Create files in non-alphabetical order
    fs::write(root.join("z_file.txt"), "content").unwrap();
    fs::write(root.join("a_file.txt"), "content").unwrap();
    fs::write(root.join("m_file.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Get root directory node
    let root_node = tree.nodes.get(&tree.root_id).unwrap();
    if let MerkleNode::Directory(dir_node) = root_node {
        // Children should be sorted
        let names: Vec<_> = dir_node.children.iter().map(|(name, _)| name).collect();
        let mut sorted_names = names.clone();
        sorted_names.sort();
        assert_eq!(names, sorted_names);
    }
}
