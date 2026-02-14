use std::fs;

use merkle::config::{xdg, ProviderConfig, ProviderType};
use merkle::progress::{PrunePolicy, SessionStatus};
use merkle::provider::CompletionOptions;
use merkle::tooling::cli::{
    AgentCommands, CliContext, Commands, ContextCommands, ProviderCommands, WorkspaceCommands,
};
use tempfile::TempDir;

use crate::integration::with_xdg_env;

fn create_test_openai_provider(provider_name: &str, model: &str, endpoint: &str) {
    let providers_dir = xdg::providers_dir().unwrap();
    fs::create_dir_all(&providers_dir).unwrap();
    let config_path = providers_dir.join(format!("{}.toml", provider_name));
    let provider_config = ProviderConfig {
        provider_name: Some(provider_name.to_string()),
        provider_type: ProviderType::OpenAI,
        model: model.to_string(),
        api_key: Some("test-api-key".to_string()),
        endpoint: Some(endpoint.to_string()),
        default_options: CompletionOptions::default(),
    };
    let toml = toml::to_string_pretty(&provider_config).unwrap();
    fs::write(config_path, toml).unwrap();
}

#[test]
fn scan_emits_session_boundary_events() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("a.txt"), "hello").unwrap();

        let cli = CliContext::new(workspace_root, None).unwrap();
        cli.execute(&Commands::Scan { force: true }).unwrap();

        let runtime = cli.progress_runtime();
        let sessions = runtime.store().list_sessions().unwrap();
        let scan_session = sessions
            .iter()
            .find(|s| s.command == "scan")
            .expect("scan session should exist");
        assert_eq!(scan_session.status, SessionStatus::Completed);

        let events = runtime
            .store()
            .read_events(&scan_session.session_id)
            .unwrap();
        assert!(events.len() >= 2);
        assert_eq!(events.first().unwrap().event_type, "session_started");
        assert_eq!(events.first().unwrap().seq, 1);
        assert_eq!(events.last().unwrap().event_type, "session_ended");
        assert!(events.windows(2).all(|w| w[1].seq == w[0].seq + 1));
        assert!(events.iter().any(|e| e.event_type == "scan_started"));
        assert!(events.iter().any(|e| e.event_type == "scan_progress"));
        assert!(events.iter().any(|e| e.event_type == "scan_completed"));
        assert!(events.iter().any(|e| e.event_type == "command_summary"));
    });
}

#[test]
fn failed_command_emits_session_end() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let target = workspace_root.join("a.txt");
        fs::write(&target, "hello").unwrap();

        let cli = CliContext::new(workspace_root.clone(), None).unwrap();
        cli.execute(&Commands::Scan { force: true }).unwrap();
        let result = cli.execute(&Commands::Context {
            command: ContextCommands::Generate {
                node: None,
                path: Some(target),
                path_positional: None,
                agent: None,
                provider: None,
                frame_type: None,
                force: false,
            },
        });
        assert!(result.is_err());

        let runtime = cli.progress_runtime();
        let sessions = runtime.store().list_sessions().unwrap();
        let failed_session = sessions
            .iter()
            .find(|s| s.command == "context.generate")
            .expect("context generate session should exist");
        assert_eq!(failed_session.status, SessionStatus::Failed);

        let events = runtime
            .store()
            .read_events(&failed_session.session_id)
            .unwrap();
        assert_eq!(events.first().unwrap().event_type, "session_started");
        assert_eq!(events.last().unwrap().event_type, "session_ended");
        assert!(events.iter().any(|e| e.event_type == "command_summary"));
    });
}

