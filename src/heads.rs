//! Frame Heads
//!
//! Provides O(1) access to the "latest" frame for a given node and frame type.

use crate::error::StorageError;
use crate::types::{FrameID, NodeID};
use bincode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const HEAD_INDEX_VERSION_V1: u32 = 1;
const HEAD_INDEX_VERSION_V2: u32 = 2;

/// Head entry with optional tombstone marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HeadEntry {
    pub frame_id: FrameID,
    pub tombstoned_at: Option<u64>,
}

/// Head index: (NodeID, frame_type) -> HeadEntry
pub struct HeadIndex {
    pub(crate) heads: HashMap<(NodeID, String), HeadEntry>,
}

impl Default for HeadIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadIndex {
    pub fn new() -> Self {
        HeadIndex {
            heads: HashMap::new(),
        }
    }

    /// Get active head for node and frame type (skips tombstoned entries).
    pub fn get_head(
        &self,
        node_id: &NodeID,
        frame_type: &str,
    ) -> Result<Option<FrameID>, StorageError> {
        Ok(self
            .heads
            .get(&(*node_id, frame_type.to_string()))
            .filter(|e| e.tombstoned_at.is_none())
            .map(|e| e.frame_id))
    }

    /// Get active head (alias for get_head; skips tombstoned).
    pub fn get_active_head(
        &self,
        node_id: &NodeID,
        frame_type: &str,
    ) -> Result<Option<FrameID>, StorageError> {
        self.get_head(node_id, frame_type)
    }

    pub fn update_head(
        &mut self,
        node_id: &NodeID,
        frame_type: &str,
        frame_id: &FrameID,
    ) -> Result<(), StorageError> {
        self.heads.insert(
            (*node_id, frame_type.to_string()),
            HeadEntry {
                frame_id: *frame_id,
                tombstoned_at: None,
            },
        );
        Ok(())
    }

