//! Merkle: Deterministic Filesystem State Management
//!
//! A Merkle-based filesystem state management system that provides deterministic,
//! hash-based tracking of filesystem state and associated context.

pub mod agent;
pub mod api;
pub mod composition;
pub mod concurrency;
pub mod error;
pub mod frame;
pub mod heads;
pub mod provider;
pub mod regeneration;
pub mod store;
pub mod synthesis;
pub mod tree;
pub mod types;
pub mod views;
