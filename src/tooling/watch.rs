//! Watch Mode Daemon
//!
//! Implements a long-lived daemon process that monitors the workspace for filesystem changes
//! and automatically updates the Merkle tree and triggers node regeneration.

use crate::api::ContextApi;
use crate::error::ApiError;
use crate::store::{NodeRecord, NodeRecordStore};
use crate::tree::builder::TreeBuilder;
use crate::tree::path::canonicalize_path;
use crate::types::NodeID;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use tracing::{error, info, warn};

/// Watch mode configuration
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Workspace root directory
    pub workspace_root: PathBuf,
    /// Debounce window in milliseconds
    pub debounce_ms: u64,
    /// Batch window in milliseconds
    pub batch_window_ms: u64,
    /// Maximum events per batch
    pub max_batch_size: usize,
    /// Enable automatic regeneration
    pub regeneration_enabled: bool,
    /// Recursive regeneration (regenerate parent frames)
    pub recursive: bool,
    /// Maximum propagation depth for recursive regeneration
    pub max_depth: usize,
    /// Agent ID for automatic regeneration
    pub agent_id: String,
    /// Ignore patterns (glob patterns)
    pub ignore_patterns: Vec<String>,
    /// Maximum event queue size
    pub max_queue_size: usize,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
            debounce_ms: 100,
            batch_window_ms: 50,
            max_batch_size: 100,
            regeneration_enabled: true,
            recursive: false,
            max_depth: 3,
            agent_id: "watch-daemon".to_string(),
            ignore_patterns: vec![
                "**/.git/**".to_string(),
                "**/.merkle/**".to_string(),
                "**/target/**".to_string(),
                "**/node_modules/**".to_string(),
                "**/.DS_Store".to_string(),
                "**/*.swp".to_string(),
                "**/*.tmp".to_string(),
            ],
            max_queue_size: 10000,
        }
    }
}

/// Filesystem change event
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChangeEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

/// Event batcher for grouping and debouncing events
struct EventBatcher {
    config: WatchConfig,
    pending_events: HashMap<PathBuf, ChangeEvent>,
    last_event_time: HashMap<PathBuf, Instant>,
}

impl EventBatcher {
    fn new(config: WatchConfig) -> Self {
        Self {
            config,
            pending_events: HashMap::new(),
            last_event_time: HashMap::new(),
        }
    }

    /// Add an event to the batcher
    ///
    /// Returns true if the event should be processed immediately (batch ready)
    fn add_event(&mut self, event: ChangeEvent) -> bool {
        let path = match &event {
            ChangeEvent::Created(p) | ChangeEvent::Modified(p) | ChangeEvent::Removed(p) => p.clone(),
            ChangeEvent::Renamed { to, .. } => to.clone(),
        };

        // Check if we should ignore this path
        if self.should_ignore(&path) {
            return false;
        }

        let now = Instant::now();
        let debounce_window = Duration::from_millis(self.config.debounce_ms);

        // Check if we have a recent event for this path
        if let Some(last_time) = self.last_event_time.get(&path) {
            if now.duration_since(*last_time) < debounce_window {
                // Update the event (latest event wins)
                self.pending_events.insert(path.clone(), event);
                return false; // Still debouncing
            }
        }

        // Add event
        self.pending_events.insert(path.clone(), event);
        self.last_event_time.insert(path, now);

        // Check if batch is ready
        self.pending_events.len() >= self.config.max_batch_size
    }

    /// Get and clear pending events
    fn take_batch(&mut self) -> Vec<ChangeEvent> {
        let events: Vec<_> = self.pending_events.values().cloned().collect();
        self.pending_events.clear();
        self.last_event_time.clear();
        events
    }

    /// Check if a path should be ignored
    fn should_ignore(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in &self.config.ignore_patterns {
            // Simple glob matching (can be enhanced with proper glob library)
            if self.matches_pattern(&path_str, pattern) {
                return true;
            }
        }
        false
    }

    /// Simple glob pattern matching
    fn matches_pattern(&self, path: &str, pattern: &str) -> bool {
        // Simple glob matching: support ** and *
        // Convert to forward slashes for consistency
        let path_normalized = path.replace('\\', "/");
        let pattern_normalized = pattern.replace('\\', "/");

        // Handle ** (matches any number of directories)
        if pattern_normalized.contains("**") {
            let parts: Vec<&str> = pattern_normalized.split("**").collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                if prefix.is_empty() {
                    return path_normalized.contains(suffix);
                } else if suffix.is_empty() {
                    return path_normalized.starts_with(prefix);
                } else {
                    return path_normalized.starts_with(prefix) && path_normalized.contains(suffix);
                }
            }
        }

