//! Filesystem Merkle Tree
//!
//! Represents the entire workspace as a Merkle tree, where each node
//! (file or directory) has a deterministic hash based on content and structure.

pub mod builder;
pub mod hasher;
pub mod node;
pub mod path;
pub mod walker;
