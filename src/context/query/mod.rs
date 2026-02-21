//! Context query: view policy, composition, and query service.
//! Single owner of context read behavior; api delegates to this module.

pub mod service;
pub mod view_policy;

pub use service::get_node as get_node_query;
pub use view_policy::{FrameFilter, OrderingPolicy, ViewPolicy, get_context_view};
