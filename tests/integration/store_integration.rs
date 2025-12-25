//! Integration tests for NodeRecord Store

use merkle::store::{NodeRecord, NodeRecordStore, NodeType, SledNodeRecordStore};
use merkle::tree::builder::TreeBuilder;
use std::fs;
use tempfile::TempDir;

/// Test populating store from tree
#[test]
fn test_populate_store_from_tree() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    // Create test filesystem
    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();
    fs::create_dir(root.join("dir1")).unwrap();
    fs::write(root.join("dir1").join("file3.txt"), "content3").unwrap();

    // Build tree
    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    // Create store and populate
    let store_dir = TempDir::new().unwrap();
    let store = SledNodeRecordStore::new(store_dir.path()).unwrap();
    NodeRecord::populate_store_from_tree(&store, &tree).unwrap();

    // Verify all nodes are stored
    for (node_id, _) in &tree.nodes {
        let record = store.get(node_id).unwrap();
        assert!(record.is_some(), "Node {:?} should be in store", node_id);
    }
}

/// Test that file nodes are stored correctly
#[test]
fn test_file_node_stored_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("test.txt"), "test content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    let store_dir = TempDir::new().unwrap();
    let store = SledNodeRecordStore::new(store_dir.path()).unwrap();
    NodeRecord::populate_store_from_tree(&store, &tree).unwrap();

    // Find file node
    let file_node_id = tree
        .nodes
        .iter()
        .find_map(|(id, node)| {
            if let merkle::tree::node::MerkleNode::File(file) = node {
                if file.path.ends_with("test.txt") {
                    return Some(*id);
                }
            }
            None
        })
        .unwrap();

    let record = store.get(&file_node_id).unwrap().unwrap();
    assert!(matches!(record.node_type, NodeType::File { .. }));
    if let NodeType::File { size, content_hash: _ } = record.node_type {
        assert_eq!(size, "test content".len() as u64);
    }
}

/// Test that directory nodes are stored correctly
#[test]
fn test_directory_node_stored_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::create_dir(root.join("dir1")).unwrap();
    fs::write(root.join("dir1").join("file.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    let store_dir = TempDir::new().unwrap();
    let store = SledNodeRecordStore::new(store_dir.path()).unwrap();
    NodeRecord::populate_store_from_tree(&store, &tree).unwrap();

    // Find directory node
    let dir_node_id = tree
        .nodes
        .iter()
        .find_map(|(id, node)| {
            if let merkle::tree::node::MerkleNode::Directory(dir) = node {
                if dir.path.ends_with("dir1") {
                    return Some(*id);
                }
            }
            None
        })
        .unwrap();

    let record = store.get(&dir_node_id).unwrap().unwrap();
    assert!(matches!(record.node_type, NodeType::Directory));
    assert_eq!(record.children.len(), 1); // Should have one child (file.txt)
}

/// Test parent-child relationships
#[test]
fn test_parent_child_relationships() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    let store_dir = TempDir::new().unwrap();
    let store = SledNodeRecordStore::new(store_dir.path()).unwrap();
    NodeRecord::populate_store_from_tree(&store, &tree).unwrap();

    // Get root node
    let root_record = store.get(&tree.root_id).unwrap().unwrap();
    assert_eq!(root_record.children.len(), 2); // Should have 2 children

    // Verify children have root as parent
    for child_id in &root_record.children {
        let child_record = store.get(child_id).unwrap().unwrap();
        assert_eq!(child_record.parent, Some(tree.root_id));
    }
}

/// Test that root node has no parent
#[test]
fn test_root_node_no_parent() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    let store_dir = TempDir::new().unwrap();
    let store = SledNodeRecordStore::new(store_dir.path()).unwrap();
    NodeRecord::populate_store_from_tree(&store, &tree).unwrap();

    let root_record = store.get(&tree.root_id).unwrap().unwrap();
    assert_eq!(root_record.parent, None);
}

/// Test batch operations
#[test]
fn test_batch_operations() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file1.txt"), "content1").unwrap();
    fs::write(root.join("file2.txt"), "content2").unwrap();
    fs::write(root.join("file3.txt"), "content3").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    let store_dir = TempDir::new().unwrap();
    let store = SledNodeRecordStore::new(store_dir.path()).unwrap();

    // Convert all nodes to records
    let mut records = Vec::new();
    for (node_id, node) in &tree.nodes {
        let record = NodeRecord::from_merkle_node(*node_id, node, &tree).unwrap();
        records.push(record);
    }

    // Batch insert
    store.put_batch(&records).unwrap();

    // Verify all records are stored
    for record in &records {
        let retrieved = store.get(&record.node_id).unwrap();
        assert!(retrieved.is_some());
    }
}

/// Test store persistence across restarts
#[test]
fn test_store_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();

    fs::write(root.join("file.txt"), "content").unwrap();

    let builder = TreeBuilder::new(root.clone());
    let tree = builder.build().unwrap();

    let store_dir = TempDir::new().unwrap();
    let store_path = store_dir.path().join("store.db");

    // First session: populate store
    {
        let store = SledNodeRecordStore::new(&store_path).unwrap();
        NodeRecord::populate_store_from_tree(&store, &tree).unwrap();
        store.flush().unwrap();
    }

    // Second session: verify data persisted
    {
        let store = SledNodeRecordStore::new(&store_path).unwrap();
        for (node_id, _) in &tree.nodes {
            let record = store.get(node_id).unwrap();
            assert!(record.is_some(), "Node should persist across restarts");
        }
    }
}
