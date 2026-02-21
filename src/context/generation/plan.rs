use crate::context::queue::Priority;
use crate::error::ApiError;
use crate::types::{FrameID, NodeID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GenerationNodeType {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl From<PlanPriority> for Priority {
    fn from(value: PlanPriority) -> Self {
        match value {
            PlanPriority::Low => Priority::Low,
            PlanPriority::Normal => Priority::Normal,
            PlanPriority::High => Priority::High,
            PlanPriority::Urgent => Priority::Urgent,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailurePolicy {
    StopOnLevelFailure,
    Continue,
    FailImmediately,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationItem {
    pub node_id: NodeID,
    pub path: String,
    pub node_type: GenerationNodeType,
    pub agent_id: String,
    pub provider_name: String,
    pub frame_type: String,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationPlan {
    pub plan_id: String,
    pub source: String,
    pub session_id: Option<String>,
    pub levels: Vec<Vec<GenerationItem>>,
    pub priority: PlanPriority,
    pub failure_policy: FailurePolicy,
    pub target_path: String,
    pub total_nodes: usize,
    pub total_levels: usize,
}

impl GenerationPlan {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.plan_id.trim().is_empty() {
            return Err(ApiError::ConfigError(
                "Generation plan id cannot be empty".to_string(),
            ));
        }
        if self.levels.is_empty() {
            return Err(ApiError::ConfigError(
                "Generation plan must contain at least one level".to_string(),
            ));
        }
        if self.levels.iter().any(|level| level.is_empty()) {
            return Err(ApiError::ConfigError(
                "Generation plan contains an empty level".to_string(),
            ));
        }
        let computed_nodes: usize = self.levels.iter().map(std::vec::Vec::len).sum();
        if self.total_nodes != computed_nodes {
            return Err(ApiError::ConfigError(format!(
                "Generation plan total_nodes mismatch: expected {}, got {}",
                computed_nodes, self.total_nodes
            )));
        }
        if self.total_levels != self.levels.len() {
            return Err(ApiError::ConfigError(format!(
                "Generation plan total_levels mismatch: expected {}, got {}",
                self.levels.len(),
                self.total_levels
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationErrorDetail {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSummary {
    pub level_index: usize,
    pub generated_count: usize,
    pub failed_count: usize,
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    pub plan_id: String,
    pub successes: HashMap<NodeID, FrameID>,
    pub failures: HashMap<NodeID, GenerationErrorDetail>,
    pub level_summaries: Vec<LevelSummary>,
    pub total_generated: usize,
    pub total_failed: usize,
}

impl GenerationResult {
    pub fn new(plan_id: String) -> Self {
        Self {
            plan_id,
            successes: HashMap::new(),
            failures: HashMap::new(),
            level_summaries: Vec::new(),
            total_generated: 0,
            total_failed: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Hash;

    fn test_item() -> GenerationItem {
        GenerationItem {
            node_id: Hash::from([1u8; 32]),
            path: "/tmp/a.txt".to_string(),
            node_type: GenerationNodeType::File,
            agent_id: "writer".to_string(),
            provider_name: "provider".to_string(),
            frame_type: "context-writer".to_string(),
            force: false,
        }
    }

    fn test_plan() -> GenerationPlan {
        GenerationPlan {
            plan_id: "plan-1".to_string(),
            source: "test".to_string(),
            session_id: Some("s1".to_string()),
            levels: vec![vec![test_item()]],
            priority: PlanPriority::Urgent,
            failure_policy: FailurePolicy::StopOnLevelFailure,
            target_path: "/tmp".to_string(),
            total_nodes: 1,
            total_levels: 1,
        }
    }

    #[test]
    fn validate_rejects_empty_levels() {
        let mut plan = test_plan();
        plan.levels = Vec::new();
        assert!(plan.validate().is_err());
    }

    #[test]
    fn validate_rejects_incorrect_totals() {
        let mut plan = test_plan();
        plan.total_nodes = 3;
        assert!(plan.validate().is_err());
    }

    #[test]
    fn serde_round_trip_plan() {
        let plan = test_plan();
        let encoded = serde_json::to_string(&plan).unwrap();
        let decoded: GenerationPlan = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.plan_id, plan.plan_id);
        assert_eq!(decoded.total_nodes, plan.total_nodes);
        assert_eq!(decoded.total_levels, plan.total_levels);
    }
}
