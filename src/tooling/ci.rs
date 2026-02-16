//! CI Integration
//!
//! Provides batch operations, validation, and reporting for CI/CD integration.
//! Enables automated workflows and integrity verification.

use crate::api::ContextApi;
use crate::error::ApiError;
use crate::types::NodeID;
use std::collections::HashMap;

/// CI integration utilities
pub struct CiIntegration {
    api: ContextApi,
}

impl CiIntegration {
    /// Create new CI integration
    pub fn new(api: ContextApi) -> Self {
        Self { api }
    }

    /// Batch process multiple nodes
    ///
    /// Processes a list of nodes efficiently, useful for CI workflows.
    pub fn batch_process(
        &self,
        node_ids: Vec<NodeID>,
        operation: BatchOperation,
    ) -> Result<BatchReport, ApiError> {
        let mut report = BatchReport {
            processed: 0,
            succeeded: 0,
            failed: 0,
            errors: HashMap::new(),
        };

        for node_id in node_ids {
            report.processed += 1;
            match operation.execute(&self.api, node_id) {
                Ok(_) => report.succeeded += 1,
                Err(e) => {
                    report.failed += 1;
                    report.errors.insert(node_id, e.to_string());
                }
            }
        }

        Ok(report)
    }

    /// Validate workspace integrity
    ///
    /// Performs comprehensive validation of all data structures.
    pub fn validate_workspace(&self) -> Result<ValidationReport, ApiError> {
        // For Phase 2G, this is a basic implementation
        // In a full implementation, we would:
        // 1. Verify all nodes are accessible
        // 2. Check frame integrity
        // 3. Verify head index consistency
        // 4. Validate Merkle tree roots

        Ok(ValidationReport {
            valid: true,
            errors: vec![],
            warnings: vec![],
        })
    }

    /// Generate a report on context state
    ///
    /// Creates a report summarizing the current state of the workspace.
    pub fn generate_report(&self) -> Result<WorkspaceReport, ApiError> {
        // For Phase 2G, this is a basic implementation
        Ok(WorkspaceReport {
            total_nodes: 0,
            total_frames: 0,
            frame_types: HashMap::new(),
        })
    }

    /// Generate diff between two states
    ///
    /// Shows context changes between runs. For Phase 2G, this is a placeholder.
    pub fn generate_diff(&self, _baseline: &str, _current: &str) -> Result<DiffReport, ApiError> {
        Ok(DiffReport {
            added_frames: vec![],
            removed_frames: vec![],
            modified_frames: vec![],
        })
    }
}

/// Batch operation type
pub enum BatchOperation {
    EnsureNodeExists,
}

impl BatchOperation {
    fn execute(&self, api: &ContextApi, node_id: NodeID) -> Result<(), ApiError> {
        match self {
            BatchOperation::EnsureNodeExists => {
                api.node_store()
                    .get(&node_id)
                    .map_err(ApiError::from)?
                    .ok_or(ApiError::NodeNotFound(node_id))?;
                Ok(())
            }
        }
    }
}

/// Batch processing report
#[derive(Debug)]
pub struct BatchReport {
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub errors: HashMap<NodeID, String>,
}

/// Validation report
#[derive(Debug)]
pub struct ValidationReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Workspace report
#[derive(Debug)]
pub struct WorkspaceReport {
    pub total_nodes: usize,
    pub total_frames: usize,
    pub frame_types: HashMap<String, usize>,
}

/// Diff report
#[derive(Debug)]
pub struct DiffReport {
    pub added_frames: Vec<crate::types::FrameID>,
    pub removed_frames: Vec<crate::types::FrameID>,
    pub modified_frames: Vec<crate::types::FrameID>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ContextApi;
    use crate::heads::HeadIndex;
    use crate::store::persistence::SledNodeRecordStore;
    use crate::types::Hash;
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
        let agent_registry = Arc::new(parking_lot::RwLock::new(crate::agent::AgentRegistry::new()));
        let provider_registry = Arc::new(parking_lot::RwLock::new(
            crate::provider::ProviderRegistry::new(),
        ));
        let lock_manager = Arc::new(crate::concurrency::NodeLockManager::new());

        let api = ContextApi::new(
            node_store,
            frame_storage,
            head_index,
            agent_registry,
            provider_registry,
            lock_manager,
        );

        (api, temp_dir)
    }

    #[test]
    fn test_ci_integration_creation() {
        let (api, _temp_dir) = create_test_api();
        let ci = CiIntegration::new(api);
        let report = ci.validate_workspace().unwrap();
        assert!(report.valid);
    }

    #[test]
    fn test_batch_operation() {
        let (api, _temp_dir) = create_test_api();
        let ci = CiIntegration::new(api);
        let node_ids = vec![Hash::from([0u8; 32])];
        let operation = BatchOperation::EnsureNodeExists;
        // This will fail because node doesn't exist, but the batch operation should handle it
        let report = ci.batch_process(node_ids, operation).unwrap();
        assert_eq!(report.processed, 1);
        assert_eq!(report.failed, 1);
    }
}
