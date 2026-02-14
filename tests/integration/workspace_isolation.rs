//! Integration tests for Workspace Isolation
//!
//! Tests verify that multiple merkle workspaces are properly isolated:
//! - Each workspace has its own XDG data directory
//! - Data in one workspace doesn't affect another
//! - Workspaces can have the same file structure but remain isolated

use merkle::frame::{Basis, Frame};
use merkle::heads::HeadIndex;
use merkle::store::{NodeRecord, NodeType};
use merkle::tooling::cli::CliContext;
use merkle::types::{Hash, NodeID};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::integration::with_xdg_data_home;

/// Test that two separate workspaces have different XDG data directories
#[test]
fn test_workspace_isolation_xdg_directories() {
    let test_dir = TempDir::new().unwrap();
    let workspace1 = TempDir::new().unwrap();
    let workspace2 = TempDir::new().unwrap();

    with_xdg_data_home(&test_dir, || {
        // Get XDG data directories for each workspace
        let data_dir1 = merkle::config::xdg::workspace_data_dir(workspace1.path()).unwrap();
        let data_dir2 = merkle::config::xdg::workspace_data_dir(workspace2.path()).unwrap();

        // Verify they are different
        assert_ne!(
            data_dir1, data_dir2,
            "Each workspace should have a unique XDG data directory"
        );

        // Verify the paths are based on the workspace paths
        assert!(data_dir1
            .to_string_lossy()
            .contains(workspace1.path().to_string_lossy().as_ref()));
        assert!(data_dir2
            .to_string_lossy()
            .contains(workspace2.path().to_string_lossy().as_ref()));
    });
}

/// Test that data stored in one workspace doesn't appear in another
#[test]
fn test_workspace_isolation_data_isolation() {
    let test_dir = TempDir::new().unwrap();
    let workspace1 = TempDir::new().unwrap();
    let workspace2 = TempDir::new().unwrap();

    with_xdg_data_home(&test_dir, || {
        // Create identical file structures in both workspaces
        fs::write(workspace1.path().join("test.txt"), "content1").unwrap();
        fs::write(workspace2.path().join("test.txt"), "content2").unwrap();

        // Initialize CLI contexts for both workspaces
        let ctx1 = CliContext::new(workspace1.path().to_path_buf(), None).unwrap();
        let ctx2 = CliContext::new(workspace2.path().to_path_buf(), None).unwrap();

        // Create a test node ID
        let node_id: NodeID = [1u8; 32];

        // Create a test node record for workspace 1
        let node_record1 = NodeRecord {
            node_id,
            path: PathBuf::from("test.txt"),
            node_type: NodeType::File {
                size: 8,
                content_hash: [0u8; 32],
            },
            children: vec![],
            parent: None,
            frame_set_root: None,
            metadata: std::collections::HashMap::new(),
            tombstoned_at: None,
        };

        // Store node in workspace 1
        ctx1.api().node_store().put(&node_record1).unwrap();

        // Verify node exists in workspace 1
        let retrieved1 = ctx1.api().node_store().get(&node_id).unwrap();
        assert!(retrieved1.is_some(), "Node should exist in workspace 1");

        // Verify node does NOT exist in workspace 2
        let retrieved2 = ctx2.api().node_store().get(&node_id).unwrap();
        assert!(retrieved2.is_none(), "Node should NOT exist in workspace 2");
    });
}

/// Test that frames stored in one workspace don't appear in another
#[test]
fn test_workspace_isolation_frame_isolation() {
    let test_dir = TempDir::new().unwrap();
    let workspace1 = TempDir::new().unwrap();
    let workspace2 = TempDir::new().unwrap();

    with_xdg_data_home(&test_dir, || {
        // Initialize CLI contexts for both workspaces
        let ctx1 = CliContext::new(workspace1.path().to_path_buf(), None).unwrap();
        let ctx2 = CliContext::new(workspace2.path().to_path_buf(), None).unwrap();

        // Create a test frame for workspace 1
        let node_id: NodeID = [1u8; 32];
        let basis = Basis::Node(node_id);
        let content = b"workspace1 content".to_vec();
        let frame1 = Frame::new(
            basis,
            content,
            "test".to_string(),
            "agent1".to_string(),
            std::collections::HashMap::new(),
        )
        .unwrap();

        // Store frame in workspace 1
        ctx1.api().frame_storage().store(&frame1).unwrap();

        // Verify frame exists in workspace 1
        let retrieved1 = ctx1.api().frame_storage().get(&frame1.frame_id).unwrap();
        assert!(retrieved1.is_some(), "Frame should exist in workspace 1");

        // Verify frame does NOT exist in workspace 2
        let retrieved2 = ctx2.api().frame_storage().get(&frame1.frame_id).unwrap();
        assert!(
            retrieved2.is_none(),
            "Frame should NOT exist in workspace 2"
        );
    });
}

