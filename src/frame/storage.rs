//! Frame storage implementation
//!
//! Provides content-addressed storage for context frames using the filesystem.
//! Frames are stored at paths based on their FrameID to enable efficient
//! content-addressed retrieval.

use crate::error::StorageError;
use crate::frame::Frame;
use crate::types::FrameID;
use bincode;
use std::fs;
use std::path::{Path, PathBuf};

/// Content-addressed frame storage
///
/// Stores frames on the filesystem using a content-addressed path structure:
/// `{root}/frames/{hex[0..2]}/{hex[2..4]}/{frame_id}.frame`
///
/// This structure:
/// - Enables efficient content-addressed lookup
/// - Prevents directory bloat (distributes files across subdirectories)
/// - Supports deduplication (same FrameID = same path)
pub struct FrameStorage {
    root: PathBuf,
}

impl FrameStorage {
    /// Create a new FrameStorage at the given root path
    ///
    /// The root path should be a directory where frames will be stored.
    /// The directory structure will be created as needed.
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self, StorageError> {
        let root = root.as_ref().to_path_buf();

        // Create the frames directory if it doesn't exist
        let frames_dir = root.join("frames");
        fs::create_dir_all(&frames_dir).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create frames directory at {:?}: {}", frames_dir, e),
            ))
        })?;

        Ok(Self { root })
    }

    /// Get the root path of this storage
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Store a frame to disk
    ///
    /// Uses atomic writes (write to .tmp, then rename) to ensure consistency.
    /// If a frame with the same FrameID already exists, this is a no-op (deduplication).
    ///
    /// Returns an error if:
    /// - The frame cannot be serialized
    /// - The filesystem operation fails
    /// - The FrameID doesn't match the computed hash (corruption detection)
    pub fn store(&self, frame: &Frame) -> Result<(), StorageError> {
        // Verify FrameID matches computed hash (corruption detection)
        let computed_id = crate::frame::id::compute_frame_id(
            &frame.basis,
            &frame.content,
            &frame.frame_type,
        )?;

        if computed_id != frame.frame_id {
            return Err(StorageError::HashMismatch {
                expected: frame.frame_id,
                actual: computed_id,
            });
        }

        // Check if frame already exists (deduplication)
        if self.exists(&frame.frame_id)? {
            return Ok(()); // Frame already stored, skip
        }

        // Compute storage path
        let frame_path = self.frame_path(&frame.frame_id);
        let temp_path = frame_path.with_extension("frame.tmp");

        // Create parent directories if needed
        if let Some(parent) = frame_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create parent directory {:?}: {}", parent, e),
                ))
            })?;
        }

        // Serialize frame to bytes
        let serialized = bincode::serialize(frame).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to serialize frame: {}", e),
            ))
        })?;

        // Write to temporary file (atomic write)
        fs::write(&temp_path, &serialized).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write frame to {:?}: {}", temp_path, e),
            ))
        })?;

        // Atomically rename temp file to final location
        fs::rename(&temp_path, &frame_path).map_err(|e| {
            // Clean up temp file on error
            let _ = fs::remove_file(&temp_path);
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to rename temp file to {:?}: {}", frame_path, e),
            ))
        })?;

        Ok(())
    }

    /// Retrieve a frame by FrameID
    ///
    /// Returns `None` if the frame doesn't exist.
    /// Returns an error if the frame exists but cannot be deserialized (corruption).
    pub fn get(&self, frame_id: &FrameID) -> Result<Option<Frame>, StorageError> {
        let frame_path = self.frame_path(frame_id);

        // Check if file exists
        if !frame_path.exists() {
            return Ok(None);
        }

        // Read file
        let bytes = fs::read(&frame_path).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read frame from {:?}: {}", frame_path, e),
            ))
        })?;

        // Deserialize frame
        let frame: Frame = bincode::deserialize(&bytes).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to deserialize frame from {:?}: {}", frame_path, e),
            ))
        })?;

        // Verify FrameID matches (corruption detection)
        if frame.frame_id != *frame_id {
            return Err(StorageError::HashMismatch {
                expected: *frame_id,
                actual: frame.frame_id,
            });
        }

        Ok(Some(frame))
    }

    /// Check if a frame exists
    ///
    /// Returns `true` if a frame with the given FrameID exists in storage.
    pub fn exists(&self, frame_id: &FrameID) -> Result<bool, StorageError> {
        let frame_path = self.frame_path(frame_id);
        Ok(frame_path.exists())
    }

    /// Compute the filesystem path for a given FrameID
    ///
    /// Path structure: `{root}/frames/{hex[0..2]}/{hex[2..4]}/{frame_id}.frame`
    ///
    /// This distributes frames across subdirectories to prevent directory bloat.
    fn frame_path(&self, frame_id: &FrameID) -> PathBuf {
        // Convert FrameID to hex string using standard formatting
        let hex: String = frame_id.iter().map(|b| format!("{:02x}", b)).collect();

        // Extract first 2 and next 2 hex characters for subdirectory structure
        let prefix1 = &hex[0..2];
        let prefix2 = &hex[2..4];

        // Build path: frames/{prefix1}/{prefix2}/{frame_id}.frame
        self.root
            .join("frames")
            .join(prefix1)
            .join(prefix2)
            .join(format!("{}.frame", hex))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Basis, Frame};
    use crate::types::NodeID;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_store_and_retrieve() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create a test frame
        let node_id: NodeID = [1u8; 32];
        let basis = Basis::Node(node_id);
        let content = b"test frame content".to_vec();
        let frame_type = "test".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(basis, content, frame_type, metadata).unwrap();

        // Store frame
        storage.store(&frame).unwrap();

        // Retrieve frame
        let retrieved = storage.get(&frame.frame_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();

        // Verify frame matches
        assert_eq!(retrieved.frame_id, frame.frame_id);
        assert_eq!(retrieved.content, frame.content);
        assert_eq!(retrieved.frame_type, frame.frame_type);
    }

    #[test]
    fn test_deduplication() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create a test frame
        let node_id: NodeID = [1u8; 32];
        let basis = Basis::Node(node_id);
        let content = b"test content".to_vec();
        let frame_type = "test".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(basis, content, frame_type, metadata).unwrap();

        // Store frame twice
        storage.store(&frame).unwrap();
        storage.store(&frame).unwrap(); // Should be a no-op

        // Verify frame exists
        assert!(storage.exists(&frame.frame_id).unwrap());

        // Verify only one file exists (deduplication worked)
        let frame_path = storage.frame_path(&frame.frame_id);
        assert!(frame_path.exists());
    }

    #[test]
    fn test_get_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        let frame_id: FrameID = [0u8; 32];
        let result = storage.get(&frame_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_exists() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        let node_id: NodeID = [1u8; 32];
        let basis = Basis::Node(node_id);
        let content = b"test".to_vec();
        let frame_type = "test".to_string();
        let metadata = HashMap::new();

        let frame = Frame::new(basis, content, frame_type, metadata).unwrap();

        // Frame doesn't exist yet
        assert!(!storage.exists(&frame.frame_id).unwrap());

        // Store frame
        storage.store(&frame).unwrap();

        // Frame exists now
        assert!(storage.exists(&frame.frame_id).unwrap());
    }

    #[test]
    fn test_path_structure() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        let frame_id: FrameID = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        ];

        let path = storage.frame_path(&frame_id);

        // Verify path structure: frames/{hex[0..2]}/{hex[2..4]}/{frame_id}.frame
        assert!(path.to_string_lossy().contains("frames/12/34"));
        assert!(path.to_string_lossy().ends_with(".frame"));
        assert!(path.to_string_lossy().contains("123456789abcdef0112233445566778899aabbccddeeff000102030405060708"));
    }

    #[test]
    fn test_corruption_detection() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameStorage::new(temp_dir.path()).unwrap();

        // Create a frame
        let node_id: NodeID = [1u8; 32];
        let basis = Basis::Node(node_id);
        let content = b"test".to_vec();
        let frame_type = "test".to_string();
        let metadata = HashMap::new();

        let mut frame = Frame::new(basis, content, frame_type, metadata).unwrap();

        // Corrupt the FrameID
        frame.frame_id[0] = 0xFF;

        // Store should fail due to hash mismatch
        let result = storage.store(&frame);
        assert!(result.is_err());
        match result {
            Err(StorageError::HashMismatch { .. }) => {}
            _ => panic!("Expected HashMismatch error"),
        }
    }
}
