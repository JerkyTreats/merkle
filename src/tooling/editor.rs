//! Editor Integration Hooks
//!
//! Provides file watchers and change notifications for editor integration.
//! Monitors filesystem changes and triggers regeneration when needed.

use crate::api::ContextApi;
use crate::error::ApiError;
use crate::types::NodeID;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use tracing::error;

/// Editor integration hooks
pub struct EditorHooks {
    #[allow(dead_code)]
    api: ContextApi,
    workspace_root: PathBuf,
}

impl EditorHooks {
    /// Create new editor hooks
    pub fn new(api: ContextApi, workspace_root: PathBuf) -> Self {
        Self {
            api,
            workspace_root,
        }
    }

    /// Start watching for filesystem changes
    ///
    /// Returns a channel receiver for change events and a watcher handle.
    /// The watcher will monitor the workspace and send events when files change.
    pub fn watch(
        &self,
    ) -> Result<(mpsc::Receiver<notify::Result<Event>>, RecommendedWatcher), ApiError> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = tx.send(res) {
                error!("Error sending watch event: {}", e);
            }
        })
        .map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create watcher: {}", e),
            )))
        })?;

        watcher
            .watch(&self.workspace_root, RecursiveMode::Recursive)
            .map_err(|e| {
                ApiError::StorageError(crate::error::StorageError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to watch directory: {}", e),
                )))
            })?;

        Ok((rx, watcher))
    }

    /// Handle a filesystem change event
    ///
    /// This method processes change events and triggers regeneration if needed.
    /// For Phase 2G, this is a basic implementation that can be extended.
    pub fn handle_event(&self, event: Event) -> Result<Option<NodeID>, ApiError> {
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                // For Phase 2G, we just return that a change occurred
                // In a full implementation, we would:
                // 1. Map file path to NodeID
                // 2. Trigger regeneration for that node
                // 3. Return the NodeID that was affected
                Ok(None) // Placeholder - would return NodeID in full implementation
            }
            _ => Ok(None),
        }
    }

    /// Register a callback for node changes
    ///
    /// This allows editors to register callbacks that will be called when
    /// nodes change. For Phase 2G, this is a placeholder for future implementation.
    pub fn on_node_change<F>(&self, _callback: F)
    where
        F: Fn(NodeID) + Send + Sync + 'static,
    {
        // TODO: Implement callback registration
        // This would store callbacks and invoke them when nodes change
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ContextApi;
    use crate::heads::HeadIndex;
    use crate::regeneration::BasisIndex;
    use crate::store::persistence::SledNodeRecordStore;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_api() -> (ContextApi, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("store");
        let node_store = Arc::new(SledNodeRecordStore::new(&store_path).unwrap());
        let frame_storage_path = temp_dir.path().join("frames");
        std::fs::create_dir_all(&frame_storage_path).unwrap();
        let frame_storage =
            Arc::new(crate::frame::storage::FrameStorage::new(&frame_storage_path).unwrap());
        let head_index = Arc::new(parking_lot::RwLock::new(HeadIndex::new()));
        let basis_index = Arc::new(parking_lot::RwLock::new(BasisIndex::new()));
        let agent_registry = Arc::new(parking_lot::RwLock::new(crate::agent::AgentRegistry::new()));
        let provider_registry = Arc::new(parking_lot::RwLock::new(
            crate::provider::ProviderRegistry::new(),
        ));
        let lock_manager = Arc::new(crate::concurrency::NodeLockManager::new());

        let api = ContextApi::new(
            node_store,
            frame_storage,
            head_index,
            basis_index,
            agent_registry,
            provider_registry,
            lock_manager,
        );

        (api, temp_dir)
    }

    #[test]
    fn test_editor_hooks_creation() {
        let (api, temp_dir) = create_test_api();
        let hooks = EditorHooks::new(api, temp_dir.path().to_path_buf());
        // Just test that creation works
        assert_eq!(hooks.workspace_root, temp_dir.path());
    }
}
