//! Persistence layer for NodeRecord Store

use crate::error::StorageError;
use crate::store::{NodeRecord, NodeRecordStore};
use crate::types::NodeID;
use bincode;
use sled;
use std::path::Path;

/// Sled-based implementation of NodeRecordStore
pub struct SledNodeRecordStore {
    db: sled::Db,
}

impl SledNodeRecordStore {
    /// Create a new SledNodeRecordStore at the given path
    ///
    /// The path can be a directory (sled will create a database there) or
    /// a file path (sled will use it as the database file).
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let db = sled::open(path).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open sled database: {}", e),
            ))
        })?;
        Ok(Self { db })
    }

    /// Get the underlying sled database (for advanced operations)
    pub fn db(&self) -> &sled::Db {
        &self.db
    }
}

impl NodeRecordStore for SledNodeRecordStore {
    fn get(&self, node_id: &NodeID) -> Result<Option<NodeRecord>, StorageError> {
        let key = node_id.as_slice();
        match self.db.get(key).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to get node record: {}", e),
            ))
        })? {
            Some(value) => {
                let record: NodeRecord = bincode::deserialize(&value).map_err(|e| {
                    StorageError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to deserialize node record: {}", e),
                    ))
                })?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    fn put(&self, record: &NodeRecord) -> Result<(), StorageError> {
        let key = record.node_id.as_slice();
        let value = bincode::serialize(record).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize node record: {}", e),
            ))
        })?;

        self.db.insert(key, value).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to put node record: {}", e),
            ))
        })?;

        Ok(())
    }
}

impl SledNodeRecordStore {
    /// Check if a node exists in the store
    pub fn contains(&self, node_id: &NodeID) -> Result<bool, StorageError> {
        let key = node_id.as_slice();
        let exists = self.db.contains_key(key).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to check node existence: {}", e),
            ))
        })?;
        Ok(exists)
    }

    /// Batch insert multiple node records
    ///
    /// This is more efficient than calling `put()` multiple times.
    pub fn put_batch(&self, records: &[NodeRecord]) -> Result<(), StorageError> {
        let mut batch = sled::Batch::default();

        for record in records {
            let key = record.node_id.as_slice();
            let value = bincode::serialize(record).map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to serialize node record: {}", e),
                ))
            })?;
            batch.insert(key, value);
        }

        self.db.apply_batch(batch).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to apply batch: {}", e),
            ))
        })?;

        Ok(())
    }

    /// Flush all pending writes to disk
    pub fn flush(&self) -> Result<(), StorageError> {
        self.db.flush().map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to flush database: {}", e),
            ))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NodeType;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_store_and_retrieve() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let node_id = [1u8; 32];
        let record = NodeRecord {
            node_id,
            path: std::path::PathBuf::from("/test/file.txt"),
            node_type: NodeType::File {
                size: 100,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: HashMap::new(),
        };

        // Store
        store.put(&record).unwrap();

        // Retrieve
        let retrieved = store.get(&node_id).unwrap().unwrap();
        assert_eq!(retrieved.node_id, node_id);
        assert_eq!(retrieved.path, record.path);
    }

    #[test]
    fn test_get_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let node_id = [1u8; 32];
        let result = store.get(&node_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_contains() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let node_id = [1u8; 32];
        assert!(!store.contains(&node_id).unwrap());

        let record = NodeRecord {
            node_id,
            path: std::path::PathBuf::from("/test/file.txt"),
            node_type: NodeType::File {
                size: 100,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: HashMap::new(),
        };

        store.put(&record).unwrap();
        assert!(store.contains(&node_id).unwrap());
    }

    #[test]
    fn test_put_batch() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let records = vec![
            NodeRecord {
                node_id: [1u8; 32],
                path: std::path::PathBuf::from("/test/file1.txt"),
                node_type: NodeType::File {
                    size: 100,
                    content_hash: [2u8; 32],
                },
                children: vec![],
                parent: None,
                frame_set_root: None,
                metadata: HashMap::new(),
            },
            NodeRecord {
                node_id: [3u8; 32],
                path: std::path::PathBuf::from("/test/file2.txt"),
                node_type: NodeType::File {
                    size: 200,
                    content_hash: [4u8; 32],
                },
                children: vec![],
                parent: None,
                frame_set_root: None,
                metadata: HashMap::new(),
            },
        ];

        store.put_batch(&records).unwrap();

        // Verify both records are stored
        assert!(store.get(&[1u8; 32]).unwrap().is_some());
        assert!(store.get(&[3u8; 32]).unwrap().is_some());
    }

    #[test]
    fn test_update_existing() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let node_id = [1u8; 32];
        let record1 = NodeRecord {
            node_id,
            path: std::path::PathBuf::from("/test/file.txt"),
            node_type: NodeType::File {
                size: 100,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: HashMap::new(),
        };

        store.put(&record1).unwrap();

        // Update with new record
        let record2 = NodeRecord {
            node_id,
            path: std::path::PathBuf::from("/test/file_updated.txt"),
            node_type: NodeType::File {
                size: 200,
                content_hash: [3u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: HashMap::new(),
        };

        store.put(&record2).unwrap();

        // Verify update
        let retrieved = store.get(&node_id).unwrap().unwrap();
        assert_eq!(retrieved.path, record2.path);
        assert_eq!(retrieved.path, std::path::PathBuf::from("/test/file_updated.txt"));
    }
}
