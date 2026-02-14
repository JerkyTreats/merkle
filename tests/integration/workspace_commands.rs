//! Integration tests for workspace commands: ignore list, scan, validate.
//!
//! Covers merkle workspace ignore (list/add), merkle scan (idempotency, force,
//! ignore list and .gitignore sync), and merkle workspace validate (passed,
//! not scanned, JSON format).

use merkle::ignore;
use merkle::tooling::cli::{CliContext, Commands, WorkspaceCommands};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::integration::with_xdg_data_home;

#[test]
fn test_ignore_list_empty_missing_file() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Ignore {
                    path: None,
                    dry_run: false,
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(out.contains("empty") || out.contains("Ignore list"));
    });
}

#[test]
fn test_workspace_ignore_add_and_list() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let sub = workspace_root.join("ignored_dir");
        fs::create_dir_all(&sub).unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();

        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Ignore {
                    path: Some(PathBuf::from("ignored_dir")),
                    dry_run: false,
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(out.contains("Added") && out.contains("ignored_dir"));

        let list_out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Ignore {
                    path: None,
                    dry_run: false,
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(list_out.contains("ignored_dir"));
    });
}

#[test]
fn test_workspace_ignore_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let f = workspace_root.join("would_ignore.txt");
        fs::write(&f, "x").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Ignore {
                    path: Some(PathBuf::from("would_ignore.txt")),
                    dry_run: true,
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(out.contains("Would add"));
        let list_path = ignore::ignore_list_path(&workspace_root).unwrap();
        assert!(
            !list_path.exists()
                || fs::read_to_string(&list_path)
                    .unwrap_or_default()
                    .is_empty()
        );
    });
}

#[test]
fn test_scan_then_validate_passed() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("a.txt"), "a").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: true }).unwrap();
        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Validate {
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(out.contains("Validation passed"));
        assert!(out.contains("All checks passed"));
    });
}

#[test]
fn test_validate_not_scanned_warning() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("b.txt"), "b").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Validate {
                    format: "text".to_string(),
                },
            })
            .unwrap();
        assert!(out.contains("Root node not found") || out.contains("not be scanned"));
    });
}

#[test]
fn test_validate_format_json() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("c.txt"), "c").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: true }).unwrap();
        let out = ctx
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::Validate {
                    format: "json".to_string(),
                },
            })
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed
            .get("valid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false));
        assert!(parsed.get("root_hash").is_some());
        assert!(parsed.get("node_count").is_some());
        assert!(parsed.get("frame_count").is_some());
        assert!(parsed.get("errors").unwrap().as_array().unwrap().is_empty());
    });
}

#[test]
fn test_scan_without_force_already_exists() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("d.txt"), "d").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: true }).unwrap();
        let out = ctx.execute(&Commands::Scan { force: false }).unwrap();
        assert!(out.contains("already exists") && out.contains("--force"));
    });
}

#[test]
fn test_scan_with_force_repopulates() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("e.txt"), "e").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        let out1 = ctx.execute(&Commands::Scan { force: true }).unwrap();
        fs::write(workspace_root.join("f.txt"), "f").unwrap();
        let out2 = ctx.execute(&Commands::Scan { force: true }).unwrap();
        assert!(out1.contains("Scanned"));
        assert!(out2.contains("Scanned"));
        assert!(out1 != out2 || out2.contains("nodes"));
    });
}

#[test]
fn test_workspace_ignore_path_outside_workspace_errors() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let outside = temp_dir.path().join("other").join("path");
        fs::create_dir_all(&outside).unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        let result = ctx.execute(&Commands::Workspace {
            command: WorkspaceCommands::Ignore {
                path: Some(outside),
                dry_run: false,
                format: "text".to_string(),
            },
        });
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("outside") || err.to_string().contains("Path"));
    });
}

#[test]
fn test_scan_default_uses_gitignore_when_ignore_list_missing() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("keep.txt"), "keep").unwrap();
        fs::write(workspace_root.join(".gitignore"), "ignore_me\n").unwrap();
        fs::create_dir_all(workspace_root.join("ignore_me")).unwrap();
        fs::write(workspace_root.join("ignore_me").join("x"), "x").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: true }).unwrap();
        let records = ctx.api().node_store().list_all().unwrap();
        let paths: Vec<String> = records
            .iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .collect();
        assert!(paths.iter().any(|p| p.contains("keep")));
        assert!(!paths.iter().any(|p| p.contains("ignore_me")));
    });
}

#[test]
fn test_scan_syncs_gitignore_to_ignore_list() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("a.txt"), "a").unwrap();
        fs::write(workspace_root.join(".gitignore"), "synced_ignore\n*.log\n").unwrap();
        fs::create_dir_all(workspace_root.join("synced_ignore")).unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: true }).unwrap();
        let list_path = merkle::ignore::ignore_list_path(&workspace_root).unwrap();
        let contents = fs::read_to_string(&list_path).unwrap();
        assert!(contents.contains("# .gitignore"));
        assert!(contents.contains("# end .gitignore"));
        assert!(contents.contains("synced_ignore"));
        assert!(contents.contains("*.log"));
        let records = ctx.api().node_store().list_all().unwrap();
        let paths: Vec<String> = records
            .iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .collect();
        assert!(!paths.iter().any(|p| p.contains("synced_ignore")));
    });
}

#[test]
fn test_scan_respects_ignore_list() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("keep.txt"), "keep").unwrap();
        let skip_dir = workspace_root.join("skip_me");
        fs::create_dir_all(&skip_dir).unwrap();
        fs::write(skip_dir.join("file.txt"), "x").unwrap();
        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Workspace {
            command: WorkspaceCommands::Ignore {
                path: Some(PathBuf::from("skip_me")),
                dry_run: false,
                format: "text".to_string(),
            },
        })
        .unwrap();
        ctx.execute(&Commands::Scan { force: true }).unwrap();
        let records = ctx.api().node_store().list_all().unwrap();
        let paths: Vec<String> = records
            .iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .collect();
        assert!(paths.iter().any(|p| p.contains("keep")));
        assert!(!paths.iter().any(|p| p.contains("skip_me")));
    });
}

/// Regression: after `merkle scan`, `merkle status` must show the tree as scanned.
/// Guards against status using a different root computation (e.g. ignore config) than scan,
/// which would make the stored root not found and show "Scanned: no".
#[test]
fn test_scan_then_status_shows_scanned() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_data_home(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("kept.txt"), "content").unwrap();
        fs::write(workspace_root.join(".gitignore"), "ignored\n").unwrap();
        fs::create_dir_all(workspace_root.join("ignored")).unwrap();
        fs::write(workspace_root.join("ignored").join("x"), "x").unwrap();

        let ctx = CliContext::new(workspace_root.clone(), None).unwrap();
        ctx.execute(&Commands::Scan { force: true }).unwrap();

        let out = ctx
            .execute(&Commands::Status {
                format: "text".to_string(),
                workspace_only: true,
                agents_only: false,
                providers_only: false,
                breakdown: false,
                test_connectivity: false,
            })
            .unwrap();
        assert!(
            out.contains("Scanned: yes"),
            "status must show tree as scanned after scan; got: {}",
            out
        );
        assert!(!out.contains("Scanned: no"));
    });
}