#[test]
fn context_get_emits_summary_event() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let target = workspace_root.join("a.txt");
        fs::write(&target, "hello").unwrap();

        let cli = CliContext::new(workspace_root.clone(), None).unwrap();
        cli.execute(&Commands::Scan { force: true }).unwrap();
        cli.execute(&Commands::Context {
            command: ContextCommands::Get {
                node: None,
                path: Some(target),
                agent: None,
                frame_type: None,
                max_frames: 5,
                ordering: "recency".to_string(),
                combine: false,
                separator: "\n".to_string(),
                format: "json".to_string(),
                include_metadata: false,
                include_deleted: false,
            },
        })
        .unwrap();

        let runtime = cli.progress_runtime();
        let sessions = runtime.store().list_sessions().unwrap();
        let context_get_session = sessions
            .iter()
            .find(|s| s.command == "context.get")
            .expect("context.get session should exist");

        let events = runtime
            .store()
            .read_events(&context_get_session.session_id)
            .unwrap();
        assert!(events
            .iter()
            .any(|e| e.event_type == "context_read_summary"));
        assert!(events.iter().any(|e| e.event_type == "command_summary"));
    });
}

#[test]
fn command_families_emit_typed_summaries_with_command_summary() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::write(workspace_root.join("a.txt"), "hello").unwrap();

        let cli = CliContext::new(workspace_root, None).unwrap();

        let checks: Vec<(Commands, &str, &str)> = vec![
            (
                Commands::Workspace {
                    command: WorkspaceCommands::Status {
                        format: "text".to_string(),
                        breakdown: false,
                    },
                },
                "workspace.status",
                "status_summary",
            ),
            (
                Commands::Workspace {
                    command: WorkspaceCommands::Validate {
                        format: "text".to_string(),
                    },
                },
                "workspace.validate",
                "validate_summary",
            ),
            (
                Commands::Workspace {
                    command: WorkspaceCommands::Ignore {
                        path: None,
                        dry_run: false,
                        format: "text".to_string(),
                    },
                },
                "workspace.ignore",
                "config_mutation_summary",
            ),
            (
                Commands::Workspace {
                    command: WorkspaceCommands::ListDeleted {
                        older_than: None,
                        format: "text".to_string(),
                    },
                },
                "workspace.list_deleted",
                "list_summary",
            ),
            (
                Commands::Status {
                    format: "text".to_string(),
                    workspace_only: false,
                    agents_only: false,
                    providers_only: false,
                    breakdown: false,
                    test_connectivity: false,
                },
                "status",
                "status_summary",
            ),
            (Commands::Validate, "validate", "validate_summary"),
            (
                Commands::Agent {
                    command: AgentCommands::List {
                        format: "text".to_string(),
                        role: None,
                    },
                },
                "agent.list",
                "config_mutation_summary",
            ),
            (
                Commands::Provider {
                    command: ProviderCommands::List {
                        format: "text".to_string(),
                        type_filter: None,
                    },
                },
                "provider.list",
                "config_mutation_summary",
            ),
            (
                Commands::Init {
                    force: false,
                    list: true,
                },
                "init",
                "init_summary",
            ),
        ];

        for (command, _, _) in &checks {
            cli.execute(command).unwrap();
        }

        let runtime = cli.progress_runtime();
        let sessions = runtime.store().list_sessions().unwrap();

        for (_, command_name, typed_event_type) in checks {
            let session = sessions
                .iter()
                .find(|s| s.command == command_name)
                .unwrap_or_else(|| panic!("session {command_name} should exist"));
            let events = runtime.store().read_events(&session.session_id).unwrap();

            let typed_idx = events
                .iter()
                .position(|e| e.event_type == typed_event_type)
                .unwrap_or_else(|| {
                    panic!("{typed_event_type} should be emitted for {command_name}")
                });
            let generic_idx = events
                .iter()
                .position(|e| e.event_type == "command_summary")
                .expect("command_summary should be emitted");
            let ended_idx = events
                .iter()
                .position(|e| e.event_type == "session_ended")
                .expect("session_ended should be emitted");

            assert!(typed_idx < ended_idx);
            assert!(generic_idx < ended_idx);
        }
    });
}