    /// Tombstone all head entries for a node (all frame types).
    pub fn tombstone_heads_for_node(&mut self, node_id: &NodeID) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        for ((nid, _), entry) in self.heads.iter_mut() {
            if *nid == *node_id {
                entry.tombstoned_at = Some(now);
            }
        }
    }

    /// Restore all head entries for a node (remove tombstone marker).
    pub fn restore_heads_for_node(&mut self, node_id: &NodeID) {
        for ((nid, _), entry) in self.heads.iter_mut() {
            if *nid == *node_id {
                entry.tombstoned_at = None;
            }
        }
    }

    /// Purge tombstoned head entries older than cutoff.
    pub fn purge_tombstoned(&mut self, cutoff: u64) {
        self.heads
            .retain(|_, e| e.tombstoned_at.map_or(true, |ts| ts > cutoff));
    }

    /// Get all frame IDs for a given node (including tombstoned; used e.g. for compact).
    pub fn get_all_heads_for_node(&self, node_id: &NodeID) -> Vec<FrameID> {
        self.heads
            .iter()
            .filter_map(|((nid, _), e)| {
                if *nid == *node_id {
                    Some(e.frame_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all unique node IDs that have active (non-tombstoned) heads.
    pub fn get_all_node_ids(&self) -> Vec<NodeID> {
        let mut node_ids = std::collections::HashSet::new();
        for ((node_id, _), e) in &self.heads {
            if e.tombstoned_at.is_none() {
                node_ids.insert(*node_id);
            }
        }
        node_ids.into_iter().collect()
    }

    /// Count distinct node IDs that have an active head for the given frame type.
    pub fn count_nodes_for_frame_type(&self, frame_type: &str) -> usize {
        let mut node_ids = std::collections::HashSet::new();
        for ((node_id, ft), e) in &self.heads {
            if ft.as_str() == frame_type && e.tombstoned_at.is_none() {
                node_ids.insert(*node_id);
            }
        }
        node_ids.len()
    }

    /// Get the persistence path for a workspace root
    ///
    /// Uses XDG data directory: $XDG_DATA_HOME/merkle/workspaces/<hash>/head_index.bin
    pub fn persistence_path(workspace_root: &Path) -> PathBuf {
        // Try to use XDG data directory, fall back to .merkle if XDG is not available
        if let Ok(data_dir) = crate::config::xdg::workspace_data_dir(workspace_root) {
            data_dir.join("head_index.bin")
        } else {
            // Fallback to old location if XDG is not available
            workspace_root.join(".merkle").join("head_index.bin")
        }
    }

    /// Load head index from disk
    ///
    /// Returns an empty index if the file doesn't exist or is corrupted.
    pub fn load_from_disk<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path = path.as_ref();

        // Check if file exists
        if !path.exists() {
            return Ok(HeadIndex::new());
        }

        // Read file
        let bytes = fs::read(path).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read head index from {:?}: {}", path, e),
            ))
        })?;

        // Try legacy V1 format (single bincode blob) first.
        if let Ok(persistence) = bincode::deserialize::<HeadIndexPersistenceV1>(&bytes) {
            if persistence.version == HEAD_INDEX_VERSION_V1 {
                let mut heads = HashMap::new();
                for entry in persistence.entries {
                    if entry.frame_id.len() != 32 || entry.node_id.len() != 32 {
                        return Err(StorageError::IoError(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Invalid frame_id or node_id length in head index".to_string(),
                        )));
                    }
                    let mut node_id = [0u8; 32];
                    node_id.copy_from_slice(&entry.node_id);
                    let mut frame_id = [0u8; 32];
                    frame_id.copy_from_slice(&entry.frame_id);
                    heads.insert(
                        (node_id, entry.frame_type),
                        HeadEntry {
                            frame_id,
                            tombstoned_at: None,
                        },
                    );
                }
                return Ok(HeadIndex { heads });
            }
        }

        // V2 format: 4-byte version then bincode(entries).
        if bytes.len() < 4 {
            return Err(StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Head index file too short".to_string(),
            )));
        }
        let version = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if version != HEAD_INDEX_VERSION_V2 {
            return Err(StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unsupported head index version: {}", version),
            )));
        }
        let entries: Vec<HeadIndexEntry> = bincode::deserialize(&bytes[4..]).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize head index entries: {}", e),
            ))
        })?;

        let mut heads = HashMap::new();
        for entry in entries {
            if entry.frame_id.len() != 32 || entry.node_id.len() != 32 {
                return Err(StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid frame_id or node_id length in head index".to_string(),
                )));
            }
            let mut node_id = [0u8; 32];
            node_id.copy_from_slice(&entry.node_id);
            let mut frame_id = [0u8; 32];
            frame_id.copy_from_slice(&entry.frame_id);
            heads.insert(
                (node_id, entry.frame_type),
                HeadEntry {
                    frame_id,
                    tombstoned_at: entry.tombstoned_at,
                },
            );
        }

        Ok(HeadIndex { heads })
    }

    /// Save head index to disk atomically
    ///
    /// Uses temporary file + rename for atomic writes.
    pub fn save_to_disk<P: AsRef<Path>>(&self, path: P) -> Result<(), StorageError> {
        let path = path.as_ref();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create parent directory {:?}: {}", parent, e),
                ))
            })?;
        }

        let mut entries = Vec::new();
        for ((node_id, frame_type), head_entry) in &self.heads {
            entries.push(HeadIndexEntry {
                node_id: node_id.to_vec(),
                frame_type: frame_type.clone(),
                frame_id: head_entry.frame_id.to_vec(),
                tombstoned_at: head_entry.tombstoned_at,
            });
        }

        // V2 format: 4-byte version then bincode(entries).
        let payload = bincode::serialize(&entries).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to serialize head index entries: {}", e),
            ))
        })?;
        let mut serialized = Vec::with_capacity(4 + payload.len());
        serialized.extend_from_slice(&HEAD_INDEX_VERSION_V2.to_le_bytes());
        serialized.extend_from_slice(&payload);

        // Write to temporary file (atomic write)
        let temp_path = path.with_extension("bin.tmp");
        fs::write(&temp_path, &serialized).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write head index to {:?}: {}", temp_path, e),
            ))
        })?;

        // Atomically rename temp file to final location
        fs::rename(&temp_path, path).map_err(|e| {
            // Clean up temp file on error
            let _ = fs::remove_file(&temp_path);
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to rename temp file to {:?}: {}", path, e),
            ))
        })?;

        Ok(())
    }
}

