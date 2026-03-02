//! Persistence layer for NodeRecord Store

use crate::error::StorageError;
use crate::store::{NodeRecord, NodeRecordStore};
use crate::types::NodeID;
use bincode;
use sled;
use std::path::Path;
use tracing::warn;

fn deserialize_node_record(bytes: &[u8]) -> Result<NodeRecord, StorageError> {
    bincode::deserialize(bytes).map_err(|e| {
        StorageError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to deserialize node record: {}", e),
        ))
    })
}

fn serialize_node_record(record: &NodeRecord) -> Result<Vec<u8>, StorageError> {
    bincode::serialize(record).map_err(|e| {
        StorageError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to serialize node record: {}", e),
        ))
    })
}

fn is_corrupt_node_record_error(err: &StorageError) -> bool {
    matches!(err, StorageError::IoError(io_err) if io_err.kind() == std::io::ErrorKind::InvalidData)
}

fn is_node_record_key(key: &[u8]) -> bool {
    // Path index keys are namespaced as "path:<canonical-path>" and can
    // coincidentally be 32 bytes long, so length alone is not sufficient.
    !key.starts_with(b"path:") && key.len() == 32
}

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

    /// Create a new SledNodeRecordStore from an existing sled database handle.
    pub fn from_db(db: sled::Db) -> Self {
        Self { db }
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
                let record = deserialize_node_record(&value)?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    fn put(&self, record: &NodeRecord) -> Result<(), StorageError> {
        let key = record.node_id.as_slice();
        let value = serialize_node_record(record)?;

        self.db.insert(key, value).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to put node record: {}", e),
            ))
        })?;

        // Store path-to-NodeID mapping for efficient path lookups
        // Use a prefix to separate path mappings from node records
        let path_key = format!("path:{}", record.path.to_string_lossy());
        let path_value = bincode::serialize(&record.node_id).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize node ID for path mapping: {}", e),
            ))
        })?;

        self.db
            .insert(path_key.as_bytes(), path_value)
            .map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to store path mapping: {}", e),
                ))
            })?;

        Ok(())
    }

    fn find_by_path(&self, path: &Path) -> Result<Option<NodeRecord>, StorageError> {
        let record = self.get_by_path(path)?;
        // Active-only: skip tombstoned nodes
        Ok(record.filter(|r| r.tombstoned_at.is_none()))
    }

    fn get_by_path(&self, path: &Path) -> Result<Option<NodeRecord>, StorageError> {
        let path_str = path.to_string_lossy();
        let path_key = format!("path:{}", path_str);

        match self.db.get(path_key.as_bytes()).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to get path mapping: {}", e),
            ))
        })? {
            Some(node_id_bytes) => {
                let node_id: NodeID = bincode::deserialize(&node_id_bytes).map_err(|e| {
                    StorageError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to deserialize node ID from path mapping: {}", e),
                    ))
                })?;
                self.get(&node_id)
            }
            None => Ok(None),
        }
    }

    fn list_all(&self) -> Result<Vec<NodeRecord>, StorageError> {
        let mut records = Vec::new();
        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to iterate store: {}", e),
                ))
            })?;
            if !is_node_record_key(key.as_ref()) {
                continue;
            }
            match deserialize_node_record(&value) {
                Ok(record) => records.push(record),
                Err(err) if is_corrupt_node_record_error(&err) => {
                    warn!(
                        key = %hex::encode(key.as_ref()),
                        error = %err,
                        "Skipping corrupt node record during store iteration"
                    );
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        Ok(records)
    }

    fn list_active(&self) -> Result<Vec<NodeRecord>, StorageError> {
        let records = self.list_all()?;
        Ok(records
            .into_iter()
            .filter(|r| r.tombstoned_at.is_none())
            .collect())
    }

    fn tombstone(&self, node_id: &NodeID) -> Result<NodeRecord, StorageError> {
        let mut record = self
            .get(node_id)?
            .ok_or_else(|| StorageError::InvalidPath("Node not found".to_string()))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?
            .as_secs();
        record.tombstoned_at = Some(now);
        self.put(&record)?;
        Ok(record)
    }

    fn restore(&self, node_id: &NodeID) -> Result<NodeRecord, StorageError> {
        let mut record = self
            .get(node_id)?
            .ok_or_else(|| StorageError::InvalidPath("Node not found".to_string()))?;
        record.tombstoned_at = None;
        self.put(&record)?;
        Ok(record)
    }

    fn purge(&self, node_id: &NodeID, cutoff: u64) -> Result<(), StorageError> {
        let record = self
            .get(node_id)?
            .ok_or_else(|| StorageError::InvalidPath("Node not found".to_string()))?;
        let ts = record
            .tombstoned_at
            .ok_or_else(|| StorageError::InvalidPath("Node is not tombstoned".to_string()))?;
        if ts > cutoff {
            return Err(StorageError::InvalidPath(
                "Tombstone is newer than cutoff".to_string(),
            ));
        }
        let key = node_id.as_slice();
        self.db.remove(key).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to remove node record: {}", e),
            ))
        })?;
        let path_key = format!("path:{}", record.path.to_string_lossy());
        self.db.remove(path_key.as_bytes()).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to remove path mapping: {}", e),
            ))
        })?;
        Ok(())
    }

    fn list_tombstoned(&self, older_than: Option<u64>) -> Result<Vec<NodeID>, StorageError> {
        let mut out = Vec::new();
        for item in self.db.iter() {
            let (key, value) = item.map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to iterate store: {}", e),
                ))
            })?;
            if !is_node_record_key(key.as_ref()) {
                continue;
            }
            let record = match deserialize_node_record(&value) {
                Ok(record) => record,
                Err(err) if is_corrupt_node_record_error(&err) => {
                    warn!(
                        key = %hex::encode(key.as_ref()),
                        error = %err,
                        "Skipping corrupt node record while listing tombstoned nodes"
                    );
                    continue;
                }
                Err(err) => return Err(err),
            };
            if let Some(ts) = record.tombstoned_at {
                if older_than.map_or(true, |cutoff| ts <= cutoff) {
                    out.push(record.node_id);
                }
            }
        }
        Ok(out)
    }

    fn flush(&self) -> Result<(), StorageError> {
        self.db.flush().map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to flush database: {}", e),
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
            let value = serialize_node_record(record)?;
            batch.insert(key, value);

            // Maintain the same path secondary index written by put.
            let path_key = format!("path:{}", record.path.to_string_lossy());
            let path_value = bincode::serialize(&record.node_id).map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to serialize node ID for path mapping: {}", e),
                ))
            })?;
            batch.insert(path_key.as_bytes(), path_value);
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
            metadata: Default::default(),
            tombstoned_at: None,
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
            metadata: Default::default(),
            tombstoned_at: None,
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
                metadata: Default::default(),
                tombstoned_at: None,
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
                metadata: Default::default(),
                tombstoned_at: None,
            },
        ];

        store.put_batch(&records).unwrap();

        // Verify both records are stored
        assert!(store.get(&[1u8; 32]).unwrap().is_some());
        assert!(store.get(&[3u8; 32]).unwrap().is_some());
        assert!(store
            .get_by_path(std::path::Path::new("/test/file1.txt"))
            .unwrap()
            .is_some());
        assert!(store
            .get_by_path(std::path::Path::new("/test/file2.txt"))
            .unwrap()
            .is_some());
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
            metadata: Default::default(),
            tombstoned_at: None,
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
            metadata: Default::default(),
            tombstoned_at: None,
        };

        store.put(&record2).unwrap();

        // Verify update
        let retrieved = store.get(&node_id).unwrap().unwrap();
        assert_eq!(retrieved.path, record2.path);
        assert_eq!(
            retrieved.path,
            std::path::PathBuf::from("/test/file_updated.txt")
        );
    }

    #[test]
    fn test_tombstone_and_find_by_path_skips() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();
        let node_id = [1u8; 32];
        let path = std::path::PathBuf::from("/test/file.txt");
        let record = NodeRecord {
            node_id,
            path: path.clone(),
            node_type: NodeType::File {
                size: 100,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        store.put(&record).unwrap();
        assert!(store.find_by_path(&path).unwrap().is_some());
        let updated = store.tombstone(&node_id).unwrap();
        assert!(updated.tombstoned_at.is_some());
        assert!(store
            .get(&node_id)
            .unwrap()
            .unwrap()
            .tombstoned_at
            .is_some());
        assert!(store.find_by_path(&path).unwrap().is_none());
        assert!(store.get_by_path(&path).unwrap().is_some());
    }

    #[test]
    fn test_restore_clears_tombstone() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();
        let node_id = [1u8; 32];
        let path = std::path::PathBuf::from("/test/file.txt");
        let record = NodeRecord {
            node_id,
            path: path.clone(),
            node_type: NodeType::File {
                size: 100,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        store.put(&record).unwrap();
        store.tombstone(&node_id).unwrap();
        store.restore(&node_id).unwrap();
        assert!(store
            .get(&node_id)
            .unwrap()
            .unwrap()
            .tombstoned_at
            .is_none());
        assert!(store.find_by_path(&path).unwrap().is_some());
    }

    #[test]
    fn test_purge_removes_record() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();
        let node_id = [1u8; 32];
        let path = std::path::PathBuf::from("/test/file.txt");
        let record = NodeRecord {
            node_id,
            path: path.clone(),
            node_type: NodeType::File {
                size: 100,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        store.put(&record).unwrap();
        store.tombstone(&node_id).unwrap();
        let ts = store.get(&node_id).unwrap().unwrap().tombstoned_at.unwrap();
        store.purge(&node_id, ts).unwrap();
        assert!(store.get(&node_id).unwrap().is_none());
        assert!(store.get_by_path(&path).unwrap().is_none());
    }

    #[test]
    fn test_list_tombstoned_and_list_active() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();
        let r1 = NodeRecord {
            node_id: [1u8; 32],
            path: std::path::PathBuf::from("/a"),
            node_type: NodeType::File {
                size: 0,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        let r2 = NodeRecord {
            node_id: [2u8; 32],
            path: std::path::PathBuf::from("/b"),
            node_type: NodeType::File {
                size: 0,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        store.put(&r1).unwrap();
        store.put(&r2).unwrap();
        assert_eq!(store.list_active().unwrap().len(), 2);
        store.tombstone(&[1u8; 32]).unwrap();
        let tomb = store.list_tombstoned(None).unwrap();
        assert_eq!(tomb.len(), 1);
        assert_eq!(store.list_active().unwrap().len(), 1);
    }

    #[test]
    fn test_list_all_skips_corrupt_node_records() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let valid = NodeRecord {
            node_id: [1u8; 32],
            path: std::path::PathBuf::from("/ok"),
            node_type: NodeType::File {
                size: 10,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        store.put(&valid).unwrap();

        // Insert a truncated payload under a node-like key to simulate corruption.
        store.db.insert([9u8; 32], vec![1u8, 2u8]).unwrap();

        let records = store.list_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].node_id, [1u8; 32]);
    }

    #[test]
    fn test_list_tombstoned_skips_corrupt_node_records() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let active = NodeRecord {
            node_id: [1u8; 32],
            path: std::path::PathBuf::from("/a"),
            node_type: NodeType::File {
                size: 0,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        let tombstoned = NodeRecord {
            node_id: [2u8; 32],
            path: std::path::PathBuf::from("/b"),
            node_type: NodeType::File {
                size: 0,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: Some(1),
        };
        store.put(&active).unwrap();
        store.put(&tombstoned).unwrap();
        store.db.insert([8u8; 32], vec![0u8]).unwrap();

        let tomb = store.list_tombstoned(None).unwrap();
        assert_eq!(tomb, vec![[2u8; 32]]);
    }

    #[test]
    fn test_list_all_ignores_path_keys_even_when_len_32() {
        let temp_dir = TempDir::new().unwrap();
        let store = SledNodeRecordStore::new(temp_dir.path()).unwrap();

        let valid = NodeRecord {
            node_id: [1u8; 32],
            path: std::path::PathBuf::from("/ok"),
            node_type: NodeType::File {
                size: 10,
                content_hash: [2u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: Default::default(),
            tombstoned_at: None,
        };
        store.put(&valid).unwrap();

        let key = b"path:./apps/react/src/components".to_vec();
        assert_eq!(key.len(), 32);
        let value = bincode::serialize(&[7u8; 32]).unwrap();
        store.db.insert(key, value).unwrap();

        let records = store.list_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].node_id, [1u8; 32]);
    }
}
