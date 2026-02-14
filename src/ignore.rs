//! Ignore list for scan and watch.
//!
//! The ignore_list is the single source of ignore rules. It lives at
//! `$XDG_DATA_HOME/merkle/<workspace_path>/ignore_list`. Its default value is
//! .gitignore: when the file does not exist, we behave as if it contained
//! ".gitignore", so workspace .gitignore is only read because it is the default
//! entity in the ignore_list. A line ".gitignore" in the file expands to the
//! patterns from the workspace .gitignore file.
//!
//! When the .gitignore file in the workspace is tracked and its Merkle hash
//! changes (e.g. after scan or watch tree update), we sync .gitignore into the
//! ignore_list: we read .gitignore line by line and write/update a marked block
//! in ignore_list so the list stays in sync without reading .gitignore again
//! until it changes.

use crate::config::xdg;
use crate::error::ApiError;
use crate::types::NodeID;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Built-in ignore patterns (same as WalkerConfig default).
const BUILTIN_DEFAULTS: &[&str] = &[".git", "target", "node_modules", ".cargo"];

/// Special token in ignore_list that means "expand to patterns from workspace .gitignore".
const GITIGNORE_ENTRY: &str = ".gitignore";

/// Markers for the .gitignore block in ignore_list (synced when .gitignore node hash changes).
const GITIGNORE_BLOCK_START: &str = "# .gitignore";
const GITIGNORE_BLOCK_END: &str = "# end .gitignore";

/// Path to the ignore list file for a workspace.
/// `workspace_data_dir(workspace_root).join("ignore_list")`.
pub fn ignore_list_path(workspace_root: &Path) -> Result<PathBuf, ApiError> {
    let data_dir = xdg::workspace_data_dir(workspace_root)?;
    Ok(data_dir.join("ignore_list"))
}

fn gitignore_hash_path(workspace_root: &Path) -> Result<PathBuf, ApiError> {
    let data_dir = xdg::workspace_data_dir(workspace_root)?;
    Ok(data_dir.join("._gitignore_hash"))
}

/// Read workspace .gitignore into a list of pattern strings (minimal parse: trim, skip empty and #).
pub fn read_gitignore_patterns(workspace_root: &Path) -> Vec<String> {
    let gitignore_path = workspace_root.join(".gitignore");
    if !gitignore_path.exists() || !gitignore_path.is_file() {
        return Vec::new();
    }
    let Ok(contents) = fs::read_to_string(&gitignore_path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(line.to_string());
    }
    out
}

/// Sync workspace .gitignore into the ignore_list file: read .gitignore line by line,
/// read ignore_list line by line, replace or insert the marked block, write back.
/// Preserves user-added lines outside the block. Creates ignore_list and parent dir if needed.
pub fn sync_gitignore_to_ignore_list(workspace_root: &Path) -> Result<(), ApiError> {
    let list_path = ignore_list_path(workspace_root)?;
    let gitignore_patterns = read_gitignore_patterns(workspace_root);

    let (before, after) = if list_path.exists() && list_path.is_file() {
        let contents = fs::read_to_string(&list_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read ignore list: {}", e)))?;
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut in_block = false;
        let mut found_block = false;
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed == GITIGNORE_BLOCK_START {
                in_block = true;
                found_block = true;
                continue;
            }
            if trimmed == GITIGNORE_BLOCK_END {
                in_block = false;
                continue;
            }
            if in_block {
                continue;
            }
            if found_block {
                after.push(line.to_string());
            } else {
                before.push(line.to_string());
            }
        }
        (before, after)
    } else {
        if let Some(parent) = list_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    ApiError::ConfigError(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
        }
        (Vec::new(), Vec::new())
    };

    let mut f = fs::File::create(&list_path)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    for line in before {
        writeln!(f, "{}", line)
            .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    }
    writeln!(f, "{}", GITIGNORE_BLOCK_START)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    for p in &gitignore_patterns {
        writeln!(f, "{}", p)
            .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    }
    writeln!(f, "{}", GITIGNORE_BLOCK_END)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    for line in after {
        writeln!(f, "{}", line)
            .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    }
    Ok(())
}

