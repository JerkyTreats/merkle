//! Context get entry point for CLI: resolve node, build view, return NodeContext.

use crate::api::{ContextApi, ContextView, NodeContext};
use crate::error::ApiError;
use crate::types::NodeID;
use crate::views::OrderingPolicy;
use crate::workspace;
use std::path::Path;

fn parse_node_id(s: &str) -> Result<NodeID, ApiError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s)
        .map_err(|e| ApiError::InvalidFrame(format!("Invalid hex string: {}", e)))?;
    if bytes.len() != 32 {
        return Err(ApiError::InvalidFrame(format!(
            "NodeID must be 32 bytes, got {} bytes",
            bytes.len()
        )));
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(crate::types::Hash::from(hash))
}

/// Single get entry point: resolve node_id, build ContextView, call api.get_node.
pub fn get_node_for_cli(
    api: &ContextApi,
    workspace_root: &std::path::PathBuf,
    node: Option<&str>,
    path: Option<&Path>,
    agent: Option<&str>,
    frame_type: Option<&str>,
    max_frames: usize,
    ordering: &str,
    _include_deleted: bool,
) -> Result<NodeContext, ApiError> {
    let node_id = match (node, path) {
        (Some(node_str), None) => parse_node_id(node_str)?,
        (None, Some(p)) => workspace::resolve_workspace_node_id(
            api,
            workspace_root,
            Some(p),
            None,
            false,
        )?,
        (Some(_), Some(_)) => {
            return Err(ApiError::ConfigError(
                "Cannot specify both --node and --path. Use one or the other.".to_string(),
            ));
        }
        (None, None) => {
            return Err(ApiError::ConfigError(
                "Must specify either --node <node_id> or --path <path>.".to_string(),
            ));
        }
    };

    let ordering_policy = match ordering {
        "recency" => OrderingPolicy::Recency,
        "deterministic" => OrderingPolicy::Type,
        _ => {
            return Err(ApiError::ConfigError(format!(
                "Invalid ordering: '{}'. Must be 'recency' or 'deterministic'.",
                ordering
            )));
        }
    };

    let mut builder = ContextView::builder().max_frames(max_frames);
    match ordering_policy {
        OrderingPolicy::Recency => builder = builder.recent(),
        OrderingPolicy::Type => builder = builder.by_type_ordering(),
        _ => builder = builder.recent(),
    }
    if let Some(agent_id) = agent {
        builder = builder.by_agent(agent_id);
    }
    if let Some(ft) = frame_type {
        builder = builder.by_type(ft);
    }
    let view = builder.build();
    api.get_node(node_id, view)
}
