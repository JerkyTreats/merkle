//! Context domain: frame model, query, mutation, generation, and queue.
//! Owns context behavior; CLI, agent adapter, and workspace watch consume via explicit contracts.

pub mod facade;
pub mod frame;
pub mod generation;
pub mod query;
pub mod queue;
pub mod types;

pub use facade::ContextFacade;
pub use frame::{Basis, Frame, FrameMerkleSet, FrameStorage};
pub use generation::{
    FailurePolicy, GenerationExecutor, GenerationItem, GenerationNodeType, GenerationPlan,
    GenerationResult, PlanPriority, QueueSubmitter,
};
pub use queue::{
    FrameGenerationQueue, GenerationConfig, GenerationRequest, GenerationRequestOptions,
    Priority, QueueEventContext, QueueStats,
};
pub use types::{CompactResult, RestoreResult, TombstoneResult};
