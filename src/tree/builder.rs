//! Tree builder for constructing filesystem Merkle trees

use crate::error::StorageError;
use crate::tree::hasher;
use crate::tree::node::{DirectoryNode, FileNode, MerkleNode};
use crate::tree::path;
use crate::tree::walker::{Entry, Walker, WalkerConfig};
use crate::types::NodeID;
use hex;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, error, info, instrument, trace, warn};

/// Complete Merkle tree structure
#[derive(Debug, Clone)]
pub struct Tree {
    /// Root directory NodeID
    pub root_id: NodeID,
    /// Map of NodeID to MerkleNode
    pub nodes: HashMap<NodeID, MerkleNode>,
    /// Map of NodeID to parent NodeID (for fast parent lookups)
    parent_map: HashMap<NodeID, NodeID>,
}

impl Tree {
    /// Find the parent NodeID for a given node
    ///
    /// Returns None if the node is the root or not found.
    pub fn find_parent(&self, node_id: &NodeID) -> Option<NodeID> {
        self.parent_map.get(node_id).copied()
    }

    /// Get all children NodeIDs for a given node
    ///
    /// Returns an empty vector if the node is a file or not found.
    pub fn get_children(&self, node_id: &NodeID) -> Vec<NodeID> {
        match self.nodes.get(node_id) {
            Some(MerkleNode::Directory(dir)) => {
                dir.children.iter().map(|(_, child_id)| *child_id).collect()
            }
            _ => vec![],
        }
    }

    /// Return the NodeID of the .gitignore file node if present in the tree.
    pub fn find_gitignore_node_id(&self) -> Option<NodeID> {
        use std::ffi::OsStr;
        for (node_id, node) in &self.nodes {
            let path = match node {
                MerkleNode::File(f) => &f.path,
                MerkleNode::Directory(d) => &d.path,
            };
            if path.file_name() == Some(OsStr::new(".gitignore")) {
                return Some(*node_id);
            }
        }
        None
    }
}

/// Tree builder for constructing filesystem Merkle trees
pub struct TreeBuilder {
    root: PathBuf,
    walker_config: Option<WalkerConfig>,
}