#[test]
fn regenerate_failure_emits_regeneration_failed_event() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let cli = CliContext::new(workspace_root, None).unwrap();
        let node_id = hex::encode([7u8; 32]);
        let result = cli.execute(&Commands::Regenerate {
            node_id,
            recursive: false,
            agent_id: "missing-agent".to_string(),
        });
        assert!(result.is_err());

        let runtime = cli.progress_runtime();
        let sessions = runtime.store().list_sessions().unwrap();
        let regen_session = sessions
            .iter()
            .find(|s| s.command == "regenerate")
            .expect("regenerate session should exist");
        let events = runtime
            .store()
            .read_events(&regen_session.session_id)
            .unwrap();
        assert!(events
            .iter()
            .any(|e| e.event_type == "regeneration_started"));
        assert!(events.iter().any(|e| e.event_type == "regeneration_failed"));
    });
}

#[test]
fn provider_test_failure_emits_provider_request_failed_event() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        create_test_openai_provider("provider-test-fail", "gpt-4-test", "http://127.0.0.1:9");

        let cli = CliContext::new(workspace_root, None).unwrap();
        let result = cli.execute(&Commands::Provider {
            command: ProviderCommands::Test {
                provider_name: "provider-test-fail".to_string(),
                model: Some("gpt-4-test".to_string()),
                timeout: 1,
            },
        });
        assert!(result.is_ok());

        let runtime = cli.progress_runtime();
        let sessions = runtime.store().list_sessions().unwrap();
        let session = sessions
            .iter()
            .find(|s| s.command == "provider.test")
            .expect("provider.test session should exist");
        let events = runtime.store().read_events(&session.session_id).unwrap();

        let sent_idx = events
            .iter()
            .position(|e| e.event_type == "provider_request_sent")
            .expect("provider_request_sent should be emitted");
        let failed_idx = events
            .iter()
            .position(|e| e.event_type == "provider_request_failed")
            .expect("provider_request_failed should be emitted");
        let ended_idx = events
            .iter()
            .position(|e| e.event_type == "session_ended")
            .expect("session_ended should be emitted");

        assert!(events
            .iter()
            .all(|e| e.event_type != "provider_response_received"));
        assert!(sent_idx < failed_idx);
        assert!(failed_idx < ended_idx);
    });
}

#[test]
fn interrupted_session_remains_readable() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let cli = CliContext::new(workspace_root, None).unwrap();
        let runtime = cli.progress_runtime();
        let session_id = runtime
            .start_command_session("manual.long_running".to_string())
            .unwrap();

        let changed = runtime.mark_interrupted_sessions().unwrap();
        assert_eq!(changed, 1);
        let session = runtime
            .store()
            .get_session(&session_id)
            .unwrap()
            .expect("session should exist");
        assert_eq!(session.status, SessionStatus::Interrupted);

        let events = runtime.store().read_events(&session_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "session_started");
    });
}

#[test]
fn pruning_removes_only_old_completed_sessions() {
    let temp_dir = TempDir::new().unwrap();
    with_xdg_env(&temp_dir, || {
        let workspace_root = temp_dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();

        let cli = CliContext::new(workspace_root, None).unwrap();
        let runtime = cli.progress_runtime();

        let s1 = runtime.start_command_session("one".to_string()).unwrap();
        runtime.finish_command_session(&s1, true, None).unwrap();
        let s2 = runtime.start_command_session("two".to_string()).unwrap();
        runtime.finish_command_session(&s2, true, None).unwrap();
        let active = runtime.start_command_session("active".to_string()).unwrap();

        let removed = runtime
            .prune(PrunePolicy {
                max_completed: 1,
                max_age_ms: u64::MAX,
            })
            .unwrap();
        assert!(removed >= 1);

        let sessions = runtime.store().list_sessions().unwrap();
        let completed_count = sessions
            .iter()
            .filter(|s| s.status == SessionStatus::Completed)
            .count();
        assert_eq!(completed_count, 1);
        assert!(sessions.iter().any(|s| s.session_id == active));
    });
}
