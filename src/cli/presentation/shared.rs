//! Shared presentation: workspace result formatters (validate, ignore, list_deleted).

use crate::error::ApiError;
use crate::workspace::{IgnoreResult, ListDeletedResult, ValidateResult};

pub fn format_validate_result_text(result: &ValidateResult) -> String {
    if result.errors.is_empty() && result.warnings.is_empty() {
        format!(
            "Validation passed:\n  Root hash: {}\n  Nodes: {}\n  Frames: {}\n  All checks passed",
            result.root_hash, result.node_count, result.frame_count
        )
    } else {
        let mut s = format!(
            "Validation completed with issues:\n  Root hash: {}\n  Nodes: {}\n  Frames: {}",
            result.root_hash, result.node_count, result.frame_count
        );
        if !result.errors.is_empty() {
            s.push_str(&format!("\n\nErrors ({}):", result.errors.len()));
            for e in &result.errors {
                s.push_str(&format!("\n  - {}", e));
            }
        }
        if !result.warnings.is_empty() {
            s.push_str(&format!("\n\nWarnings ({}):", result.warnings.len()));
            for w in &result.warnings {
                s.push_str(&format!("\n  - {}", w));
            }
        }
        s
    }
}

pub fn format_ignore_result(result: &IgnoreResult, format: &str) -> Result<String, ApiError> {
    match (result, format) {
        (IgnoreResult::List { entries }, "json") => {
            let out = serde_json::json!({ "ignored": entries });
            serde_json::to_string_pretty(&out).map_err(|e| {
                ApiError::StorageError(crate::error::StorageError::InvalidPath(e.to_string()))
            })
        }
        (IgnoreResult::List { entries }, _) => {
            if entries.is_empty() {
                Ok("Ignore list is empty.".to_string())
            } else {
                let mut lines: Vec<String> = entries
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("  {}. {}", i + 1, p))
                    .collect();
                lines.insert(0, "Ignore list:".to_string());
                Ok(lines.join("\n"))
            }
        }
        (IgnoreResult::Added { path }, _) => Ok(path.clone()),
    }
}

pub fn format_list_deleted_result(
    result: &ListDeletedResult,
    format: &str,
) -> Result<String, ApiError> {
    if format == "json" {
        let arr: Vec<serde_json::Value> = result
            .rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "path": r.path,
                    "node_id": r.node_id_short,
                    "tombstoned_at": r.tombstoned_at,
                    "age": r.age
                })
            })
            .collect();
        return serde_json::to_string_pretty(&arr).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::InvalidPath(e.to_string()))
        });
    }
    use comfy_table::Table;
    let mut table = Table::new();
    table.load_preset(comfy_table::presets::UTF8_FULL);
    table.set_header(vec!["Path", "Node ID", "Tombstoned At", "Age"]);
    for r in &result.rows {
        let ts_str = if r.tombstoned_at > 0 {
            format!("{}", r.tombstoned_at)
        } else {
            "-".to_string()
        };
        table.add_row(vec![&r.path, &r.node_id_short, &ts_str, &r.age]);
    }
    Ok(table.to_string())
}
