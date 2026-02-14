//! FrameID computation for context frames

use crate::error::StorageError;
use crate::frame::Basis;
use crate::types::{FrameID, Hash};
use blake3::Hasher;

/// Compute FrameID for a context frame
///
/// FrameID = hash(basis_hash || agent_id || content || frame_type)
///
/// The basis_hash is computed from the Basis enum, ensuring deterministic
/// FrameID generation. The agent_id is included to preserve agent attribution
/// in the FrameID (Phase 2A requirement).
pub fn compute_frame_id(
    basis: &Basis,
    content: &[u8],
    frame_type: &str,
    agent_id: &str,
) -> Result<FrameID, StorageError> {
    let basis_hash = compute_basis_hash(basis)?;

    let mut hasher = Hasher::new();

    // Hash basis
    hasher.update(&basis_hash);

    // Hash agent ID (Phase 2A: agent identity preserved in FrameID)
    hasher.update(b"agent:");
    hasher.update(agent_id.as_bytes());

    // Hash frame type
    hasher.update(b"type:");
    hasher.update(frame_type.as_bytes());

    // Hash content
    hasher.update(b"content:");
    hasher.update(content);

    Ok(*hasher.finalize().as_bytes())
}

/// Compute hash of the basis
///
/// Basis hash depends on the Basis variant:
/// - Node(NodeID): hash("node:" || NodeID)
/// - Frame(FrameID): hash("frame:" || FrameID)
/// - Both { node, frame }: hash("both:" || NodeID || FrameID)
///
/// This is public for use in regeneration (Phase 2D) to detect basis changes.
pub fn compute_basis_hash(basis: &Basis) -> Result<Hash, StorageError> {
    let mut hasher = Hasher::new();

    match basis {
        Basis::Node(node_id) => {
            hasher.update(b"node:");
            hasher.update(node_id);
        }
        Basis::Frame(frame_id) => {
            hasher.update(b"frame:");
            hasher.update(frame_id);
        }
        Basis::Both { node, frame } => {
            hasher.update(b"both:");
            hasher.update(node);
            hasher.update(frame);
        }
    }

    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_id_deterministic() {
        let basis = Basis::Node([1u8; 32]);
        let content = b"test content";
        let frame_type = "analysis";
        let agent_id = "test-agent";

        let frame_id1 = compute_frame_id(&basis, content, frame_type, agent_id).unwrap();
        let frame_id2 = compute_frame_id(&basis, content, frame_type, agent_id).unwrap();

        assert_eq!(frame_id1, frame_id2);
    }

    #[test]
    fn test_frame_id_different_content_different_id() {
        let basis = Basis::Node([1u8; 32]);
        let content1 = b"test content";
        let content2 = b"different content";
        let frame_type = "analysis";
        let agent_id = "test-agent";

        let frame_id1 = compute_frame_id(&basis, content1, frame_type, agent_id).unwrap();
        let frame_id2 = compute_frame_id(&basis, content2, frame_type, agent_id).unwrap();

        assert_ne!(frame_id1, frame_id2);
    }

    #[test]
    fn test_frame_id_different_basis_different_id() {
        let basis1 = Basis::Node([1u8; 32]);
        let basis2 = Basis::Node([2u8; 32]);
        let content = b"test content";
        let frame_type = "analysis";
        let agent_id = "test-agent";

        let frame_id1 = compute_frame_id(&basis1, content, frame_type, agent_id).unwrap();
        let frame_id2 = compute_frame_id(&basis2, content, frame_type, agent_id).unwrap();

        assert_ne!(frame_id1, frame_id2);
    }

    #[test]
    fn test_frame_id_different_agent_different_id() {
        let basis = Basis::Node([1u8; 32]);
        let content = b"test content";
        let frame_type = "analysis";
        let agent_id1 = "agent-1";
        let agent_id2 = "agent-2";

        let frame_id1 = compute_frame_id(&basis, content, frame_type, agent_id1).unwrap();
        let frame_id2 = compute_frame_id(&basis, content, frame_type, agent_id2).unwrap();

        // Different agent IDs should produce different FrameIDs (Phase 2A requirement)
        assert_ne!(frame_id1, frame_id2);
    }

    #[test]
    fn test_frame_id_both_basis() {
        let basis = Basis::Both {
            node: [1u8; 32],
            frame: [2u8; 32],
        };
        let content = b"test content";
        let frame_type = "analysis";
        let agent_id = "test-agent";

        let frame_id = compute_frame_id(&basis, content, frame_type, agent_id).unwrap();

        // Should produce a different ID than node-only or frame-only
        let node_basis = Basis::Node([1u8; 32]);
        let node_frame_id = compute_frame_id(&node_basis, content, frame_type, agent_id).unwrap();

        assert_ne!(frame_id, node_frame_id);
    }
}