fn read_stored_gitignore_hash(workspace_root: &Path) -> Result<Option<NodeID>, ApiError> {
    let hash_path = gitignore_hash_path(workspace_root)?;
    if !hash_path.exists() || !hash_path.is_file() {
        return Ok(None);
    }
    let hex_str = fs::read_to_string(&hash_path).map_err(|e| {
        ApiError::ConfigError(format!("Failed to read .gitignore hash file: {}", e))
    })?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        return Ok(None);
    }
    let mut arr = [0u8; 32];
    for (i, chunk) in hex_str.as_bytes().chunks(2).enumerate() {
        if i >= 32 || chunk.len() != 2 {
            return Ok(None);
        }
        let s = std::str::from_utf8(chunk)
            .map_err(|_| ApiError::ConfigError("Invalid hex in .gitignore hash".to_string()))?;
        arr[i] = u8::from_str_radix(s, 16)
            .map_err(|_| ApiError::ConfigError("Invalid hex in .gitignore hash".to_string()))?;
    }
    Ok(Some(arr))
}

fn write_stored_gitignore_hash(workspace_root: &Path, node_id: &NodeID) -> Result<(), ApiError> {
    let hash_path = gitignore_hash_path(workspace_root)?;
    if let Some(parent) = hash_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| ApiError::ConfigError(format!("Failed to create directory: {}", e)))?;
        }
    }
    let hex_str = hex::encode(node_id);
    fs::write(&hash_path, hex_str)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write .gitignore hash: {}", e)))?;
    Ok(())
}

/// If the current .gitignore node hash differs from the last stored one, sync
/// .gitignore into ignore_list and update the stored hash. Call after a tree
/// build when you have the current .gitignore node id (if any).
pub fn maybe_sync_gitignore_after_tree(
    workspace_root: &Path,
    current_gitignore_node_id: Option<&NodeID>,
) -> Result<(), ApiError> {
    let stored = read_stored_gitignore_hash(workspace_root)?;
    let current = current_gitignore_node_id.copied();
    let changed = match (stored, current) {
        (None, None) => false,
        (Some(a), Some(b)) => a != b,
        _ => true,
    };
    if !changed {
        return Ok(());
    }
    sync_gitignore_to_ignore_list(workspace_root)?;
    if let Some(id) = current {
        write_stored_gitignore_hash(workspace_root, &id)?;
    } else {
        let hash_path = gitignore_hash_path(workspace_root)?;
        if hash_path.exists() {
            let _ = fs::remove_file(&hash_path);
        }
    }
    Ok(())
}

/// Load ignore patterns for a workspace. The only source read is the ignore_list file.
/// When the ignore_list does not exist, its default value is .gitignore, so we expand
/// to patterns from workspace .gitignore. When it exists, we use the "# .gitignore" block
/// if present (synced from .gitignore when that file changes); otherwise a line ".gitignore"
/// expands to the workspace .gitignore patterns. Built-in defaults are always prepended.
pub fn load_ignore_patterns(workspace_root: &Path) -> Result<Vec<String>, ApiError> {
    let mut patterns: Vec<String> = BUILTIN_DEFAULTS.iter().map(|s| (*s).to_string()).collect();

    let list_path = ignore_list_path(workspace_root)?;

    if !list_path.exists() || !list_path.is_file() {
        patterns.extend(read_gitignore_patterns(workspace_root));
        return Ok(patterns);
    }

    let contents = fs::read_to_string(&list_path).unwrap_or_default();
    let mut in_block = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == GITIGNORE_BLOCK_START {
            in_block = true;
            continue;
        }
        if line == GITIGNORE_BLOCK_END {
            in_block = false;
            continue;
        }
        if line.starts_with('#') && !in_block {
            continue;
        }
        if in_block {
            patterns.push(line.to_string());
            continue;
        }
        if line == GITIGNORE_ENTRY {
            patterns.extend(read_gitignore_patterns(workspace_root));
        } else {
            patterns.push(line.to_string());
        }
    }

    Ok(patterns)
}

/// Read and parse the ignore list file (for list mode).
/// Returns empty vec if file does not exist.
pub fn read_ignore_list(workspace_root: &Path) -> Result<Vec<String>, ApiError> {
    let list_path = ignore_list_path(workspace_root)?;
    if !list_path.exists() || !list_path.is_file() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&list_path)
        .map_err(|e| ApiError::ConfigError(format!("Failed to read ignore list: {}", e)))?;
    let mut out = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(line.to_string());
    }
    Ok(out)
}

