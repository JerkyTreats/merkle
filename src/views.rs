//! Context Views
//!
//! Re-exports view policy from context domain for compatibility.
//! Selects and orders a bounded set of frames based on policies.

pub use crate::context::query::view_policy::{
    get_context_view, FrameFilter, OrderingPolicy, ViewPolicy,
};
