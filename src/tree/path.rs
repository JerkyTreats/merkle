//! Path canonicalization and normalization utilities

use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// Canonicalize and normalize a path for deterministic hashing
///
/// This function:
/// 1. Canonicalizes the path (resolves symlinks, `..`, `.`)
/// 2. Normalizes Unicode to NFC
/// 3. Removes trailing slashes (except root)
/// 4. Converts to a consistent representation
pub fn canonicalize_path(path: &Path) -> Result<PathBuf, crate::error::StorageError> {
    // Use dunce for cross-platform canonicalization
    let canonical = dunce::canonicalize(path).map_err(|e| {
        crate::error::StorageError::InvalidPath(format!("Failed to canonicalize path: {}", e))
    })?;

    // Convert to string for Unicode normalization
    let path_str = canonical.to_string_lossy();

    // Normalize Unicode to NFC (Canonical Composition)
    let normalized: String = path_str.nfc().collect();

    // Convert back to PathBuf
    let mut normalized_path = PathBuf::from(normalized);

    // Remove trailing slashes (except root)
    if normalized_path.as_os_str().len() > 1 {
        let mut path_str = normalized_path.to_string_lossy().to_string();
        while path_str.ends_with('/') || path_str.ends_with('\\') {
            path_str.pop();
        }
        normalized_path = PathBuf::from(path_str);
    }

    Ok(normalized_path)
}

/// Normalize a path string for hashing (without filesystem access)
///
/// This is used when we already have a canonical path and just need
/// to normalize Unicode and format consistently.
pub fn normalize_path_string(path: &str) -> String {
    // Normalize Unicode to NFC
    let normalized: String = path.nfc().collect();

    // Remove trailing slashes (except root)
    let mut result = normalized;
    if result.len() > 1 {
        while result.ends_with('/') || result.ends_with('\\') {
            result.pop();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_path_normalization_removes_trailing_slash() {
        let normalized = normalize_path_string("/some/path/");
        assert!(!normalized.ends_with('/'));
        assert_eq!(normalized, "/some/path");
    }

    #[test]
    fn test_path_normalization_preserves_root() {
        let normalized = normalize_path_string("/");
        assert_eq!(normalized, "/");
    }

    #[test]
    fn test_unicode_normalization() {
        // Test that Unicode is normalized to NFC
        let path1 = normalize_path_string("/café");
        let path2 = normalize_path_string("/cafe\u{0301}"); // e + combining acute
        assert_eq!(path1, path2);
    }

    #[test]
    fn test_canonicalize_path() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        let canonical = canonicalize_path(&test_file).unwrap();
        assert!(canonical.is_absolute());
        assert!(!canonical.to_string_lossy().ends_with('/'));
    }
}