/// Test that head indices are isolated between workspaces
#[test]
fn test_workspace_isolation_head_index_isolation() {
    let test_dir = TempDir::new().unwrap();
    let workspace1 = TempDir::new().unwrap();
    let workspace2 = TempDir::new().unwrap();

    with_xdg_data_home(&test_dir, || {
        // Initialize CLI contexts for both workspaces
        let ctx1 = CliContext::new(workspace1.path().to_path_buf(), None).unwrap();
        let ctx2 = CliContext::new(workspace2.path().to_path_buf(), None).unwrap();

        let node_id: NodeID = [1u8; 32];
        let frame_id = merkle::types::FrameID::from([2u8; 32]);

        // Add head entry in workspace 1
        {
            let mut head_index = ctx1.api().head_index().write();
            head_index.update_head(&node_id, "test", &frame_id).unwrap();
        }

        // Verify head exists in workspace 1
        {
            let head_index = ctx1.api().head_index().read();
            let head = head_index.get_head(&node_id, "test").unwrap();
            assert_eq!(head, Some(frame_id), "Head should exist in workspace 1");
        }

        // Verify head does NOT exist in workspace 2
        {
            let head_index = ctx2.api().head_index().read();
            let head = head_index.get_head(&node_id, "test").unwrap();
            assert_eq!(head, None, "Head should NOT exist in workspace 2");
        }
    });
}

/// Test that basis indices are isolated between workspaces
#[test]
fn test_workspace_isolation_basis_index_isolation() {
    let test_dir = TempDir::new().unwrap();
    let workspace1 = TempDir::new().unwrap();
    let workspace2 = TempDir::new().unwrap();

    with_xdg_data_home(&test_dir, || {
        // Initialize CLI contexts for both workspaces
        let ctx1 = CliContext::new(workspace1.path().to_path_buf(), None).unwrap();
        let ctx2 = CliContext::new(workspace2.path().to_path_buf(), None).unwrap();

        let basis_hash: Hash = [1u8; 32].into();
        let frame_id = merkle::types::FrameID::from([2u8; 32]);

        // Add basis entry in workspace 1
        {
            let mut basis_index = ctx1.api().basis_index().write();
            basis_index.add_frame(basis_hash, frame_id);
        }

        // Verify basis entry exists in workspace 1
        {
            let basis_index = ctx1.api().basis_index().read();
            let frames = basis_index.get_frames_by_basis(&basis_hash);
            assert_eq!(frames.len(), 1, "Basis entry should exist in workspace 1");
            assert_eq!(frames[0], frame_id);
        }

        // Verify basis entry does NOT exist in workspace 2
        {
            let basis_index = ctx2.api().basis_index().read();
            let frames = basis_index.get_frames_by_basis(&basis_hash);
            assert_eq!(
                frames.len(),
                0,
                "Basis entry should NOT exist in workspace 2"
            );
        }
    });
}

