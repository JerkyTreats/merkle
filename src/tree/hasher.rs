//! Hash computation for filesystem nodes using BLAKE3

use crate::error::StorageError;
use crate::tree::path;
use crate::types::{Hash, NodeID};
use blake3::Hasher;
use std::collections::BTreeMap;
use std::path::Path;

/// Compute NodeID for a file node
///
/// NodeID = hash("file" || path_len || path || content_hash || metadata)
///
/// This ensures deterministic NodeID generation: same file content and path
/// always produces the same NodeID.
pub fn compute_file_node_id(
    file_path: &Path,
    content_hash: &Hash,
    metadata: &BTreeMap<String, String>,
) -> Result<NodeID, StorageError> {
    let canonical_path = path::canonicalize_path(file_path)?;
    let path_string = canonical_path.to_string_lossy();
    let path_bytes = path_string.as_bytes();

    let mut hasher = Hasher::new();

    // Hash type discriminator
    hasher.update(b"file");

    // Hash path length (8 bytes, big-endian for determinism)
    hasher.update(&(path_bytes.len() as u64).to_be_bytes());

    // Hash path
    hasher.update(path_bytes);

    // Hash content hash
    hasher.update(content_hash);

    // Hash metadata (sorted for determinism)
    for (key, value) in metadata.iter() {
        hasher.update(key.as_bytes());
        hasher.update(b":");
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }

    Ok(*hasher.finalize().as_bytes())
}

/// Compute NodeID for a directory node
///
/// NodeID = hash("directory" || path_len || path || children_count || children || metadata)
///
/// Children must be sorted by name for determinism.
pub fn compute_directory_node_id(
    dir_path: &Path,
    children: &[(String, NodeID)], // Must be sorted by name
    metadata: &BTreeMap<String, String>,
) -> Result<NodeID, StorageError> {
    let canonical_path = path::canonicalize_path(dir_path)?;
    let path_string = canonical_path.to_string_lossy();
    let path_bytes = path_string.as_bytes();

    let mut hasher = Hasher::new();

    // Hash type discriminator
    hasher.update(b"directory");

    // Hash path length (8 bytes, big-endian)
    hasher.update(&(path_bytes.len() as u64).to_be_bytes());

    // Hash path
    hasher.update(path_bytes);

    // Hash children count (8 bytes, big-endian)
    hasher.update(&(children.len() as u64).to_be_bytes());

    // Hash children (already sorted by name)
    for (name, node_id) in children.iter() {
        hasher.update(name.as_bytes());
        hasher.update(b":");
        hasher.update(node_id);
    }

    // Hash metadata (sorted for determinism)
    for (key, value) in metadata.iter() {
        hasher.update(key.as_bytes());
        hasher.update(b":");
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }

    Ok(*hasher.finalize().as_bytes())
}

/// Compute content hash for file bytes
///
/// Uses BLAKE3 to hash file content deterministically.
pub fn compute_content_hash(content: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(content);
    *hasher.finalize().as_bytes()
}

/// Compute a generic hash of arbitrary data
pub fn compute_hash(data: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(data);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_content_hash_deterministic() {
        let content = b"test content";
        let hash1 = compute_content_hash(content);
        let hash2 = compute_content_hash(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_file_node_id_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let content_hash = compute_content_hash(b"test content");
        let metadata = BTreeMap::new();

        let node_id1 = compute_file_node_id(&test_file, &content_hash, &metadata).unwrap();
        let node_id2 = compute_file_node_id(&test_file, &content_hash, &metadata).unwrap();

        assert_eq!(node_id1, node_id2);
    }

    #[test]
    fn test_file_node_id_different_content_different_id() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        let content_hash1 = compute_content_hash(b"test content");
        let content_hash2 = compute_content_hash(b"different content");
        let metadata = BTreeMap::new();

        let node_id1 = compute_file_node_id(&test_file, &content_hash1, &metadata).unwrap();
        let node_id2 = compute_file_node_id(&test_file, &content_hash2, &metadata).unwrap();

        assert_ne!(node_id1, node_id2);
    }

    #[test]
    fn test_directory_node_id_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test_dir");
        fs::create_dir(&test_dir).unwrap();

        let children = vec![
            ("file1.txt".to_string(), [1u8; 32]),
            ("file2.txt".to_string(), [2u8; 32]),
        ];
        let metadata = BTreeMap::new();

        let node_id1 = compute_directory_node_id(&test_dir, &children, &metadata).unwrap();
        let node_id2 = compute_directory_node_id(&test_dir, &children, &metadata).unwrap();

        assert_eq!(node_id1, node_id2);
    }

    #[test]
    fn test_directory_node_id_different_children_different_id() {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test_dir");
        fs::create_dir(&test_dir).unwrap();

        let children1 = vec![("file1.txt".to_string(), [1u8; 32])];
        let children2 = vec![
            ("file1.txt".to_string(), [1u8; 32]),
            ("file2.txt".to_string(), [2u8; 32]),
        ];
        let metadata = BTreeMap::new();

        let node_id1 = compute_directory_node_id(&test_dir, &children1, &metadata).unwrap();
        let node_id2 = compute_directory_node_id(&test_dir, &children2, &metadata).unwrap();

        assert_ne!(node_id1, node_id2);
    }
}