/// Persistence format for head index (version 1: legacy single-blob).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadIndexPersistenceV1 {
    version: u32,
    entries: Vec<HeadIndexEntryV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadIndexEntryV1 {
    node_id: Vec<u8>,
    frame_type: String,
    frame_id: Vec<u8>,
}

/// Entry in the head index persistence format (version 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeadIndexEntry {
    node_id: Vec<u8>,
    frame_type: String,
    frame_id: Vec<u8>,
    tombstoned_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_head_index() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("head_index.bin");

        // Create a head index with some entries
        let mut index = HeadIndex::new();
        let node_id: NodeID = [1u8; 32];
        let frame_id: FrameID = [2u8; 32];
        index.update_head(&node_id, "test", &frame_id).unwrap();

        // Save to disk
        index.save_to_disk(&path).unwrap();
        assert!(path.exists());

        // Load from disk
        let loaded = HeadIndex::load_from_disk(&path).unwrap();
        assert_eq!(loaded.heads.len(), 1);
        assert_eq!(loaded.get_head(&node_id, "test").unwrap(), Some(frame_id));
    }

    #[test]
    fn test_load_nonexistent_head_index() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.bin");

        // Load from non-existent file should return empty index
        let loaded = HeadIndex::load_from_disk(&path).unwrap();
        assert_eq!(loaded.heads.len(), 0);
    }

    #[test]
    fn test_persistence_path() {
        let workspace_root = std::path::Path::new("/workspace");
        let path = HeadIndex::persistence_path(workspace_root);
        assert!(path.to_string_lossy().ends_with(".merkle/head_index.bin"));
    }

    #[test]
    fn test_save_and_load_multiple_entries() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("head_index.bin");

        // Create a head index with multiple entries
        let mut index = HeadIndex::new();
        let node_id1: NodeID = [1u8; 32];
        let node_id2: NodeID = [2u8; 32];
        let frame_id1: FrameID = [10u8; 32];
        let frame_id2: FrameID = [20u8; 32];
        let frame_id3: FrameID = [30u8; 32];

        index.update_head(&node_id1, "type1", &frame_id1).unwrap();
        index.update_head(&node_id1, "type2", &frame_id2).unwrap();
        index.update_head(&node_id2, "type1", &frame_id3).unwrap();

        // Save to disk
        index.save_to_disk(&path).unwrap();

        // Load from disk
        let loaded = HeadIndex::load_from_disk(&path).unwrap();
        assert_eq!(loaded.heads.len(), 3);
        assert_eq!(
            loaded.get_head(&node_id1, "type1").unwrap(),
            Some(frame_id1)
        );
        assert_eq!(
            loaded.get_head(&node_id1, "type2").unwrap(),
            Some(frame_id2)
        );
        assert_eq!(
            loaded.get_head(&node_id2, "type1").unwrap(),
            Some(frame_id3)
        );
    }

    #[test]
    fn test_tombstone_and_restore_heads() {
        let mut index = HeadIndex::new();
        let node_id: NodeID = [1u8; 32];
        let frame_id: FrameID = [2u8; 32];
        index.update_head(&node_id, "test", &frame_id).unwrap();
        assert_eq!(index.get_head(&node_id, "test").unwrap(), Some(frame_id));
        index.tombstone_heads_for_node(&node_id);
        assert_eq!(index.get_head(&node_id, "test").unwrap(), None);
        assert_eq!(index.get_active_head(&node_id, "test").unwrap(), None);
        index.restore_heads_for_node(&node_id);
        assert_eq!(index.get_head(&node_id, "test").unwrap(), Some(frame_id));
    }

    #[test]
    fn test_purge_tombstoned() {
        let mut index = HeadIndex::new();
        let node_id: NodeID = [1u8; 32];
        let frame_id: FrameID = [2u8; 32];
        index.update_head(&node_id, "test", &frame_id).unwrap();
        index.tombstone_heads_for_node(&node_id);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        index.purge_tombstoned(ts);
        assert_eq!(index.heads.len(), 0);
    }
}
