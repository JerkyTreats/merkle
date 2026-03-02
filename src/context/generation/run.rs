//! Single generate entry point: resolve, plan, queue, execute.
//! CLI and other callers use this only; no plan/queue/executor orchestration in adapters.

use crate::api::ContextApi;
use crate::agent::profile::prompt_contract::PromptContract;
use crate::context::generation::plan::{
    FailurePolicy, GenerationItem, GenerationNodeType, GenerationPlan, PlanPriority,
};
use crate::context::generation::GenerationExecutor;
use crate::context::queue::{FrameGenerationQueue, GenerationConfig, QueueEventContext};
use crate::error::ApiError;
use crate::store::NodeType;
use crate::telemetry::{now_millis, ProgressRuntime};
use crate::types::NodeID;
use crate::workspace;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn format_failure_samples(
    result: &crate::context::generation::plan::GenerationResult,
    max_samples: usize,
) -> String {
    let mut messages: Vec<&str> = result
        .failures
        .values()
        .map(|detail| detail.message.as_str())
        .collect();
    messages.sort_unstable();
    messages.dedup();

    let samples: Vec<&str> = messages.into_iter().take(max_samples).collect();
    if samples.is_empty() {
        return String::new();
    }

    let mut out = format!(" Sample errors: {}", samples.join(" | "));
    let remaining = result.failures.len().saturating_sub(samples.len());
    if remaining > 0 {
        out.push_str(&format!(" | ... and {} more", remaining));
    }
    out
}

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

fn resolve_agent_id(api: &ContextApi, agent_id: Option<&str>) -> Result<String, ApiError> {
    if let Some(agent_id) = agent_id {
        api.get_agent(agent_id)?;
        return Ok(agent_id.to_string());
    }
    let (agent_count, agent_ids) = {
        let registry = api.agent_registry().read();
        let writer_agents =
            registry.list_by_role(Some(crate::agent::AgentRole::Writer));
        let agent_ids: Vec<String> = writer_agents.iter().map(|a| a.agent_id.clone()).collect();
        (agent_ids.len(), agent_ids)
    };
    match agent_count {
        0 => Err(ApiError::ConfigError(
            "No Writer agents found. Use `meld agent list` to see available agents, or use `--agent <agent_id>` to specify an agent.".to_string()
        )),
        1 => Ok(agent_ids[0].clone()),
        _ => Err(ApiError::ConfigError(format!(
            "Multiple Writer agents found: {}. Use `--agent <agent_id>` to specify which agent to use.",
            agent_ids.join(", ")
        ))),
    }
}

fn resolve_provider_name(api: &ContextApi, provider_name: Option<&str>) -> Result<String, ApiError> {
    let provider_name = provider_name.ok_or_else(|| {
        ApiError::ProviderNotConfigured(
            "Provider is required. Use `--provider <provider_name>` to specify a provider. Use `meld provider list` to see available providers.".to_string()
        )
    })?;
    let registry = api.provider_registry().read();
    registry.get_or_error(provider_name)?;
    drop(registry);
    Ok(provider_name.to_string())
}

fn find_missing_descendant_heads(
    api: &ContextApi,
    target_node_id: NodeID,
    frame_type: &str,
) -> Result<Vec<String>, ApiError> {
    let mut missing = Vec::new();
    let mut visited: HashSet<NodeID> = HashSet::new();
    let mut queue = VecDeque::new();
    let target_record = api
        .node_store()
        .get(&target_node_id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NodeNotFound(target_node_id))?;
    for child in &target_record.children {
        queue.push_back(*child);
    }
    while let Some(node_id) = queue.pop_front() {
        if !visited.insert(node_id) {
            continue;
        }
        let record = api
            .node_store()
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
        if api.get_head(&node_id, frame_type)?.is_none() {
            missing.push(record.path.to_string_lossy().to_string());
        }
        for child in &record.children {
            queue.push_back(*child);
        }
    }
    Ok(missing)
}

