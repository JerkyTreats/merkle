//! Context generation: plan and executor for running generation plans against the queue.
//! Behavior-named; executor runs the plan; queue and provider stay in their domains.

pub mod executor;
pub mod plan;

pub use executor::{GenerationExecutor, QueueSubmitter};
pub use plan::{
    FailurePolicy, GenerationErrorDetail, GenerationItem, GenerationNodeType, GenerationPlan,
    GenerationResult, LevelSummary, PlanPriority,
};
