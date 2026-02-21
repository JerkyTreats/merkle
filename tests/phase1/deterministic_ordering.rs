use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use merkle::context::frame::{Basis, Frame};
use merkle::tooling::cli::{CliContext, Commands, ContextCommands, WorkspaceCommands};
use merkle::tooling::{WatchConfig, WatchDaemon};
use tempfile::TempDir;

use crate::phase1::support::{
    canonical, create_context_api, create_progress_runtime, latest_session_events,
    register_writer_agent_in_registry, with_xdg_env,
};

#[test]
fn unified_status_workspace_json_is_stable_across_runs() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("stable.txt"), "hello").unwrap();

        let cli = CliContext::new(workspace_root.clone(), None).unwrap();
        cli.execute(&Commands::Scan { force: true }).unwrap();

        let first = cli
            .execute(&Commands::Status {
                format: "json".to_string(),
                workspace_only: true,
                agents_only: false,
                providers_only: false,
                breakdown: false,
                test_connectivity: false,
            })
            .unwrap();

        let second = cli
            .execute(&Commands::Status {
                format: "json".to_string(),
                workspace_only: true,
                agents_only: false,
                providers_only: false,
                breakdown: false,
                test_connectivity: false,
            })
            .unwrap();

        let first_json: serde_json::Value = serde_json::from_str(&first).unwrap();
        let second_json: serde_json::Value = serde_json::from_str(&second).unwrap();
        assert_eq!(first_json, second_json);
    });
}

#[test]
fn list_deleted_json_path_and_id_order_is_stable() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let a = workspace_root.join("a.txt");
        let b = workspace_root.join("b.txt");
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();

        let cli = CliContext::new(workspace_root.clone(), None).unwrap();
        cli.execute(&Commands::Scan { force: true }).unwrap();

        cli.execute(&Commands::Workspace {
            command: WorkspaceCommands::Delete {
                path: Some(a.clone()),
                node: None,
                dry_run: false,
                no_ignore: false,
            },
        })
        .unwrap();

        cli.execute(&Commands::Workspace {
            command: WorkspaceCommands::Delete {
                path: Some(b.clone()),
                node: None,
                dry_run: false,
                no_ignore: false,
            },
        })
        .unwrap();

        let first = cli
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::ListDeleted {
                    older_than: None,
                    format: "json".to_string(),
                },
            })
            .unwrap();

        let second = cli
            .execute(&Commands::Workspace {
                command: WorkspaceCommands::ListDeleted {
                    older_than: None,
                    format: "json".to_string(),
                },
            })
            .unwrap();

        let first_json: serde_json::Value = serde_json::from_str(&first).unwrap();
        let second_json: serde_json::Value = serde_json::from_str(&second).unwrap();

        let project = |value: &serde_json::Value| -> Vec<(String, String)> {
            value
                .as_array()
                .expect("list deleted output should be a json array")
                .iter()
                .map(|entry| {
                    (
                        entry
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        entry
                            .get("node_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
                .collect()
        };

        assert_eq!(project(&first_json), project(&second_json));
    });
}

#[test]
fn context_get_deterministic_ordering_is_stable() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let target = workspace_root.join("ordered.txt");
        fs::write(&target, "ordered").unwrap();

        let cli = CliContext::new(workspace_root.clone(), None).unwrap();
        cli.execute(&Commands::Scan { force: true }).unwrap();

        let node_record = cli
            .api()
            .node_store()
            .find_by_path(&canonical(&target))
            .unwrap()
            .expect("target should exist in node store");
        let node_id = node_record.node_id;

        register_writer_agent_in_registry(cli.api(), "phase1-writer");

        let mut metadata = HashMap::new();
        metadata.insert("agent_id".to_string(), "phase1-writer".to_string());

        let frame_b = Frame::new(
            Basis::Node(node_id),
            b"frame-b".to_vec(),
            "zeta".to_string(),
            "phase1-writer".to_string(),
            metadata.clone(),
        )
        .unwrap();
        cli.api()
            .put_frame(node_id, frame_b, "phase1-writer".to_string())
            .unwrap();

        let frame_a = Frame::new(
            Basis::Node(node_id),
            b"frame-a".to_vec(),
            "alpha".to_string(),
            "phase1-writer".to_string(),
            metadata,
        )
        .unwrap();
        cli.api()
            .put_frame(node_id, frame_a, "phase1-writer".to_string())
            .unwrap();

        let run_get = || {
            cli.execute(&Commands::Context {
                command: ContextCommands::Get {
                    node: None,
                    path: Some(target.clone()),
                    agent: None,
                    frame_type: None,
                    max_frames: 10,
                    ordering: "deterministic".to_string(),
                    combine: false,
                    separator: "\n\n---\n\n".to_string(),
                    format: "json".to_string(),
                    include_metadata: false,
                    include_deleted: false,
                },
            })
            .unwrap()
        };

        let first = run_get();
        let second = run_get();

        let first_json: serde_json::Value = serde_json::from_str(&first).unwrap();
        let second_json: serde_json::Value = serde_json::from_str(&second).unwrap();

        assert_eq!(first_json, second_json);

        let frame_types: Vec<String> = first_json
            .get("frames")
            .and_then(|v| v.as_array())
            .expect("frames should be present")
            .iter()
            .map(|f| {
                f.get("frame_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();

        let mut sorted = frame_types.clone();
        sorted.sort();
        assert_eq!(frame_types, sorted);
    });
}

#[test]
fn watch_contract_is_stable_for_defaults_and_startup_event_sequence() {
    let defaults = WatchConfig::default();
    assert_eq!(defaults.debounce_ms, 100);
    assert_eq!(defaults.batch_window_ms, 50);
    assert_eq!(
        defaults.ignore_patterns,
        vec![
            "**/.git/**".to_string(),
            "**/.merkle/**".to_string(),
            "**/target/**".to_string(),
            "**/node_modules/**".to_string(),
            "**/.DS_Store".to_string(),
            "**/*.swp".to_string(),
            "**/*.tmp".to_string(),
        ]
    );

    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_root).unwrap();
    fs::write(workspace_root.join("watch.txt"), "watch").unwrap();

    let api = Arc::new(create_context_api(&workspace_root));
    let progress_db = temp_dir.path().join("progress-db");
    let progress = create_progress_runtime(&progress_db);
    let session_id = progress.start_command_session("watch".to_string()).unwrap();

    let mut config = WatchConfig::default();
    config.workspace_root = workspace_root;
    config.auto_create_frames = false;
    config.debounce_ms = 20;
    config.batch_window_ms = 20;
    config.session_id = Some(session_id.clone());
    config.progress = Some(Arc::clone(&progress));

    let daemon = Arc::new(WatchDaemon::new(api, config).unwrap());
    let daemon_for_thread = Arc::clone(&daemon);
    let worker = std::thread::spawn(move || daemon_for_thread.start());

    std::thread::sleep(Duration::from_millis(120));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        daemon.stop().await.unwrap();
    });

    let start_result = worker.join().expect("watch thread should join");
    start_result.expect("watch daemon should stop cleanly");

    progress
        .finish_command_session(&session_id, true, None)
        .unwrap();

    let events = latest_session_events(&progress, "watch");
    assert_eq!(
        events.first().map(|e| e.event_type.as_str()),
        Some("session_started")
    );
    assert!(events.iter().any(|e| e.event_type == "watch_started"));
    assert_eq!(
        events.last().map(|e| e.event_type.as_str()),
        Some("session_ended")
    );
    assert!(events.windows(2).all(|w| w[1].seq == w[0].seq + 1));
}