/// Remove a path from the ignore list file. Path is normalized to workspace-relative for comparison.
/// Only removes user-added lines (exact match); preserves the # .gitignore block and other lines.
pub fn remove_from_ignore_list(workspace_root: &Path, path: &Path) -> Result<(), ApiError> {
    let list_path = ignore_list_path(workspace_root)?;
    if !list_path.exists() || !list_path.is_file() {
        return Ok(());
    }
    let path_norm = normalize_workspace_relative(workspace_root, path)?;
    let contents = fs::read_to_string(&list_path)
        .map_err(|e| ApiError::ConfigError(format!("Failed to read ignore list: {}", e)))?;
    let mut in_block = false;
    let mut out = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == GITIGNORE_BLOCK_START {
            in_block = true;
            out.push(line.to_string());
            continue;
        }
        if trimmed == GITIGNORE_BLOCK_END {
            in_block = false;
            out.push(line.to_string());
            continue;
        }
        if in_block {
            out.push(line.to_string());
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push(line.to_string());
            continue;
        }
        if trimmed == GITIGNORE_ENTRY {
            out.push(line.to_string());
            continue;
        }
        if trimmed == path_norm {
            continue;
        }
        out.push(line.to_string());
    }
    let new_contents = out.join("\n");
    let has_final_newline = contents.ends_with('\n');
    let to_write = if has_final_newline && !new_contents.is_empty() && !new_contents.ends_with('\n')
    {
        format!("{}\n", new_contents)
    } else {
        new_contents
    };
    fs::write(&list_path, to_write)
        .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    Ok(())
}

/// Append a path to the ignore list file. Creates parent directory and file if needed.
/// Does not deduplicate (optional per spec).
pub fn append_to_ignore_list(workspace_root: &Path, path: &str) -> Result<(), ApiError> {
    let list_path = ignore_list_path(workspace_root)?;
    if let Some(parent) = list_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                ApiError::ConfigError(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }
    let line = format!("{}\n", path);
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&list_path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()))
        .map_err(|e| ApiError::ConfigError(format!("Failed to write ignore list: {}", e)))?;
    Ok(())
}

/// Normalize a path to workspace-relative form (e.g. "node_modules", "src/generated").
/// Returns error if path is outside workspace.
/// If the path exists, canonicalize and strip workspace prefix; otherwise resolve relative to workspace.
pub fn normalize_workspace_relative(
    workspace_root: &Path,
    path: &Path,
) -> Result<String, ApiError> {
    let workspace_canon = workspace_root
        .canonicalize()
        .map_err(|e| ApiError::ConfigError(format!("Failed to canonicalize workspace: {}", e)))?;
    let path_resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };
    let path_canon = if path_resolved.exists() {
        path_resolved
            .canonicalize()
            .map_err(|e| ApiError::ConfigError(format!("Failed to canonicalize path: {}", e)))?
    } else {
        // Path does not exist (e.g. future ignore); must be relative to workspace
        if path.is_absolute() {
            let path_str = path.to_string_lossy();
            let ws_str = workspace_canon.to_string_lossy();
            if !path_str.starts_with(ws_str.as_ref()) {
                return Err(ApiError::ConfigError(
                    "Path is outside workspace".to_string(),
                ));
            }
            let suffix = path_str.strip_prefix(ws_str.as_ref()).unwrap_or(&path_str);
            let suffix = suffix.trim_start_matches(std::path::MAIN_SEPARATOR);
            return Ok(suffix.to_string());
        }
        // Relative path: strip leading ./
        return Ok(path.to_string_lossy().trim_start_matches("./").to_string());
    };
    let path_str = path_canon.to_string_lossy();
    let ws_str = workspace_canon.to_string_lossy();
    if !path_str.starts_with(ws_str.as_ref()) {
        return Err(ApiError::ConfigError(
            "Path is outside workspace".to_string(),
        ));
    }
    let suffix = path_str.strip_prefix(ws_str.as_ref()).unwrap_or(&path_str);
    let suffix = suffix.trim_start_matches(std::path::MAIN_SEPARATOR);
    Ok(suffix.to_string())
}
