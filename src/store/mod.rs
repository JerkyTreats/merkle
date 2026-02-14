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
    /// Timestamp when this node was tombstoned (Unix seconds), or None if active.
    pub tombstoned_at: Option<u64>,
}

/// NodeRecord Store interface
pub trait NodeRecordStore {
    fn get(&self, node_id: &NodeID) -> Result<Option<NodeRecord>, StorageError>;
    fn put(&self, record: &NodeRecord) -> Result<(), StorageError>;

    /// Find a node record by its canonicalized path
    ///
    /// Returns the NodeRecord if found, None if the path is not in the tree.
    fn find_by_path(&self, path: &Path) -> Result<Option<NodeRecord>, StorageError>;

    /// List all node records in the store.
    ///
    /// Used for status (total count, path breakdown, top paths). Path mappings
    /// (e.g. path:...) are not returned; only node records keyed by NodeID.
    fn list_all(&self) -> Result<Vec<NodeRecord>, StorageError>;

    /// List node records that are not tombstoned (active only).
    fn list_active(&self) -> Result<Vec<NodeRecord>, StorageError>;

    /// Get node record by path, including tombstoned nodes.
    /// Used for restore path resolution. Path key is only removed on purge.
    fn get_by_path(&self, path: &Path) -> Result<Option<NodeRecord>, StorageError>;

    /// Mark a node as tombstoned. Sets tombstoned_at to current timestamp.
    /// Does not tombstone descendants; caller is responsible for cascade.
    fn tombstone(&self, node_id: &NodeID) -> Result<NodeRecord, StorageError>;

    /// Remove tombstone marker from a node (restore).
    fn restore(&self, node_id: &NodeID) -> Result<NodeRecord, StorageError>;

    /// Permanently remove a tombstoned node record (compaction).
    /// Only succeeds if node is tombstoned and tombstoned_at is older than cutoff.
    fn purge(&self, node_id: &NodeID, cutoff: u64) -> Result<(), StorageError>;

    /// List all tombstoned node IDs, optionally filtered by age (older_than timestamp).
    fn list_tombstoned(&self, older_than: Option<u64>) -> Result<Vec<NodeID>, StorageError>;

    /// Flush any buffered writes to disk. Default implementation is a no-op.
    fn flush(&self) -> Result<(), StorageError> {
        Ok(())
    }
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
                    metadata: file
                        .metadata
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    tombstoned_at: None,
                })
            }
            MerkleNode::Directory(dir) => {
                let children: Vec<NodeID> =
                    dir.children.iter().map(|(_, node_id)| *node_id).collect();

                Ok(NodeRecord {
                    node_id,
                    path: dir.path.clone(),
                    node_type: NodeType::Directory,
                    children,
                    parent: tree.find_parent(&node_id),
                    frame_set_root: None,
                    metadata: dir
                        .metadata
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    tombstoned_at: None,
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
