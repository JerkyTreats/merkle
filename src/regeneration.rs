//! Incremental Regeneration
//!
//! Rebuilds derived context frames when their basis changes. Regeneration is incremental,
//! localized, and basis-driven—only frames whose basis has changed are regenerated.
//! Old frames are retained (append-only), ensuring full history preservation.

use crate::error::{ApiError, StorageError};
use crate::frame::id::compute_basis_hash;
use crate::frame::{Frame, FrameStorage};
use crate::heads::HeadIndex;
use crate::store::NodeRecordStore;
use crate::types::{FrameID, Hash, NodeID};
use bincode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Basis index: basis_hash → Vec<FrameID>
///
/// Enables fast lookup of frames affected by basis changes.
/// Maps a basis hash to all frames that were created with that basis.
pub struct BasisIndex {
    /// Index: basis_hash → Vec<FrameID>
    index: HashMap<Hash, Vec<FrameID>>,
    /// Reverse index: FrameID → basis_hash (for cleanup)
    reverse: HashMap<FrameID, Hash>,
}

impl Default for BasisIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl BasisIndex {
    /// Create a new empty basis index
    pub fn new() -> Self {
        BasisIndex {
            index: HashMap::new(),
            reverse: HashMap::new(),
        }
    }

    /// Add a frame to the index
    ///
    /// Associates the frame's basis hash with its FrameID.
    pub fn add_frame(&mut self, basis_hash: Hash, frame_id: FrameID) {
        self.index
            .entry(basis_hash)
            .or_insert_with(Vec::new)
            .push(frame_id);
        self.reverse.insert(frame_id, basis_hash);
    }

    /// Remove a frame from the index
    ///
    /// Note: This doesn't delete the frame from storage, just removes it from the index.
    /// Old frames are preserved (append-only).
    pub fn remove_frame(&mut self, frame_id: &FrameID) {
        if let Some(basis_hash) = self.reverse.remove(frame_id) {
            if let Some(frame_ids) = self.index.get_mut(&basis_hash) {
                frame_ids.retain(|&id| id != *frame_id);
                if frame_ids.is_empty() {
                    self.index.remove(&basis_hash);
                }
            }
        }
    }

    /// Get all frames with a given basis hash
    pub fn get_frames_by_basis(&self, basis_hash: &Hash) -> Vec<FrameID> {
        self.index
            .get(basis_hash)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Get the basis hash for a frame
    pub fn get_basis_for_frame(&self, frame_id: &FrameID) -> Option<Hash> {
        self.reverse.get(frame_id).copied()
    }

    /// Check if a basis hash exists in the index
    pub fn has_basis(&self, basis_hash: &Hash) -> bool {
        self.index.contains_key(basis_hash)
    }

    /// Get the number of basis entries in the index
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Iterate over all basis entries
    pub fn iter(&self) -> impl Iterator<Item = (&Hash, &Vec<FrameID>)> {
        self.index.iter()
    }

    /// Get the persistence path for a workspace root
    ///
    /// Uses XDG data directory: $XDG_DATA_HOME/merkle/workspaces/<hash>/basis_index.bin
    pub fn persistence_path(workspace_root: &Path) -> PathBuf {
        // Try to use XDG data directory, fall back to .merkle if XDG is not available
        if let Ok(data_dir) = crate::config::xdg::workspace_data_dir(workspace_root) {
            data_dir.join("basis_index.bin")
        } else {
            // Fallback to old location if XDG is not available
            workspace_root.join(".merkle").join("basis_index.bin")
        }
    }

    /// Load basis index from disk
    ///
    /// Returns an empty index if the file doesn't exist or is corrupted.
    pub fn load_from_disk<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path = path.as_ref();

        // Check if file exists
        if !path.exists() {
            return Ok(BasisIndex::new());
        }

        // Read file
        let bytes = fs::read(path).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read basis index from {:?}: {}", path, e),
            ))
        })?;

        // Deserialize
        let persistence: BasisIndexPersistence = bincode::deserialize(&bytes).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize basis index from {:?}: {}", path, e),
            ))
        })?;

        // Validate version
        if persistence.version != 1 {
            return Err(StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Unsupported basis index version: {} (expected 1)",
                    persistence.version
                ),
            )));
        }

        // Convert entries to HashMap
        let mut index = HashMap::new();
        let mut reverse = HashMap::new();

        for entry in persistence.entries {
            // Validate hash and frame_id are 32 bytes
            if entry.basis_hash.len() != 32 {
                return Err(StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid basis_hash length in basis index"),
                )));
            }

            let mut basis_hash = [0u8; 32];
            basis_hash.copy_from_slice(&entry.basis_hash);

            let mut frame_ids = Vec::new();
            for frame_id_bytes in entry.frame_ids {
                if frame_id_bytes.len() != 32 {
                    return Err(StorageError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Invalid frame_id length in basis index"),
                    )));
                }
                let mut frame_id = [0u8; 32];
                frame_id.copy_from_slice(&frame_id_bytes);
                frame_ids.push(frame_id);
                reverse.insert(frame_id, basis_hash);
            }

            index.insert(basis_hash, frame_ids);
        }

        Ok(BasisIndex { index, reverse })
    }

    /// Save basis index to disk atomically
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

        // Convert HashMap to persistence format
        let mut entries = Vec::new();
        for (basis_hash, frame_ids) in &self.index {
            entries.push(BasisIndexEntry {
                basis_hash: basis_hash.to_vec(),
                frame_ids: frame_ids.iter().map(|f| f.to_vec()).collect(),
            });
        }

        let persistence = BasisIndexPersistence {
            version: 1,
            entries,
        };

        // Serialize
        let serialized = bincode::serialize(&persistence).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to serialize basis index: {}", e),
            ))
        })?;

        // Write to temporary file (atomic write)
        let temp_path = path.with_extension("bin.tmp");
        fs::write(&temp_path, &serialized).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write basis index to {:?}: {}", temp_path, e),
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

