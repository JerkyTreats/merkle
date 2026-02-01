//! NodeRecord Store
//!
//! Provides fast lookup storage for node metadata and relationships.
//! Acts as an index into the filesystem Merkle tree.

pub mod persistence;

pub use persistence::SledNodeRecordStore;

use crate::error::StorageError;
use crate::tree::node::MerkleNode;
use crate::tree::Tree;
use crate::types::{Hash, NodeID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Node type enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    File { size: u64, content_hash: [u8; 32] },
    Directory,
}

/// NodeRecord: Metadata and relationships for a filesystem node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRecord {
    pub node_id: NodeID,
    pub path: PathBuf,
    pub node_type: NodeType,
    pub children: Vec<NodeID>,
    pub parent: Option<NodeID>,
    pub frame_set_root: Option<Hash>,
    pub metadata: HashMap<String, String>,
}

/// NodeRecord Store interface
pub trait NodeRecordStore {
    fn get(&self, node_id: &NodeID) -> Result<Option<NodeRecord>, StorageError>;
    fn put(&self, record: &NodeRecord) -> Result<(), StorageError>;
    
    /// Find a node record by its canonicalized path
    ///
    /// Returns the NodeRecord if found, None if the path is not in the tree.
    fn find_by_path(&self, path: &Path) -> Result<Option<NodeRecord>, StorageError>;
}

impl NodeRecord {
    /// Convert a MerkleNode to a NodeRecord
    ///
    /// Requires the tree to look up parent relationships.
    pub fn from_merkle_node(
        node_id: NodeID,
        node: &MerkleNode,
        tree: &Tree,
    ) -> Result<Self, StorageError> {
        match node {
            MerkleNode::File(file) => {
                Ok(NodeRecord {
                    node_id,
                    path: file.path.clone(),
                    node_type: NodeType::File {
                        size: file.size,
                        content_hash: file.content_hash,
                    },
                    children: vec![], // Files have no children
                    parent: tree.find_parent(&node_id),
                    frame_set_root: None,
                    metadata: file.metadata.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                })
            }
            MerkleNode::Directory(dir) => {
                let children: Vec<NodeID> = dir.children.iter().map(|(_, node_id)| *node_id).collect();

                Ok(NodeRecord {
                    node_id,
                    path: dir.path.clone(),
                    node_type: NodeType::Directory,
                    children,
                    parent: tree.find_parent(&node_id),
                    frame_set_root: None,
                    metadata: dir.metadata.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                })
            }
        }
    }

    /// Populate a NodeRecordStore from a Tree
    ///
    /// Converts all nodes in the tree to NodeRecords and stores them.
    pub fn populate_store_from_tree(
        store: &dyn NodeRecordStore,
        tree: &Tree,
    ) -> Result<(), StorageError> {
        for (node_id, node) in &tree.nodes {
            let record = Self::from_merkle_node(*node_id, node, tree)?;
            store.put(&record)?;
        }
        Ok(())
    }
}
