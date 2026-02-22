//! Integration tests for tombstone-based node deletion: delete, restore, compact, list-deleted.

use merkle::cli::{Commands, RunContext, WorkspaceCommands};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::integration::with_xdg_data_home;

#[test]
fn test_workspace_delete_and_list_deleted() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("ws");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("a.txt"), "a").unwrap();
        fs::create_dir_all(workspace_root.join("sub")).unwrap();
        fs::write(workspace_root.join("sub").join("b.txt"), "b").unwrap();

        let ctx = RunContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: false }).unwrap();

        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Delete {
                    path: Some(PathBuf::from("sub")),
                    node: None,
                    dry_run: false,
                    no_ignore: true,
                },
            })
            .unwrap();
        assert!(out.contains("Deleted") && out.contains("nodes"));

        let list_out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::ListDeleted {
                    older_than: None,
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(list_out.contains("sub") || list_out.contains("Path"));
    });
}

#[test]
fn test_workspace_delete_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("ws");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("f.txt"), "x").unwrap();
        let ctx = RunContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: false }).unwrap();

        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Delete {
                    path: Some(PathBuf::from("f.txt")),
                    node: None,
                    dry_run: true,
                    no_ignore: true,
                },
            })
            .unwrap();
        assert!(out.contains("Would delete") && out.contains("nodes"));

        let list_out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::ListDeleted {
                    older_than: None,
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(!list_out.contains("f.txt") || list_out.contains("empty") || list_out == "");
    });
}

#[test]
fn test_workspace_restore() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("ws");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("r.txt"), "r").unwrap();
        let ctx = RunContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: false }).unwrap();
        ctx.execute(&Commands::Workspace {
            command: WorkspaceCommands::Delete {
                path: Some(PathBuf::from("r.txt")),
                node: None,
                dry_run: false,
                no_ignore: true,
            },
        })
        .unwrap();

        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Restore {
                    path: Some(PathBuf::from("r.txt")),
                    node: None,
                    dry_run: false,
                },
            })
            .unwrap();
        assert!(out.contains("Restored") && out.contains("nodes"));
    });
}

#[test]
fn test_workspace_compact_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("ws");
        fs::create_dir_all(&workspace_root).unwrap();
        let ctx = RunContext::new(workspace_root.clone(), None).unwrap();
        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Compact {
                    ttl: Some(90),
                    all: false,
                    keep_frames: true,
                    dry_run: true,
                },
            })
            .unwrap();
        assert!(out.contains("Would compact") || out.contains("0 nodes"));
    });
}

#[test]
fn test_list_deleted_json_format() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("ws");
        fs::create_dir_all(&workspace_root).unwrap();
        let ctx = RunContext::new(workspace_root.clone(), None).unwrap();
        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::ListDeleted {
                    older_than: None,
                    format: "json".to_string(),
                },
            })
            .unwrap();
        let _: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    });
}