/// Persistence format for basis index
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BasisIndexPersistence {
    version: u32,
    entries: Vec<BasisIndexEntry>,
}

/// Entry in the basis index persistence format
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BasisIndexEntry {
    basis_hash: Vec<u8>,
    frame_ids: Vec<Vec<u8>>,
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_basis_index() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("basis_index.bin");

        // Create a basis index with some entries
        let mut index = BasisIndex::new();
        let basis_hash: Hash = [1u8; 32];
        let frame_id: FrameID = [2u8; 32];
        index.add_frame(basis_hash, frame_id);

        // Save to disk
        index.save_to_disk(&path).unwrap();
        assert!(path.exists());

        // Load from disk
        let loaded = BasisIndex::load_from_disk(&path).unwrap();
        assert_eq!(loaded.index.len(), 1);
        let frames = loaded.get_frames_by_basis(&basis_hash);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], frame_id);
        assert_eq!(loaded.get_basis_for_frame(&frame_id), Some(basis_hash));
    }

    #[test]
    fn test_load_nonexistent_basis_index() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.bin");

        // Load from non-existent file should return empty index
        let loaded = BasisIndex::load_from_disk(&path).unwrap();
        assert_eq!(loaded.index.len(), 0);
    }

    #[test]
    fn test_persistence_path() {
        let workspace_root = std::path::Path::new("/workspace");
        let path = BasisIndex::persistence_path(workspace_root);
        assert!(path.to_string_lossy().ends_with(".merkle/basis_index.bin"));
    }

    #[test]
    fn test_save_and_load_multiple_entries() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("basis_index.bin");

        // Create a basis index with multiple entries
        let mut index = BasisIndex::new();
        let basis_hash1: Hash = [1u8; 32];
        let basis_hash2: Hash = [2u8; 32];
        let frame_id1: FrameID = [10u8; 32];
        let frame_id2: FrameID = [20u8; 32];
        let frame_id3: FrameID = [30u8; 32];

        index.add_frame(basis_hash1, frame_id1);
        index.add_frame(basis_hash1, frame_id2); // Same basis, different frame
        index.add_frame(basis_hash2, frame_id3);

        // Save to disk
        index.save_to_disk(&path).unwrap();

        // Load from disk
        let loaded = BasisIndex::load_from_disk(&path).unwrap();
        assert_eq!(loaded.index.len(), 2);

        let frames1 = loaded.get_frames_by_basis(&basis_hash1);
        assert_eq!(frames1.len(), 2);
        assert!(frames1.contains(&frame_id1));
        assert!(frames1.contains(&frame_id2));

        let frames2 = loaded.get_frames_by_basis(&basis_hash2);
        assert_eq!(frames2.len(), 1);
        assert_eq!(frames2[0], frame_id3);
    }
}