fn collect_subtree_levels(api: &ContextApi, target_node_id: NodeID) -> Result<Vec<Vec<NodeID>>, ApiError> {
    let mut levels: HashMap<usize, Vec<NodeID>> = HashMap::new();
    let mut visited: HashSet<NodeID> = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((target_node_id, 0usize));
    while let Some((node_id, depth)) = queue.pop_front() {
        if !visited.insert(node_id) {
            continue;
        }
        levels.entry(depth).or_default().push(node_id);
        let record = api
            .node_store()
            .get(&node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
        for child in &record.children {
            queue.push_back((*child, depth + 1));
        }
    }
    let mut ordered_depths: Vec<_> = levels.into_iter().collect();
    ordered_depths.sort_by(|(a, _), (b, _)| b.cmp(a));
    Ok(ordered_depths.into_iter().map(|(_, nodes)| nodes).collect())
}

#[allow(clippy::too_many_arguments)]
fn build_plan(
    api: &ContextApi,
    progress: Option<&Arc<ProgressRuntime>>,
    session_id: Option<&str>,
    target_node_id: NodeID,
    target_path: &Path,
    is_directory_target: bool,
    recursive: bool,
    force: bool,
    agent_id: &str,
    provider_name: &str,
    frame_type: &str,
) -> Result<GenerationPlan, ApiError> {
    if !recursive && is_directory_target && !force {
        if let (Some(prog), Some(sid)) = (progress, session_id) {
            prog.emit_event_best_effort(
                sid,
                "descendant_check_started",
                json!({
                    "node_id": hex::encode(target_node_id),
                    "path": target_path.to_string_lossy(),
                    "frame_type": frame_type,
                }),
            );
        }
        let missing = find_missing_descendant_heads(api, target_node_id, frame_type)?;
        if !missing.is_empty() {
            if let (Some(prog), Some(sid)) = (progress, session_id) {
                prog.emit_event_best_effort(
                    sid,
                    "descendant_check_failed",
                    json!({
                        "node_id": hex::encode(target_node_id),
                        "missing_count": missing.len(),
                        "missing_paths": missing,
                    }),
                );
            }
            return Err(ApiError::GenerationFailed(
                "Directory descendants are missing required heads; run recursive generation or use --force.".to_string(),
            ));
        }
        if let (Some(prog), Some(sid)) = (progress, session_id) {
            prog.emit_event_best_effort(
                sid,
                "descendant_check_passed",
                json!({
                    "node_id": hex::encode(target_node_id),
                    "path": target_path.to_string_lossy(),
                    "frame_type": frame_type,
                }),
            );
        }
    }

    let mut levels: Vec<Vec<GenerationItem>> = Vec::new();
    if recursive {
        let depth_levels = collect_subtree_levels(api, target_node_id)?;
        for level in depth_levels {
            let mut items = Vec::new();
            for node_id in level {
                let record = api
                    .node_store()
                    .get(&node_id)
                    .map_err(ApiError::from)?
                    .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
                if !force && api.get_head(&node_id, frame_type)?.is_some() {
                    if let (Some(prog), Some(sid)) = (progress, session_id) {
                        prog.emit_event_best_effort(
                            sid,
                            "node_skipped",
                            json!({
                                "node_id": hex::encode(node_id),
                                "path": record.path.to_string_lossy(),
                                "agent_id": agent_id,
                                "provider_name": provider_name,
                                "frame_type": frame_type,
                                "reason": "head_reuse",
                            }),
                        );
                    }
                    continue;
                }
                items.push(GenerationItem {
                    node_id,
                    path: record.path.to_string_lossy().to_string(),
                    node_type: match record.node_type {
                        NodeType::File { .. } => GenerationNodeType::File,
                        NodeType::Directory => GenerationNodeType::Directory,
                    },
                    agent_id: agent_id.to_string(),
                    provider_name: provider_name.to_string(),
                    frame_type: frame_type.to_string(),
                    force,
                });
            }
            if !items.is_empty() {
                levels.push(items);
            }
        }
    } else {
        if !force && api.get_head(&target_node_id, frame_type)?.is_some() {
            if let (Some(prog), Some(sid)) = (progress, session_id) {
                prog.emit_event_best_effort(
                    sid,
                    "node_skipped",
                    json!({
                        "node_id": hex::encode(target_node_id),
                        "path": target_path.to_string_lossy(),
                        "agent_id": agent_id,
                        "provider_name": provider_name,
                        "frame_type": frame_type,
                        "reason": "head_reuse",
                    }),
                );
            }
            return Ok(GenerationPlan {
                plan_id: format!(
                    "plan-{}-{}",
                    now_millis(),
                    &hex::encode(target_node_id)[..8]
                ),
                source: format!("context generate {}", target_path.to_string_lossy()),
                session_id: session_id.map(String::from),
                levels: Vec::new(),
                priority: PlanPriority::Urgent,
                failure_policy: FailurePolicy::StopOnLevelFailure,
                target_path: target_path.to_string_lossy().to_string(),
                total_nodes: 0,
                total_levels: 0,
            });
        }
        let target_record = api
            .node_store()
            .get(&target_node_id)
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::NodeNotFound(target_node_id))?;
        levels.push(vec![GenerationItem {
            node_id: target_node_id,
            path: target_record.path.to_string_lossy().to_string(),
            node_type: match target_record.node_type {
                NodeType::File { .. } => GenerationNodeType::File,
                NodeType::Directory => GenerationNodeType::Directory,
            },
            agent_id: agent_id.to_string(),
            provider_name: provider_name.to_string(),
            frame_type: frame_type.to_string(),
            force,
        }]);
    }

    let total_nodes: usize = levels.iter().map(Vec::len).sum();
    Ok(GenerationPlan {
        plan_id: format!(
            "plan-{}-{}",
            now_millis(),
            &hex::encode(target_node_id)[..8]
        ),
        source: format!("context generate {}", target_path.to_string_lossy()),
        session_id: session_id.map(String::from),
        total_levels: levels.len(),
        levels,
        priority: PlanPriority::Urgent,
        failure_policy: FailurePolicy::StopOnLevelFailure,
        target_path: target_path.to_string_lossy().to_string(),
        total_nodes,
    })
}

