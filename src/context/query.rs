//! Context query: view policy, composition, and query service.
//! Single owner of context read behavior; api delegates to this module.

pub mod composition;
pub mod get;
pub mod service;
pub mod view_policy;
pub mod view;

pub use composition::{compose_frames, CompositionPolicy, CompositionSource};
pub use get::get_node_for_cli;
pub use service::get_node as get_node_query;
pub use view_policy::{FrameFilter, OrderingPolicy, ViewPolicy, get_context_view};
pub use view::{ContextView, ContextViewBuilder, NodeContext};