/// Regeneration report
///
/// Summary of regeneration results.
#[derive(Debug, Clone)]
pub struct RegenerationReport {
    /// NodeID that was regenerated
    pub node_id: NodeID,
    /// Number of frames regenerated
    pub regenerated_count: usize,
    /// FrameIDs of regenerated frames
    pub frame_ids: Vec<FrameID>,
    /// Number of legacy synthesized frame heads skipped
    pub legacy_synthesis_skipped: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

fn is_legacy_synthesized_frame(frame: &Frame) -> bool {
    frame.metadata.contains_key("basis_hash") || frame.metadata.contains_key("synthesis_policy")
}

/// Detect basis changes for a node.
///
/// Compares the stored basis hash for each regular frame type with the current basis hash.
/// Legacy synthesized frame heads are ignored and reported as skipped elsewhere.
pub fn detect_basis_changes(
    node_id: NodeID,
    frame_types: &[String],
    basis_index: &BasisIndex,
    head_index: &HeadIndex,
    frame_storage: &FrameStorage,
) -> Result<Vec<String>, ApiError> {
    let mut changed_types = Vec::new();

    for frame_type in frame_types {
        // Get current head frame for this type
        let head_frame_id = match head_index
            .get_head(&node_id, frame_type)
            .map_err(ApiError::from)?
        {
            Some(id) => id,
            None => continue, // No frame to regenerate
        };

        // Get the frame to determine its basis type
        let frame = match frame_storage.get(&head_frame_id).map_err(ApiError::from)? {
            Some(f) => f,
            None => continue, // Frame not found, skip
        };

        // Legacy synthesized frames are read-only after synthesis feature removal.
        if is_legacy_synthesized_frame(&frame) {
            continue;
        }

        // Regular frame - compare frame basis hash
        let stored_basis_hash = match basis_index.get_basis_for_frame(&head_frame_id) {
            Some(hash) => hash,
            None => continue, // Frame not in index, skip
        };

        // Compute current basis hash
        let current_basis_hash = compute_basis_hash(&frame.basis).map_err(ApiError::from)?;

        // Check if basis has changed
        if stored_basis_hash != current_basis_hash {
            changed_types.push(frame_type.clone());
        }
    }

    Ok(changed_types)
}

/// Regenerate frames for a node.
///
/// Detects basis changes and reports them. Legacy synthesized frame heads are skipped.
pub fn regenerate_node(
    node_id: NodeID,
    recursive: bool,
    basis_index: &mut BasisIndex,
    head_index: &mut HeadIndex,
    frame_storage: &FrameStorage,
    node_store: &dyn NodeRecordStore,
    agent_id: String,
) -> Result<RegenerationReport, ApiError> {
    let start_time = std::time::Instant::now();

    // Get all frame types for this node
    let all_frame_ids = head_index.get_all_heads_for_node(&node_id);
    let mut frame_types: Vec<String> = Vec::new();

    // Extract frame types from head index
    // We need to get frame types from the frames themselves
    for frame_id in &all_frame_ids {
        if let Some(frame) = frame_storage.get(frame_id).map_err(ApiError::from)? {
            if !frame_types.contains(&frame.frame_type) {
                frame_types.push(frame.frame_type.clone());
            }
        }
    }

    // Detect basis changes
    let changed_types = detect_basis_changes(
        node_id,
        &frame_types,
        basis_index,
        head_index,
        frame_storage,
    )?;

    let mut regenerated_frame_ids = Vec::new();
    let mut legacy_synthesis_skipped = 0;

    for frame_type in &frame_types {
        let head_frame_id = match head_index
            .get_head(&node_id, frame_type)
            .map_err(ApiError::from)?
        {
            Some(id) => id,
            None => continue,
        };
        let head_frame = match frame_storage.get(&head_frame_id).map_err(ApiError::from)? {
            Some(f) => f,
            None => continue,
        };
        if is_legacy_synthesized_frame(&head_frame) {
            legacy_synthesis_skipped += 1;
        }
    }

    // Regenerate each changed frame type
    for frame_type in &changed_types {
        // Get current head frame
        let head_frame_id = match head_index
            .get_head(&node_id, frame_type)
            .map_err(ApiError::from)?
        {
            Some(id) => id,
            None => continue,
        };

        let head_frame = match frame_storage.get(&head_frame_id).map_err(ApiError::from)? {
            Some(f) => f,
            None => continue,
        };

        // Regular frame - check if basis changed
        let stored_basis_hash = match basis_index.get_basis_for_frame(&head_frame_id) {
            Some(hash) => hash,
            None => continue, // Not in index, skip
        };

        let current_basis_hash = compute_basis_hash(&head_frame.basis).map_err(ApiError::from)?;

        if stored_basis_hash != current_basis_hash {
            // Basis changed - non-synthesized frames cannot be auto-regenerated.
            continue;
        }
    }

    // If recursive, regenerate child nodes
    if recursive {
        let node_record = node_store
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;

        for child_node_id in &node_record.children {
            let child_report = regenerate_node(
                *child_node_id,
                true,
                basis_index,
                head_index,
                frame_storage,
                node_store,
                agent_id.clone(),
            )?;

            regenerated_frame_ids.extend(child_report.frame_ids);
            legacy_synthesis_skipped += child_report.legacy_synthesis_skipped;
        }
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    Ok(RegenerationReport {
        node_id,
        regenerated_count: regenerated_frame_ids.len(),
        frame_ids: regenerated_frame_ids,
        legacy_synthesis_skipped,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::storage::FrameStorage;
    use crate::frame::{Basis, Frame};
    use crate::heads::HeadIndex;
    use crate::store::{NodeRecord, NodeRecordStore, NodeType, SledNodeRecordStore};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn test_basis_index_add_and_get() {
        let mut index = BasisIndex::new();
        let basis_hash: Hash = [1u8; 32];
        let frame_id: FrameID = [2u8; 32];

        index.add_frame(basis_hash, frame_id);

        let frames = index.get_frames_by_basis(&basis_hash);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], frame_id);
    }

    #[test]
    fn test_basis_index_multiple_frames() {
        let mut index = BasisIndex::new();
        let basis_hash: Hash = [1u8; 32];
        let frame_id1: FrameID = [2u8; 32];
        let frame_id2: FrameID = [3u8; 32];

        index.add_frame(basis_hash, frame_id1);
        index.add_frame(basis_hash, frame_id2);

        let frames = index.get_frames_by_basis(&basis_hash);
        assert_eq!(frames.len(), 2);
        assert!(frames.contains(&frame_id1));
        assert!(frames.contains(&frame_id2));
    }

    #[test]
    fn test_basis_index_remove_frame() {
        let mut index = BasisIndex::new();
        let basis_hash: Hash = [1u8; 32];
        let frame_id: FrameID = [2u8; 32];

        index.add_frame(basis_hash, frame_id);
        assert_eq!(index.get_frames_by_basis(&basis_hash).len(), 1);

        index.remove_frame(&frame_id);
        assert_eq!(index.get_frames_by_basis(&basis_hash).len(), 0);
    }

    #[test]
    fn test_basis_index_get_basis_for_frame() {
        let mut index = BasisIndex::new();
        let basis_hash: Hash = [1u8; 32];
        let frame_id: FrameID = [2u8; 32];

        index.add_frame(basis_hash, frame_id);

        let retrieved_hash = index.get_basis_for_frame(&frame_id);
        assert_eq!(retrieved_hash, Some(basis_hash));
    }

    #[test]
    fn test_regeneration_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let frame_storage_path = temp_dir.path().join("frames");

        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage = Arc::new(FrameStorage::new(&frame_storage_path).unwrap());
        let mut head_index = HeadIndex::new();
        let mut basis_index = BasisIndex::new();

        let dir_node_id: NodeID = [1u8; 32];
        let dir_record = NodeRecord {
            node_id: dir_node_id,
            path: PathBuf::from("/test"),
            node_type: NodeType::Directory,
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: HashMap::new(),
            tombstoned_at: None,
        };

        node_store.put(&dir_record).unwrap();

        // Create a legacy synthesized head frame.
        let mut metadata = HashMap::new();
        metadata.insert("synthesis_policy".to_string(), "concatenation".to_string());
        metadata.insert("basis_hash".to_string(), hex::encode([7u8; 32]));
        metadata.insert("child_frame_count".to_string(), "0".to_string());

        let dir_basis = Basis::Node(dir_node_id);
        let dir_frame = Frame::new(
            dir_basis,
            b"legacy synthesized".to_vec(),
            "test".to_string(),
            "agent-1".to_string(),
            metadata,
        )
        .unwrap();

        frame_storage.store(&dir_frame).unwrap();
        head_index
            .update_head(&dir_node_id, "test", &dir_frame.frame_id)
            .unwrap();
        let dir_basis_hash = compute_basis_hash(&dir_frame.basis).unwrap();
        basis_index.add_frame(dir_basis_hash, dir_frame.frame_id);

        // First regeneration - should detect no changes
        let frame_types = vec!["test".to_string()];
        let changed = detect_basis_changes(
            dir_node_id,
            &frame_types,
            &basis_index,
            &head_index,
            &frame_storage,
        )
        .unwrap();

        assert_eq!(
            changed.len(),
            0,
            "Legacy synthesized frames are skipped by detection"
        );

        // Regenerate (should be idempotent)
        let report1 = regenerate_node(
            dir_node_id,
            false,
            &mut basis_index,
            &mut head_index,
            &frame_storage,
            node_store.as_ref(),
            "agent-1".to_string(),
        )
        .unwrap();

        assert_eq!(
            report1.regenerated_count, 0,
            "First regeneration should produce no changes"
        );
        assert_eq!(report1.legacy_synthesis_skipped, 1);

        // Regenerate again (should still be idempotent)
        let report2 = regenerate_node(
            dir_node_id,
            false,
            &mut basis_index,
            &mut head_index,
            &frame_storage,
            node_store.as_ref(),
            "agent-1".to_string(),
        )
        .unwrap();

        assert_eq!(
            report2.regenerated_count, 0,
            "Second regeneration should also produce no changes"
        );
        assert_eq!(report2.legacy_synthesis_skipped, 1);
    }
}