impl TreeBuilder {
    /// Create a new tree builder for the given root path
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            walker_config: None,
        }
    }

    /// Set walker config (ignore patterns, etc.). When set, the walker uses this config
    /// instead of the default.
    pub fn with_walker_config(mut self, config: WalkerConfig) -> Self {
        self.walker_config = Some(config);
        self
    }

    /// Build the complete Merkle tree from the filesystem
    ///
    /// This processes files and directories bottom-up to compute NodeIDs,
    /// ensuring that directory NodeIDs depend on their children's NodeIDs.
    #[instrument(skip(self), fields(workspace = %self.root.display()))]
    pub fn build(&self) -> Result<Tree, StorageError> {
        let start = Instant::now();
        info!("Starting tree build");

        // Step 1: Walk filesystem and collect entries
        let walker = match &self.walker_config {
            Some(config) => Walker::with_config(self.root.clone(), config.clone()),
            None => Walker::new(self.root.clone()),
        };
        let entries = match walker.walk() {
            Ok(e) => {
                debug!(entry_count = e.len(), "Walked filesystem");
                e
            }
            Err(e) => {
                error!("Filesystem walk failed: {}", e);
                return Err(e);
            }
        };

        // Step 2: Separate files and directories
        let mut files = Vec::new();
        let mut directories = Vec::new();

        for entry in entries {
            match entry {
                Entry::File { path, size } => files.push((path, size)),
                Entry::Directory { path } => directories.push(path),
            }
        }

        // Step 3: Process files first (they have no dependencies)
        let mut node_map: HashMap<PathBuf, NodeID> = HashMap::new();
        let mut nodes: HashMap<NodeID, MerkleNode> = HashMap::new();

        for (file_path, size) in files {
            let (node_id, file_node) = self.hash_file(&file_path, size)?;
            // Canonicalize path for consistent lookups
            let canonical_path = path::canonicalize_path(&file_path)?;
            node_map.insert(canonical_path, node_id);
            nodes.insert(node_id, MerkleNode::File(file_node));
        }

        // Step 4: Process directories bottom-up (deepest first)
        // Add root directory to the list if it's not already there
        if !directories.contains(&self.root) {
            directories.push(self.root.clone());
        }

        // Sort directories by depth (deepest first) to ensure children are processed before parents
        directories.sort_by(|a, b| {
            let depth_a = a.components().count();
            let depth_b = b.components().count();
            depth_b.cmp(&depth_a) // Reverse order: deepest first
        });

        for dir_path in directories {
            let (node_id, dir_node) = self.hash_directory(&dir_path, &node_map)?;
            // Canonicalize path for consistent lookups
            let canonical_path = path::canonicalize_path(&dir_path)?;
            node_map.insert(canonical_path, node_id);
            nodes.insert(node_id, MerkleNode::Directory(dir_node));
        }

        // Step 5: Build parent map for fast parent lookups
        // For each directory, map all its children to the directory as their parent
        let mut parent_map: HashMap<NodeID, NodeID> = HashMap::new();

        for (node_id, node) in &nodes {
            if let MerkleNode::Directory(dir) = node {
                // All children of this directory have this directory as their parent
                for (_, child_node_id) in &dir.children {
                    parent_map.insert(*child_node_id, *node_id);
                }
            }
        }

        // Step 6: Get root directory NodeID
        let canonical_root = path::canonicalize_path(&self.root)?;
        let root_id = node_map.get(&canonical_root).copied().ok_or_else(|| {
            error!("Root directory not found in node map: {:?}", canonical_root);
            StorageError::InvalidPath(format!(
                "Root directory not found in node map: {:?}",
                canonical_root
            ))
        })?;

        let duration = start.elapsed();
        info!(
            node_count = nodes.len(),
            root_id = %hex::encode(root_id),
            duration_ms = duration.as_millis(),
            "Tree build completed"
        );

        Ok(Tree {
            root_id,
            nodes,
            parent_map,
        })
    }

    /// Compute root hash of the workspace
    ///
    /// This is a convenience method that builds the tree and returns the root NodeID.
    pub fn compute_root(&self) -> Result<NodeID, StorageError> {
        let tree = self.build()?;
        Ok(tree.root_id)
    }

    /// Hash a file and compute its NodeID
    #[instrument(skip(self), fields(path = %file_path.display()))]
    fn hash_file(&self, file_path: &Path, size: u64) -> Result<(NodeID, FileNode), StorageError> {
        trace!("Hashing file");
        // Read file content
        let content = std::fs::read(file_path).map_err(|e| {
            error!("Failed to read file: {}", e);
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read file {:?}: {}", file_path, e),
            ))
        })?;

        // Compute content hash
        let content_hash = hasher::compute_content_hash(&content);
        trace!(content_hash = %hex::encode(content_hash), "Computed content hash");

        // Extract metadata (currently empty, can be extended)
        let metadata = BTreeMap::new();

        // Compute NodeID
        let node_id = hasher::compute_file_node_id(file_path, &content_hash, &metadata)?;

        // Create FileNode
        let file_node = FileNode {
            path: file_path.to_path_buf(),
            content_hash,
            size,
            metadata,
        };

        Ok((node_id, file_node))
    }

    /// Hash a directory and compute its NodeID
    ///
    /// Requires that all children have already been processed and are in node_map.
    fn hash_directory(
        &self,
        dir_path: &Path,
        node_map: &HashMap<PathBuf, NodeID>,
    ) -> Result<(NodeID, DirectoryNode), StorageError> {
        // Read directory contents
        let dir_entries = std::fs::read_dir(dir_path).map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read directory {:?}: {}", dir_path, e),
            ))
        })?;

        // Collect children (name, NodeID) pairs
        let mut children = Vec::new();

        for entry in dir_entries {
            let entry = entry.map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to read directory entry in {:?}: {}", dir_path, e),
                ))
            })?;

            let child_path = entry.path();
            let child_name = entry.file_name().to_string_lossy().to_string();

            // Canonicalize child path for consistent lookup
            let canonical_child_path = match path::canonicalize_path(&child_path) {
                Ok(p) => p,
                Err(_) => {
                    // Skip if canonicalization fails (might be symlink or special file)
                    continue;
                }
            };

            // Look up child NodeID in node_map
            if let Some(&child_node_id) = node_map.get(&canonical_child_path) {
                children.push((child_name, child_node_id));
            } else {
                // Child not found - this can happen if we're processing the root
                // and haven't processed all children yet, or if there's a mismatch
                // Skip for now, but this indicates a problem with the algorithm
                // For now, we'll skip missing children (they might be symlinks or special files)
                continue;
            }
        }

        // Sort children by name for determinism
        children.sort_by(|a, b| a.0.cmp(&b.0));

        // Extract metadata (currently empty, can be extended)
        let metadata = BTreeMap::new();

        // Compute NodeID
        let node_id = hasher::compute_directory_node_id(dir_path, &children, &metadata)?;

        // Create DirectoryNode
        let dir_node = DirectoryNode {
            path: dir_path.to_path_buf(),
            children,
            metadata,
        };

        Ok((node_id, dir_node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_build_tree_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        fs::write(root.join("test.txt"), "test content").unwrap();

        let builder = TreeBuilder::new(root.clone());
        let tree = builder.build().unwrap();

        // Should have one file node
        assert_eq!(tree.nodes.len(), 2); // 1 file + 1 root directory

        // Root should exist
        assert!(tree.nodes.contains_key(&tree.root_id));
    }

    #[test]
    fn test_build_tree_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        fs::write(root.join("file1.txt"), "content1").unwrap();
        fs::write(root.join("file2.txt"), "content2").unwrap();

        let builder = TreeBuilder::new(root.clone());
        let tree = builder.build().unwrap();

        // Should have 2 files + 1 root directory = 3 nodes
        assert_eq!(tree.nodes.len(), 3);
    }

    #[test]
    fn test_build_tree_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        fs::create_dir(root.join("dir1")).unwrap();
        fs::write(root.join("dir1").join("file.txt"), "content").unwrap();
        fs::write(root.join("file.txt"), "root content").unwrap();

        let builder = TreeBuilder::new(root.clone());
        let tree = builder.build().unwrap();

        // Should have: 2 files + 1 subdirectory + 1 root directory = 4 nodes
        assert!(tree.nodes.len() >= 4);
    }

    #[test]
    fn test_compute_root_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        fs::write(root.join("test.txt"), "test content").unwrap();

        let builder = TreeBuilder::new(root.clone());
        let root1 = builder.compute_root().unwrap();
        let root2 = builder.compute_root().unwrap();

        // Same filesystem should produce same root
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_compute_root_changes_with_content() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        fs::write(root.join("test.txt"), "content1").unwrap();

        let builder = TreeBuilder::new(root.clone());
        let root1 = builder.compute_root().unwrap();

        // Change file content
        fs::write(root.join("test.txt"), "content2").unwrap();

        let root2 = builder.compute_root().unwrap();

        // Different content should produce different root
        assert_ne!(root1, root2);
    }

    #[test]
    fn test_compute_root_changes_with_structure() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        fs::write(root.join("file1.txt"), "content").unwrap();

        let builder = TreeBuilder::new(root.clone());
        let root1 = builder.compute_root().unwrap();

        // Add another file
        fs::write(root.join("file2.txt"), "content").unwrap();

        let root2 = builder.compute_root().unwrap();

        // Different structure should produce different root
        assert_ne!(root1, root2);
    }
}
