//! Maps command descriptors to typed summary events and command_summary payload.
//! Telemetry owns summary mapping policy; CLI maps Commands to SummaryCommandDescriptor.

use serde_json::{json, Value};

use crate::telemetry::events::SummaryEventData;

/// Descriptor for summary emission. CLI maps Commands to this; telemetry never imports Commands.
#[derive(Debug, Clone)]
pub enum SummaryCommandDescriptor {
    WorkspaceStatus {
        format: String,
        breakdown: bool,
    },
    WorkspaceValidate {
        format: String,
    },
    WorkspaceDelete {
        target_path: bool,
        target_node: bool,
        dry_run: bool,
        no_ignore: bool,
    },
    WorkspaceRestore {
        target_path: bool,
        target_node: bool,
        dry_run: bool,
    },
    WorkspaceCompact {
        ttl_days: Option<u64>,
        all: bool,
        keep_frames: bool,
        dry_run: bool,
    },
    WorkspaceListDeleted {
        older_than_days: Option<u64>,
        format: String,
    },
    WorkspaceIgnore {
        has_path: bool,
        dry_run: bool,
        format: String,
    },
    StatusUnified {
        format: String,
        include_workspace: bool,
        include_agents: bool,
        include_providers: bool,
        breakdown: bool,
        test_connectivity: bool,
    },
    ValidateWorkspace,
    AgentAction {
        action: String,
        mutation: bool,
    },
    ProviderAction {
        action: String,
        mutation: bool,
    },
    Init {
        force: bool,
        list_only: bool,
    },
    /// No typed summary; only command_summary is emitted.
    None,
}

pub const COMMAND_SUMMARY_MESSAGE_MAX_CHARS: usize = 256;

/// Build typed summary event if the descriptor has one. Preserves event type names and payload fields.
pub fn typed_summary_event(
    descriptor: &SummaryCommandDescriptor,
    ok: bool,
    duration_ms: u128,
    error: Option<&str>,
) -> Option<(&'static str, Value)> {
    let err = error;
    match descriptor {
        SummaryCommandDescriptor::WorkspaceStatus { format, breakdown } => Some((
            "status_summary",
            json!({
                "scope": "workspace",
                "format": format,
                "breakdown": breakdown,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::WorkspaceValidate { format } => Some((
            "validate_summary",
            json!({
                "scope": "workspace",
                "format": format,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::WorkspaceDelete {
            target_path,
            target_node,
            dry_run,
            no_ignore,
        } => Some((
            "workspace_mutation_summary",
            json!({
                "operation": "delete",
                "target": if *target_path { "path" } else if *target_node { "node" } else { "unknown" },
                "dry_run": dry_run,
                "no_ignore": no_ignore,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::WorkspaceRestore {
            target_path,
            target_node,
            dry_run,
        } => Some((
            "workspace_mutation_summary",
            json!({
                "operation": "restore",
                "target": if *target_path { "path" } else if *target_node { "node" } else { "unknown" },
                "dry_run": dry_run,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::WorkspaceCompact {
            ttl_days,
            all,
            keep_frames,
            dry_run,
        } => Some((
            "workspace_maintenance_summary",
            json!({
                "operation": "compact",
                "ttl_days": ttl_days,
                "all": all,
                "keep_frames": keep_frames,
                "dry_run": dry_run,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::WorkspaceListDeleted {
            older_than_days,
            format,
        } => Some((
            "list_summary",
            json!({
                "scope": "workspace_deleted",
                "older_than_days": older_than_days,
                "format": format,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::WorkspaceIgnore {
            has_path,
            dry_run,
            format,
        } => Some((
            "config_mutation_summary",
            json!({
                "scope": "workspace_ignore",
                "action": if *has_path { "add" } else { "list" },
                "dry_run": dry_run,
                "format": format,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::StatusUnified {
            format,
            include_workspace,
            include_agents,
            include_providers,
            breakdown,
            test_connectivity,
        } => Some((
            "status_summary",
            json!({
                "scope": "unified",
                "format": format,
                "include_workspace": include_workspace,
                "include_agents": include_agents,
                "include_providers": include_providers,
                "breakdown": breakdown,
                "test_connectivity": test_connectivity,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::ValidateWorkspace => Some((
            "validate_summary",
            json!({
                "scope": "workspace",
                "format": "text",
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::AgentAction { action, mutation } => Some((
            "config_mutation_summary",
            json!({
                "scope": "agent",
                "action": action,
                "mutation": mutation,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::ProviderAction { action, mutation } => Some((
            "config_mutation_summary",
            json!({
                "scope": "provider",
                "action": action,
                "mutation": mutation,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::Init { force, list_only } => Some((
            "init_summary",
            json!({
                "force": force,
                "list_only": list_only,
                "ok": ok,
                "duration_ms": duration_ms,
                "error": err,
            }),
        )),
        SummaryCommandDescriptor::None => None,
    }
}

pub fn truncate_summary_message(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        return (value.to_string(), false);
    }
    (value.chars().take(max_chars).collect(), true)
}

/// Build command_summary payload. Caller provides command name and result-derived fields.
pub fn command_summary_data(
    command_name: &str,
    ok: bool,
    duration_ms: u128,
    message: Option<String>,
    output_chars: Option<usize>,
    error_chars: Option<usize>,
    truncated: Option<bool>,
) -> SummaryEventData {
    SummaryEventData {
        command: command_name.to_string(),
        ok,
        duration_ms,
        message,
        output_chars,
        error_chars,
        truncated,
    }
}
