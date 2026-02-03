//! Workspace status data and population logic.
//!
//! Produces the workspace section for `merkle workspace status`: tree state,
//! optional breakdown, context coverage per Writer/Synthesis agent, and top
//! paths by node count. Also provides agent status and provider status
//! formatting for `merkle agent status` and `merkle provider status`. Used by
//! the CLI and later by unified `merkle status`.

use crate::agent::{AgentRegistry, AgentRole};
use crate::error::ApiError;
use crate::heads::HeadIndex;
use crate::store::NodeRecordStore;
use crate::tree::builder::TreeBuilder;
use crate::types::NodeID;
use comfy_table::presets::UTF8_BORDERS_ONLY;
use comfy_table::Table;
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Workspace status: not-scanned (minimal) or scanned (full tree, coverage, top paths).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStatus {
    pub scanned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_coverage: Option<Vec<ContextCoverageEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_paths_by_node_count: Option<Vec<PathCount>>,
}

/// Tree section when scanned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeStatus {
    pub root_hash: String,
    pub total_nodes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breakdown: Option<Vec<PathCount>>,
}

/// Path prefix and node count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathCount {
    pub path: String,
    pub nodes: u64,
}

/// Per-agent context coverage when scanned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCoverageEntry {
    pub agent_id: String,
    pub nodes_with_frame: u64,
    pub nodes_without_frame: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_pct: Option<u64>,
}

/// Build workspace status from store, head index, agent registry, and workspace root.
///
/// When `include_breakdown` is true, the tree section includes top-level path breakdown.
pub fn build_workspace_status(
    node_store: &dyn NodeRecordStore,
    head_index: &HeadIndex,
    agent_registry: &AgentRegistry,
    workspace_root: &Path,
    include_breakdown: bool,
) -> Result<WorkspaceStatus, ApiError> {
    let root_hash: NodeID = TreeBuilder::new(workspace_root.to_path_buf())
        .compute_root()
        .map_err(ApiError::from)?;

    let root_in_store = node_store.get(&root_hash).map_err(ApiError::from)?.is_some();
    if !root_in_store {
        return Ok(WorkspaceStatus {
            scanned: false,
            message: Some("Run merkle scan to build the tree.".to_string()),
            tree: None,
            context_coverage: None,
            top_paths_by_node_count: None,
        });
    }

    let records = node_store.list_all().map_err(ApiError::from)?;
    let total_nodes = records.len() as u64;
    let root_hash_hex = hex::encode(root_hash);

    // Group by first path component (relative to workspace root) for breakdown and top paths
    let workspace_root_buf = workspace_root.to_path_buf();
    let mut prefix_counts: HashMap<String, u64> = HashMap::new();
    for record in &records {
        let rel = record
            .path
            .strip_prefix(&workspace_root_buf)
            .unwrap_or(record.path.as_path());
        let first = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let key = if first.is_empty() { ".".to_string() } else { first };
        *prefix_counts.entry(key).or_insert(0) += 1;
    }

    // Top paths: root "." first with total_nodes, then next four heaviest path prefixes (with trailing /)
    let mut top_paths: Vec<PathCount> = vec![PathCount {
        path: ".".to_string(),
        nodes: total_nodes,
    }];
    let mut rest: Vec<(String, u64)> = prefix_counts
        .iter()
        .filter(|(k, _)| *k != ".")
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    rest.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, nodes) in rest.into_iter().take(4) {
        top_paths.push(PathCount {
            path: path + "/",
            nodes,
        });
    }

    // Breakdown: top-level path -> count (when include_breakdown); paths like "src/", "config/"
    let breakdown = if include_breakdown {
        let mut by_count: Vec<(String, u64)> = prefix_counts
            .iter()
            .map(|(k, v)| {
                let path = if *k == "." {
                    ".".to_string()
                } else {
                    k.clone() + "/"
                };
                (path, *v)
            })
            .collect();
        by_count.sort_by(|a, b| b.1.cmp(&a.1));
        Some(
            by_count
                .into_iter()
                .map(|(path, nodes)| PathCount { path, nodes })
                .collect(),
        )
    } else {
        None
    };

    // Context coverage: Writer and Synthesis agents; frame type = context-<agent_id>
    let writers = agent_registry.list_by_role(Some(AgentRole::Writer));
    let synthesis = agent_registry.list_by_role(Some(AgentRole::Synthesis));
    let mut agent_ids: std::collections::HashSet<String> = writers
        .iter()
        .chain(synthesis.iter())
        .map(|a| a.agent_id.clone())
        .collect();
    let mut context_coverage: Vec<ContextCoverageEntry> = Vec::new();
    for agent_id in agent_ids.drain() {
        let frame_type = format!("context-{}", agent_id);
        let nodes_with_frame = head_index.count_nodes_for_frame_type(&frame_type) as u64;
        let nodes_without_frame = total_nodes.saturating_sub(nodes_with_frame);
        let coverage_pct = if total_nodes > 0 {
            Some((nodes_with_frame * 100) / total_nodes)
        } else {
            Some(0)
        };
        context_coverage.push(ContextCoverageEntry {
            agent_id,
            nodes_with_frame,
            nodes_without_frame,
            coverage_pct,
        });
    }
    context_coverage.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));

    Ok(WorkspaceStatus {
        scanned: true,
        message: None,
        tree: Some(TreeStatus {
            root_hash: root_hash_hex,
            total_nodes,
            breakdown,
        }),
        context_coverage: Some(context_coverage),
        top_paths_by_node_count: Some(top_paths),
    })
}