/// Request for a single generate run.
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub node: Option<String>,
    pub path: Option<PathBuf>,
    pub agent: Option<String>,
    pub provider: Option<String>,
    pub frame_type: Option<String>,
    pub force: bool,
    pub no_recursive: bool,
}

/// Single generate entry point: resolve node/agent/provider, build plan, create queue, execute.
/// Returns human-readable summary string or error.
pub fn run_generate(
    api: Arc<ContextApi>,
    workspace_root: &PathBuf,
    progress: Option<Arc<ProgressRuntime>>,
    session_id: Option<&str>,
    request: &GenerateRequest,
) -> Result<String, ApiError> {
    let node_id = match (request.node.as_deref(), request.path.as_deref()) {
        (Some(node_str), None) => parse_node_id(node_str)?,
        (None, Some(p)) => workspace::resolve_workspace_node_id(
            api.as_ref(),
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
                "Must specify either --node <node_id>, --path <path>, or a positional path (e.g. meld context generate ./foo).".to_string()
            ));
        }
    };

    let agent_id = resolve_agent_id(api.as_ref(), request.agent.as_deref())?;
    let provider_name = resolve_provider_name(api.as_ref(), request.provider.as_deref())?;
    let frame_type = request
        .frame_type
        .clone()
        .unwrap_or_else(|| format!("context-{}", agent_id));

    let agent = api.get_agent(&agent_id)?;
    if agent.role != crate::agent::AgentRole::Writer {
        return Err(ApiError::Unauthorized(format!(
            "Agent '{}' has role {:?}, but only Writer agents can generate frames.",
            agent_id, agent.role
        )));
    }

    let node_record = api
        .node_store()
        .get(&node_id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NodeNotFound(node_id))?;
    let node_path = node_record.path.to_string_lossy().to_string();

    PromptContract::from_agent(&agent)?;

    let is_directory_target = matches!(node_record.node_type, NodeType::Directory);
    let recursive = is_directory_target && !request.no_recursive;

    let plan = build_plan(
        api.as_ref(),
        progress.as_ref(),
        session_id,
        node_id,
        &node_record.path,
        is_directory_target,
        recursive,
        request.force,
        &agent_id,
        &provider_name,
        &frame_type,
    )?;

    if let (Some(prog), Some(sid)) = (progress.as_deref(), session_id) {
        prog.emit_event_best_effort(
            sid,
            "plan_constructed",
            json!({
                "plan_id": plan.plan_id,
                "node_id": hex::encode(node_id),
                "path": node_path,
                "agent_id": agent_id,
                "provider_name": provider_name,
                "frame_type": frame_type,
                "force": request.force,
                "recursive": recursive,
                "total_nodes": plan.total_nodes,
                "total_levels": plan.total_levels
            }),
        );
    }

    if plan.total_nodes == 0 {
        return Ok(
            "Frame already exists for requested target.\nUse --force to generate a new frame."
                .to_string(),
        );
    }

    let rt = if let Ok(_handle) = tokio::runtime::Handle::try_current() {
        return Err(ApiError::ProviderError(
            "Cannot generate context from within an async runtime context. This is a limitation when running from async tests.".to_string()
        ));
    } else {
        tokio::runtime::Runtime::new()
            .map_err(|e| ApiError::ProviderError(format!("Failed to create runtime: {}", e)))?
    };

    let gen_config = GenerationConfig::default();
    let event_context = match (session_id, &progress) {
        (Some(sid), Some(prog)) => Some(QueueEventContext {
            session_id: sid.to_string(),
            progress: Arc::clone(prog),
        }),
        _ => None,
    };
    let queue = Arc::new(FrameGenerationQueue::with_event_context(
        api,
        gen_config,
        event_context,
    ));

    let _guard = rt.enter();
    queue.start()?;
    let executor = GenerationExecutor::new(progress);
    drop(_guard);
    let result = rt.block_on(async { executor.execute(queue.as_ref(), plan).await })?;

    if result.total_failed > 0 {
        let failure_samples = format_failure_samples(&result, 3);
        return Err(ApiError::GenerationFailed(format!(
            "Generation completed with failures. generated={}, failed={}.{}",
            result.total_generated, result.total_failed, failure_samples
        )));
    }
    Ok(format!(
        "Generation completed: generated={}, failed={}",
        result.total_generated, result.total_failed
    ))
}
