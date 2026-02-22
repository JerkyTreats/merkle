//! Context facade: re-exports and optional single entrypoint for building context services.
//! Query, mutation, generation, and queue are available via this module and submodules.

pub use crate::context::frame::{Basis, Frame, FrameMerkleSet, FrameStorage};
pub use crate::context::generation::{
    FailurePolicy, GenerationExecutor, GenerationItem, GenerationNodeType, GenerationPlan,
    GenerationResult, PlanPriority, QueueSubmitter,
};
pub use crate::context::generation::run::{run_generate, GenerateRequest};
pub use crate::context::queue::{
    FrameGenerationQueue, GenerationConfig, GenerationRequestOptions, Priority, QueueEventContext,
    QueueStats,
};
pub use crate::context::query::{ContextView, ContextViewBuilder, NodeContext};
pub use crate::context::types::{CompactResult, RestoreResult, TombstoneResult};

/// Placeholder for a single entrypoint that builds a context service from dependencies.
/// Will be expanded when query and mutation are extracted and dependency injection is wired.
pub struct ContextFacade;
