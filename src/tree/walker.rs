//! Filesystem walker for traversing directory structures

use crate::error::StorageError;
use std::path::PathBuf;
use walkdir::{DirEntry, WalkDir};

/// Filesystem entry types
#[derive(Debug, Clone)]
pub enum Entry {
    /// A file entry with its path and size
    File { path: PathBuf, size: u64 },
    /// A directory entry with its path
    Directory { path: PathBuf },
}

/// Filesystem walker configuration
#[derive(Debug, Clone)]
pub struct WalkerConfig {
    /// Whether to follow symbolic links (default: false for determinism)
    pub follow_symlinks: bool,
    /// Patterns to ignore (e.g., ".git", "target/", "node_modules/")
    pub ignore_patterns: Vec<String>,
    /// Maximum depth to traverse (None = unlimited)
    pub max_depth: Option<usize>,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            ignore_patterns: vec![
                ".git".to_string(),
                "target".to_string(),
                "node_modules".to_string(),
                ".cargo".to_string(),
            ],
            max_depth: None,
        }
    }
}

/// Filesystem walker
pub struct Walker {
    root: PathBuf,
    config: WalkerConfig,
}

impl Walker {
    /// Create a new walker for the given root path
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            config: WalkerConfig::default(),
        }
    }

    /// Create a walker with custom configuration
    pub fn with_config(root: PathBuf, config: WalkerConfig) -> Self {
        Self { root, config }
    }

    /// Walk the filesystem and collect all entries
    ///
    /// Returns entries sorted by path for determinism.
    pub fn walk(&self) -> Result<Vec<Entry>, StorageError> {
        let mut entries = Vec::new();

        let walker = WalkDir::new(&self.root)
            .follow_links(self.config.follow_symlinks)
            .max_depth(self.config.max_depth.unwrap_or(usize::MAX));

        for entry in walker {
            let entry = entry.map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to walk directory: {}", e),
                ))
            })?;

            // Skip if matches ignore pattern
            if self.should_ignore(&entry) {
                continue;
            }

            let path = entry.path().to_path_buf();

            // Skip the root directory itself (we only want its contents)
            if path == self.root {
                continue;
            }

            let metadata = entry.metadata().map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to read metadata for {:?}: {}", path, e),
                ))
            })?;

            if metadata.is_file() {
                entries.push(Entry::File {
                    path,
                    size: metadata.len(),
                });
            } else if metadata.is_dir() {
                entries.push(Entry::Directory { path });
            }
            // Skip symlinks if not following them
        }

        // Sort entries by path for determinism
        entries.sort_by(|a, b| {
            let path_a = match a {
                Entry::File { path, .. } | Entry::Directory { path } => path,
            };
            let path_b = match b {
                Entry::File { path, .. } | Entry::Directory { path } => path,
            };
            path_a.cmp(path_b)
        });

        Ok(entries)
    }

    /// Check if an entry should be ignored based on ignore patterns
    fn should_ignore(&self, entry: &DirEntry) -> bool {
        let path = entry.path();
        let path_str = path.to_string_lossy();

        for pattern in &self.config.ignore_patterns {
            // Check if path contains the ignore pattern
            if path_str.contains(pattern) {
                return true;
            }

            // Check if any component matches
            for component in path.components() {
                if let std::path::Component::Normal(name) = component {
                    if name.to_string_lossy() == pattern.as_str() {
                        return true;
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_walker_collects_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create test files
        fs::write(root.join("file1.txt"), "content1").unwrap();
        fs::write(root.join("file2.txt"), "content2").unwrap();

        let walker = Walker::new(root);
        let entries = walker.walk().unwrap();

        assert_eq!(entries.len(), 2);
        let mut file_paths: Vec<_> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::File { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect();
        file_paths.sort();
        assert!(file_paths[0].ends_with("file1.txt"));
        assert!(file_paths[1].ends_with("file2.txt"));
    }

    #[test]
    fn test_walker_collects_directories() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create test directory structure
        fs::create_dir(root.join("dir1")).unwrap();
        fs::create_dir(root.join("dir2")).unwrap();
        fs::write(root.join("dir1").join("file.txt"), "content").unwrap();

        let walker = Walker::new(root);
        let entries = walker.walk().unwrap();

        // Should have 1 directory (dir1) and 1 file
        // Note: dir2 is empty, so it should still be collected
        let dirs: Vec<_> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Directory { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        assert!(dirs.len() >= 1);
    }

    #[test]
    fn test_walker_ignores_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create files including ignored patterns
        fs::write(root.join("file.txt"), "content").unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("config"), "git config").unwrap();

        let walker = Walker::new(root);
        let entries = walker.walk().unwrap();

        // Should only have file.txt, not .git files
        let paths: Vec<_> = entries
            .iter()
            .map(|e| match e {
                Entry::File { path, .. } | Entry::Directory { path } => path.clone(),
            })
            .collect();

        assert!(!paths.iter().any(|p| p.to_string_lossy().contains(".git")));
        assert!(paths.iter().any(|p| p.ends_with("file.txt")));
    }

    #[test]
    fn test_walker_deterministic_ordering() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create files in non-alphabetical order
        fs::write(root.join("z_file.txt"), "content").unwrap();
        fs::write(root.join("a_file.txt"), "content").unwrap();
        fs::write(root.join("m_file.txt"), "content").unwrap();

        let walker = Walker::new(root);
        let entries1 = walker.walk().unwrap();
        let entries2 = walker.walk().unwrap();

        // Should be identical
        assert_eq!(entries1.len(), entries2.len());
        for (e1, e2) in entries1.iter().zip(entries2.iter()) {
            let path1 = match e1 {
                Entry::File { path, .. } | Entry::Directory { path } => path,
            };
            let path2 = match e2 {
                Entry::File { path, .. } | Entry::Directory { path } => path,
            };
            assert_eq!(path1, path2);
        }

        // Should be sorted
        let paths: Vec<_> = entries1
            .iter()
            .map(|e| match e {
                Entry::File { path, .. } | Entry::Directory { path } => path.clone(),
            })
            .collect();
        let mut sorted_paths = paths.clone();
        sorted_paths.sort();
        assert_eq!(paths, sorted_paths);
    }
}