/// Test that persistence files are isolated between workspaces
#[test]
fn test_workspace_isolation_persistence_isolation() {
    let test_dir = TempDir::new().unwrap();
    let workspace1 = TempDir::new().unwrap();
    let workspace2 = TempDir::new().unwrap();

    with_xdg_data_home(&test_dir, || {
        // Initialize CLI contexts for both workspaces
        let ctx1 = CliContext::new(workspace1.path().to_path_buf(), None).unwrap();
        let ctx2 = CliContext::new(workspace2.path().to_path_buf(), None).unwrap();

        let node_id: NodeID = [1u8; 32];
        let frame_id = merkle::types::FrameID::from([2u8; 32]);

        // Add head entry in workspace 1 and save
        {
            let mut head_index = ctx1.api().head_index().write();
            head_index.update_head(&node_id, "test", &frame_id).unwrap();
        }
        {
            let head_index = ctx1.api().head_index().read();
            let head_index_path = HeadIndex::persistence_path(workspace1.path());
            head_index.save_to_disk(&head_index_path).unwrap();
        }

        // Add different head entry in workspace 2 and save
        let frame_id2 = merkle::types::FrameID::from([3u8; 32]);
        {
            let mut head_index = ctx2.api().head_index().write();
            head_index
                .update_head(&node_id, "test", &frame_id2)
                .unwrap();
        }
        {
            let head_index = ctx2.api().head_index().read();
            let head_index_path = HeadIndex::persistence_path(workspace2.path());
            head_index.save_to_disk(&head_index_path).unwrap();
        }

        // Verify persistence files are in different locations
        let persistence_path1 = HeadIndex::persistence_path(workspace1.path());
        let persistence_path2 = HeadIndex::persistence_path(workspace2.path());
        assert_ne!(
            persistence_path1, persistence_path2,
            "Persistence files should be in different locations"
        );
        assert!(
            persistence_path1.exists(),
            "Workspace 1 persistence file should exist"
        );
        assert!(
            persistence_path2.exists(),
            "Workspace 2 persistence file should exist"
        );

        // Drop the original contexts to release database locks
        drop(ctx1);
        drop(ctx2);

        // Create new contexts to verify persistence
        let ctx1_reload = CliContext::new(workspace1.path().to_path_buf(), None).unwrap();
        let ctx2_reload = CliContext::new(workspace2.path().to_path_buf(), None).unwrap();

        // Verify workspace 1 still has its data
        {
            let head_index = ctx1_reload.api().head_index().read();
            let head = head_index.get_head(&node_id, "test").unwrap();
            assert_eq!(
                head,
                Some(frame_id),
                "Workspace 1 should have its original data"
            );
        }

        // Verify workspace 2 still has its data
        {
            let head_index = ctx2_reload.api().head_index().read();
            let head = head_index.get_head(&node_id, "test").unwrap();
            assert_eq!(
                head,
                Some(frame_id2),
                "Workspace 2 should have its original data"
            );
        }
    });
}

/// Test that workspaces with the same file structure remain isolated
#[test]
fn test_workspace_isolation_same_structure() {
    let test_dir = TempDir::new().unwrap();
    let workspace1 = TempDir::new().unwrap();
    let workspace2 = TempDir::new().unwrap();

    with_xdg_data_home(&test_dir, || {
        // Create identical file structures
        fs::write(workspace1.path().join("file1.txt"), "content").unwrap();
        fs::write(workspace1.path().join("file2.txt"), "content").unwrap();
        fs::write(workspace2.path().join("file1.txt"), "content").unwrap();
        fs::write(workspace2.path().join("file2.txt"), "content").unwrap();

        // Initialize CLI contexts (just to verify they can be created)
        let _ctx1 = CliContext::new(workspace1.path().to_path_buf(), None).unwrap();
        let _ctx2 = CliContext::new(workspace2.path().to_path_buf(), None).unwrap();

        // Build trees - they should have different root hashes because paths are different
        // (even though content is the same, the workspace paths differ)
        use merkle::tree::builder::TreeBuilder;
        let builder1 = TreeBuilder::new(workspace1.path().to_path_buf());
        let builder2 = TreeBuilder::new(workspace2.path().to_path_buf());
        let root1 = builder1.compute_root().unwrap();
        let root2 = builder2.compute_root().unwrap();

        // Different workspace paths should produce different root hashes
        // (the tree includes path information, so different locations = different hashes)
        assert_ne!(
            root1, root2,
            "Different workspace paths should produce different root hashes"
        );

        // But workspaces should still be isolated
        let data_dir1 = merkle::config::xdg::workspace_data_dir(workspace1.path()).unwrap();
        let data_dir2 = merkle::config::xdg::workspace_data_dir(workspace2.path()).unwrap();
        assert_ne!(
            data_dir1, data_dir2,
            "Workspaces should have different data directories even with same content"
        );
    });
}
