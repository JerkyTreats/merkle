//! Shared context types used across query, mutation, generation, and queue.
//! Aligned with api ContextView, TombstoneResult, RestoreResult, CompactResult.

use serde::{Deserialize, Serialize};

/// Result of a tombstone operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneResult {
    pub nodes_tombstoned: u64,
    pub head_entries_tombstoned: u64,
}

/// Result of a restore operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub nodes_restored: u64,
    pub head_entries_restored: u64,
}

/// Result of a compact operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactResult {
    pub nodes_purged: u64,
    pub head_entries_purged: u64,
    pub frames_purged: u64,
}
