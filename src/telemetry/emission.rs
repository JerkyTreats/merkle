//! Emission orchestration and summary mapping.

pub mod engine;
pub mod summary_mapper;

pub use engine::{emit_command_summary, truncate_for_summary};
pub use summary_mapper::SummaryCommandDescriptor;