        // Handle simple * matching (single directory level)
        if pattern_normalized.contains('*') {
            // For now, use simple prefix/suffix matching
            // A proper implementation would use a glob library
            let parts: Vec<&str> = pattern_normalized.split('*').collect();
            if parts.len() == 2 {
                return path_normalized.starts_with(parts[0]) && path_normalized.contains(parts[1]);
            }
        }

        // Exact match
        path_normalized == pattern_normalized || path_normalized.contains(&pattern_normalized)
    }
}

/// Watch mode daemon
pub struct WatchDaemon {
    api: Arc<ContextApi>,
    config: WatchConfig,
    running: Arc<RwLock<bool>>,
}

impl WatchDaemon {
    /// Create a new watch daemon
    pub fn new(api: Arc<ContextApi>, config: WatchConfig) -> Self {
        Self {
            api,
            config,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the watch daemon
    ///
    /// This will:
    /// 1. Build the initial tree
    /// 2. Start the file watcher
    /// 3. Process events in a loop
    pub fn start(&self) -> Result<(), ApiError> {
        // Mark as running
        *self.running.write() = true;

        // Build initial tree
        info!("Building initial tree");
        self.build_initial_tree()?;
        info!("Initial tree built successfully");

        // Create file watcher
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = tx.send(res) {
                error!("Error sending watch event: {}", e);
            }
        }).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(
                std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to create watcher: {}", e))
            ))
        })?;

        watcher.watch(&self.config.workspace_root, RecursiveMode::Recursive).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(
                std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to watch directory: {}", e))
            ))
        })?;

        info!(workspace = ?self.config.workspace_root, "Watching workspace");

        // Create batcher with config
        let mut batcher = EventBatcher::new(self.config.clone());
        let batch_window = Duration::from_millis(self.config.batch_window_ms);

        // Event processing loop
        let mut last_batch_time = Instant::now();
        let mut pending_events = Vec::new();

        loop {
            // Check if we should stop
            if !*self.running.read() {
                break;
            }

            // Receive events with timeout
            let timeout = batch_window.saturating_sub(last_batch_time.elapsed());
            match rx.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    // Convert notify event to our ChangeEvent
                    if let Some(change_event) = self.convert_event(event) {
                        if batcher.add_event(change_event.clone()) {
                            // Batch is ready, process immediately
                            pending_events.extend(batcher.take_batch());
                        } else {
                            pending_events.push(change_event);
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("Watch error: {}", e);
                    // Continue watching despite errors
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout - check if we should process batch
                    if !pending_events.is_empty() && last_batch_time.elapsed() >= batch_window {
                        self.process_events(pending_events.drain(..).collect())?;
                        last_batch_time = Instant::now();
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    error!("Watcher channel disconnected");
                    break;
                }
            }

            // Process batch if ready
            if !pending_events.is_empty() && last_batch_time.elapsed() >= batch_window {
                self.process_events(pending_events.drain(..).collect())?;
                last_batch_time = Instant::now();
            }
        }

        Ok(())
    }

    /// Stop the watch daemon
    pub fn stop(&self) {
        *self.running.write() = false;
    }

    /// Build the initial tree from filesystem
    fn build_initial_tree(&self) -> Result<(), ApiError> {
        let builder = TreeBuilder::new(self.config.workspace_root.clone());
        let tree = builder.build().map_err(ApiError::from)?;

        // Populate store with all nodes
        NodeRecord::populate_store_from_tree(
            self.api.node_store().as_ref() as &dyn NodeRecordStore,
            &tree,
        ).map_err(ApiError::from)?;

        Ok(())
    }

    /// Convert notify Event to ChangeEvent
    fn convert_event(&self, event: Event) -> Option<ChangeEvent> {
        match event.kind {
            EventKind::Create(_) => {
                event.paths.first().map(|p| ChangeEvent::Created(p.clone()))
            }
            EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                // Rename events in notify 6.0 are Modify events with Name kind
                if event.paths.len() >= 2 {
                    Some(ChangeEvent::Renamed {
                        from: event.paths[0].clone(),
                        to: event.paths[1].clone(),
                    })
                } else if event.paths.len() == 1 {
                    // Sometimes rename events only have one path
                    event.paths.first().map(|p| ChangeEvent::Modified(p.clone()))
                } else {
                    None
                }
            }
            EventKind::Modify(_) => {
                event.paths.first().map(|p| ChangeEvent::Modified(p.clone()))
            }
            EventKind::Remove(_) => {
                event.paths.first().map(|p| ChangeEvent::Removed(p.clone()))
            }
            _ => None,
        }
    }

    /// Process a batch of change events
    fn process_events(&self, events: Vec<ChangeEvent>) -> Result<(), ApiError> {
        if events.is_empty() {
            return Ok(());
        }

        info!(event_count = events.len(), "Processing change events");

        // Collect all affected paths
        let mut affected_paths = HashSet::new();
        for event in &events {
            match event {
                ChangeEvent::Created(p) | ChangeEvent::Modified(p) | ChangeEvent::Removed(p) => {
                    affected_paths.insert(p.clone());
                }
                ChangeEvent::Renamed { from, to } => {
                    affected_paths.insert(from.clone());
                    affected_paths.insert(to.clone());
                }
            }
        }

        // Update tree for affected paths
        let affected_nodes = self.update_tree_for_paths(&affected_paths)?;

        // Trigger regeneration if enabled
        if self.config.regeneration_enabled {
            for node_id in &affected_nodes {
                let _report = self.api.regenerate(
                    *node_id,
                    self.config.recursive,
                    self.config.agent_id.clone(),
                )?;
                // Log regeneration results if needed
            }
        }

        info!(
            event_count = events.len(),
            affected_nodes = affected_nodes.len(),
            "Processed change events"
        );

        Ok(())
    }

    /// Update tree for affected paths
    ///
    /// Returns the NodeIDs of all affected nodes (changed nodes + ancestors)
    fn update_tree_for_paths(&self, paths: &HashSet<PathBuf>) -> Result<Vec<NodeID>, ApiError> {
        // For now, we'll rebuild the entire tree
        // TODO: Implement incremental updates for better performance
        let builder = TreeBuilder::new(self.config.workspace_root.clone());
        let tree = builder.build().map_err(ApiError::from)?;

        // Collect affected node IDs
        let mut affected_nodes = Vec::new();

        // Update all nodes in the tree
        for (node_id, _node) in &tree.nodes {
            // Check if this node's path is in the affected paths
            let node_record = self.api.node_store().get(node_id).map_err(ApiError::from)?;
            if let Some(record) = node_record {
                // Check if path is affected
                let canonical_path = canonicalize_path(&record.path).unwrap_or(record.path.clone());
                if paths.iter().any(|p| {
                    canonicalize_path(p).map(|cp| cp == canonical_path).unwrap_or(false)
                }) {
                    affected_nodes.push(*node_id);
                }
            }
        }

        // Populate store with updated tree
        NodeRecord::populate_store_from_tree(
            self.api.node_store().as_ref() as &dyn NodeRecordStore,
            &tree,
        ).map_err(ApiError::from)?;

        // Also include all ancestor nodes
        let mut all_affected = affected_nodes.clone();
        for node_id in &affected_nodes {
            self.collect_ancestors(*node_id, &mut all_affected)?;
        }

        Ok(all_affected)
    }

    /// Collect ancestor nodes for a given node
    fn collect_ancestors(&self, node_id: NodeID, collected: &mut Vec<NodeID>) -> Result<(), ApiError> {
        let node_record = self.api.node_store().get(&node_id).map_err(ApiError::from)?;
        if let Some(record) = node_record {
            if let Some(parent_id) = record.parent {
                if !collected.contains(&parent_id) {
                    collected.push(parent_id);
                    // Recursively collect ancestors
                    self.collect_ancestors(parent_id, collected)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_config_default() {
        let config = WatchConfig::default();
        assert_eq!(config.debounce_ms, 100);
        assert_eq!(config.batch_window_ms, 50);
        assert_eq!(config.agent_id, "watch-daemon");
    }

    #[test]
    fn test_event_batcher() {
        let config = WatchConfig::default();
        let mut batcher = EventBatcher::new(config);

        let event1 = ChangeEvent::Modified(PathBuf::from("/test/file1.txt"));
        assert!(!batcher.add_event(event1.clone()));

        // Same event again should be debounced
        assert!(!batcher.add_event(event1));

        let batch = batcher.take_batch();
        assert_eq!(batch.len(), 1);
    }
}