/// Format a section heading with bold/underline. Respects NO_COLOR and TTY.
pub fn format_section_heading(title: &str) -> String {
    format!("{}", title.bold().underline())
}

/// Format workspace status as human-readable text using comfy-table and styled headings.
pub fn format_workspace_status_text(data: &WorkspaceStatus, include_breakdown: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n\n", format_section_heading("Workspace Status")));
    out.push_str(&format!("{}\n", format_section_heading("Tree")));
    if !data.scanned {
        out.push_str("  Scanned: no\n\n");
        if let Some(ref msg) = data.message {
            out.push_str(msg);
            out.push_str("\n");
        }
        return out;
    }
    let tree = data.tree.as_ref().unwrap();
    out.push_str(&format!("  Root hash: {}...\n", &tree.root_hash[..tree.root_hash.len().min(7)]));
    out.push_str(&format!("  Total nodes: {}\n", tree.total_nodes));
    out.push_str("  Scanned: yes\n\n");
    if include_breakdown {
        if let Some(ref breakdown) = tree.breakdown {
            out.push_str("  Top-level breakdown\n\n");
            let mut table = Table::new();
            table.load_preset(UTF8_BORDERS_ONLY);
            table.set_header(vec!["Path", "Nodes"]);
            for row in breakdown {
                table.add_row(vec![row.path.clone(), row.nodes.to_string()]);
            }
            out.push_str(&format!("{}\n\n", table));
        }
    }
    if let Some(ref coverage) = data.context_coverage {
        out.push_str(&format!("{}\n\n", format_section_heading("Context coverage")));
        let mut table = Table::new();
        table.load_preset(UTF8_BORDERS_ONLY);
        table.set_header(vec!["Agent", "With frame", "Without", "Coverage"]);
        for row in coverage {
            let pct = row
                .coverage_pct
                .map(|p| format!("{}%", p))
                .unwrap_or_else(|| "-".to_string());
            table.add_row(vec![
                row.agent_id.clone(),
                row.nodes_with_frame.to_string(),
                row.nodes_without_frame.to_string(),
                pct,
            ]);
        }
        out.push_str(&format!("{}\n\n", table));
    }
    if let Some(ref top_paths) = data.top_paths_by_node_count {
        out.push_str(&format!(
            "{}\n\n",
            format_section_heading("Top paths by node count")
        ));
        let mut table = Table::new();
        table.load_preset(UTF8_BORDERS_ONLY);
        table.set_header(vec!["Path", "Nodes"]);
        for row in top_paths {
            table.add_row(vec![row.path.clone(), row.nodes.to_string()]);
        }
        out.push_str(&format!("{}\n", table));
    }
    out
}

// --- Agent status (merkle agent status) ---

/// One row for agent status table / JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusEntry {
    pub agent_id: String,
    pub role: String,
    pub valid: bool,
    pub prompt_path_exists: bool,
}

/// Agent status output for JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusOutput {
    pub agents: Vec<AgentStatusEntry>,
    pub total: usize,
    pub valid_count: usize,
}

/// Format agent status as human-readable text (comfy-table + section heading).
pub fn format_agent_status_text(entries: &[AgentStatusEntry]) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n\n", format_section_heading("Agents")));
    if entries.is_empty() {
        out.push_str("No agents configured.\n");
        return out;
    }
    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(vec!["Agent", "Role", "Valid", "Prompt"]);
    for row in entries {
        let valid_str = if row.valid { "yes" } else { "no" };
        let prompt_str = if row.role == "Reader" {
            "n/a".to_string()
        } else if row.prompt_path_exists {
            "exists".to_string()
        } else {
            "missing".to_string()
        };
        table.add_row(vec![
            row.agent_id.clone(),
            row.role.clone(),
            valid_str.to_string(),
            prompt_str,
        ]);
    }
    out.push_str(&format!("{}\n\n", table));
    let valid_count = entries.iter().filter(|e| e.valid).count();
    out.push_str(&format!("Total: {} agents, {} valid.\n", entries.len(), valid_count));
    out
}

// --- Provider status (merkle provider status) ---

/// One row for provider status table / JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatusEntry {
    pub provider_name: String,
    pub provider_type: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connectivity: Option<String>,
}

/// Provider status output for JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatusOutput {
    pub providers: Vec<ProviderStatusEntry>,
    pub total: usize,
}

/// Format provider status as human-readable text (comfy-table + section heading).
pub fn format_provider_status_text(entries: &[ProviderStatusEntry], include_connectivity: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n\n", format_section_heading("Providers")));
    if entries.is_empty() {
        out.push_str("No providers configured.\n");
        return out;
    }
    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    if include_connectivity {
        table.set_header(vec!["Provider", "Type", "Model", "Connectivity"]);
        for row in entries {
            let conn = row
                .connectivity
                .as_deref()
                .map(|c| match c {
                    "ok" => "OK",
                    "fail" => "Fail",
                    "skipped" => "Skipped",
                    _ => c,
                })
                .unwrap_or("-");
            table.add_row(vec![
                row.provider_name.clone(),
                row.provider_type.clone(),
                row.model.clone(),
                conn.to_owned(),
            ]);
        }
    } else {
        table.set_header(vec!["Provider", "Type", "Model"]);
        for row in entries {
            table.add_row(vec![
                row.provider_name.clone(),
                row.provider_type.clone(),
                row.model.clone(),
            ]);
        }
    }
    out.push_str(&format!("{}\n\n", table));
    out.push_str(&format!("Total: {} providers.\n", entries.len()));
    out
}
