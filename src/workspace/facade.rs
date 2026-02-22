//! Re-exports for consumers that depend on `crate::workspace` only.

pub use super::commands::{
    resolve_node_id_by_canonical_fallback, resolve_workspace_node_id,
    WorkspaceCommandService,
};
pub use super::format::{
    format_agent_status_text, format_provider_status_text, format_section_heading,
    format_unified_status_text, format_workspace_status_text,
};
pub use super::section::build_workspace_status;
pub use super::types::{
    AgentStatusEntry, AgentStatusOutput, ContextCoverageEntry, IgnoreResult,
    ListDeletedResult, ListDeletedRow, PathCount, ProviderStatusEntry,
    ProviderStatusOutput, TreeStatus, UnifiedStatusOutput, ValidateResult,
    WorkspaceStatus, WorkspaceStatusRequest, WorkspaceStatusResult,
};
pub use super::ci::{
    BatchOperation, BatchReport, CiIntegration, DiffReport, ValidationReport, WorkspaceReport,
};
pub use super::watch::{ChangeEvent, EditorHooks, WatchConfig, WatchDaemon};
