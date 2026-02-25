//! CLI route: single route table and run context. Dispatches to domain services and presentation.

use crate::agent::AgentCommandService;
use crate::api::ContextApi;
use crate::config::ConfigLoader;
use crate::context::generation::run::{run_generate, GenerateRequest};
use crate::context::query::get_node_for_cli;
use crate::error::ApiError;
use crate::heads::HeadIndex;
use crate::ignore;
use crate::store::persistence::SledNodeRecordStore;
use crate::telemetry::emission::{emit_command_summary, truncate_for_summary};
use crate::telemetry::{ProgressRuntime, ProviderLifecycleEventData};
use crate::telemetry::sessions::policy::PrunePolicy;
use crate::tree::walker::WalkerConfig;
use crate::workspace::{
    format_unified_status_text, format_workspace_status_text, WatchConfig, WatchDaemon,
    WorkspaceCommandService, WorkspaceStatusRequest,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::cli::parse::{
    AgentCommands, AgentPromptCommands, Commands, ContextCommands, ProviderCommands,
    WorkspaceCommands,
};
use crate::cli::{command_name, summary_descriptor};

/// Runtime context for CLI execution: workspace, config paths, and domain facades.
/// Built from workspace path and optional config path using ConfigLoader only.
pub struct RunContext {
    api: Arc<ContextApi>,
    workspace_root: PathBuf,
    config_path: Option<PathBuf>,
    #[allow(dead_code)]
    store_path: PathBuf,
    frame_storage_path: PathBuf,
    progress: Arc<ProgressRuntime>,
}

impl RunContext {
    /// Reference to the underlying context API.
    pub fn api(&self) -> &ContextApi {
        &self.api
    }

    /// Progress runtime for session and event emission.
    pub fn progress_runtime(&self) -> Arc<ProgressRuntime> {
        Arc::clone(&self.progress)
    }

    /// Create run context from workspace root and optional config path. Uses ConfigLoader only.
    pub fn new(workspace_root: PathBuf, config_path: Option<PathBuf>) -> Result<Self, ApiError> {
        let config = if let Some(ref cfg_path) = config_path {
            ConfigLoader::load_from_file(cfg_path)?
        } else {
            ConfigLoader::load(&workspace_root)?
        };

        let (store_path, frame_storage_path) =
            config.system.storage.resolve_paths(&workspace_root)?;

        std::fs::create_dir_all(&store_path)
            .map_err(|e| ApiError::StorageError(crate::error::StorageError::IoError(e)))?;

        let db = sled::open(&store_path).map_err(|e| {
            ApiError::StorageError(crate::error::StorageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to open sled database: {}", e),
            )))
        })?;
        let node_store = Arc::new(SledNodeRecordStore::from_db(db.clone()));
        let progress = Arc::new(ProgressRuntime::new(db).map_err(ApiError::StorageError)?);

        std::fs::create_dir_all(&frame_storage_path)
            .map_err(|e| ApiError::StorageError(crate::error::StorageError::IoError(e)))?;
        let frame_storage = Arc::new(
            crate::context::frame::open_storage(&frame_storage_path)
                .map_err(|e| ApiError::StorageError(e))?,
        );
        let head_index_path = HeadIndex::persistence_path(&workspace_root);
        let head_index = Arc::new(parking_lot::RwLock::new(
            HeadIndex::load_from_disk(&head_index_path).unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to load head index from disk: {}, starting with empty index",
                    e
                );
                HeadIndex::new()
            }),
        ));

        let mut agent_registry = crate::agent::AgentRegistry::new();
        agent_registry.load_from_config(&config)?;
        agent_registry.load_from_xdg()?;

        let mut provider_registry = crate::provider::ProviderRegistry::new();
        provider_registry.load_from_config(&config)?;
        provider_registry.load_from_xdg()?;

        let agent_registry = Arc::new(parking_lot::RwLock::new(agent_registry));
        let provider_registry = Arc::new(parking_lot::RwLock::new(provider_registry));
        let lock_manager = Arc::new(crate::concurrency::NodeLockManager::new());

        let api = ContextApi::with_workspace_root(
            node_store,
            frame_storage,
            head_index,
            agent_registry,
            provider_registry,
            lock_manager,
            workspace_root.clone(),
        );

        let (store_path, frame_storage_path) =
            config.system.storage.resolve_paths(&workspace_root)?;

        Ok(Self {
            api: Arc::new(api),
            workspace_root,
            config_path,
            store_path,
            frame_storage_path,
            progress,
        })
    }

    /// Execute a CLI command via the single route table.
    pub fn execute(&self, command: &Commands) -> Result<String, ApiError> {
        let started = Instant::now();
        let session_id = self.progress.start_command_session(command_name(command))?;
        let result = self.execute_inner(command, &session_id);
        self.emit_command_summary(
            &session_id,
            command,
            result.as_ref(),
            started.elapsed().as_millis(),
        );
        let ok = result.is_ok();
        let err = result.as_ref().err().map(|e| e.to_string());
        self.progress
            .finish_command_session(&session_id, ok, err)?;
        let _ = self.progress.prune(PrunePolicy::default());
        result
    }

    fn execute_inner(&self, command: &Commands, session_id: &str) -> Result<String, ApiError> {
        match command {
            Commands::Scan { force } => {
                self.progress.emit_event_best_effort(
                    session_id,
                    "scan_started",
                    json!({ "force": force }),
                );
                WorkspaceCommandService::scan(
                    self.api.as_ref(),
                    &self.workspace_root,
                    *force,
                    Some(&self.progress),
                    Some(session_id),
                )
            }
            Commands::Workspace { command } => self.handle_workspace_command(command),
            Commands::Status {
                format,
                workspace_only,
                agents_only,
                providers_only,
                breakdown,
                test_connectivity,
            } => {
                let include_all =
                    !*workspace_only && !*agents_only && !*providers_only;
                let include_workspace = include_all || *workspace_only;
                let include_agents = include_all || *agents_only;
                let include_providers = include_all || *providers_only;
                let registry_agent = self.api.agent_registry().read();
                let registry_provider = self.api.provider_registry().read();
                let unified = WorkspaceCommandService::unified_status(
                    self.api.as_ref(),
                    self.workspace_root.as_path(),
                    self.store_path.as_path(),
                    &registry_agent,
                    &registry_provider,
                    include_workspace,
                    include_agents,
                    include_providers,
                    *breakdown,
                    *test_connectivity,
                )?;
                if *format == "json" {
                    serde_json::to_string_pretty(&unified).map_err(|e| {
                        ApiError::StorageError(crate::error::StorageError::InvalidPath(
                            e.to_string(),
                        ))
                    })
                } else {
                    Ok(format_unified_status_text(
                        &unified,
                        *breakdown,
                        *test_connectivity,
                    ))
                }
            }
            Commands::Validate => {
                let result = WorkspaceCommandService::validate(
                    self.api.as_ref(),
                    &self.workspace_root,
                    &self.frame_storage_path,
                )?;
                Ok(super::format_validate_result_text(&result))
            }
            Commands::Agent { command } => self.handle_agent_command(command),
            Commands::Provider { command } => {
                self.handle_provider_command(command, session_id)
            }
            Commands::Init { force, list } => self.handle_init(*force, *list),
            Commands::Context { command } => {
                self.handle_context_command(command, session_id)
            }
            Commands::Watch {
                debounce_ms,
                batch_window_ms,
                foreground: _,
            } => self.handle_watch(*debounce_ms, *batch_window_ms, session_id),
        }
    }

    fn handle_workspace_command(
        &self,
        command: &WorkspaceCommands,
    ) -> Result<String, ApiError> {
        match command {
            WorkspaceCommands::Status { format, breakdown } => {
                let registry = self.api.agent_registry().read();
                let request = WorkspaceStatusRequest {
                    workspace_root: self.workspace_root.clone(),
                    store_path: self.store_path.clone(),
                    include_breakdown: *breakdown,
                };
                let status = WorkspaceCommandService::status(
                    self.api.as_ref(),
                    &request,
                    &registry,
                )?;
                if *format == "json" {
                    serde_json::to_string_pretty(&status).map_err(|e| {
                        ApiError::StorageError(crate::error::StorageError::InvalidPath(
                            e.to_string(),
                        ))
                    })
                } else {
                    Ok(format_workspace_status_text(
                        &status,
                        request.include_breakdown,
                    ))
                }
            }
            WorkspaceCommands::Validate { format } => {
                let result = WorkspaceCommandService::validate(
                    self.api.as_ref(),
                    &self.workspace_root,
                    &self.frame_storage_path,
                )?;
                if *format == "json" {
                    serde_json::to_string_pretty(&result).map_err(|e| {
                        ApiError::StorageError(crate::error::StorageError::InvalidPath(
                            e.to_string(),
                        ))
                    })
                } else {
                    Ok(super::format_validate_result_text(&result))
                }
            }
            WorkspaceCommands::Ignore {
                path,
                dry_run,
                format,
            } => {
                let result = WorkspaceCommandService::ignore(
                    &self.workspace_root,
                    path.as_deref(),
                    *dry_run,
                )?;
                super::format_ignore_result(&result, format.as_str())
            }
            WorkspaceCommands::Delete {
                path,
                node,
                dry_run,
                no_ignore,
            } => WorkspaceCommandService::delete(
                self.api.as_ref(),
                &self.workspace_root,
                path.as_deref(),
                node.as_deref(),
                *dry_run,
                *no_ignore,
            ),
            WorkspaceCommands::Restore {
                path,
                node,
                dry_run,
            } => WorkspaceCommandService::restore(
                self.api.as_ref(),
                &self.workspace_root,
                path.as_deref(),
                node.as_deref(),
                *dry_run,
            ),
            WorkspaceCommands::Compact {
                ttl,
                all,
                keep_frames,
                dry_run,
            } => WorkspaceCommandService::compact(
                self.api.as_ref(),
                *ttl,
                *all,
                *keep_frames,
                *dry_run,
            ),
            WorkspaceCommands::ListDeleted { older_than, format } => {
                let result =
                    WorkspaceCommandService::list_deleted(self.api.as_ref(), *older_than)?;
                super::format_list_deleted_result(&result, format.as_str())
            }
        }
    }

    fn handle_agent_command(&self, command: &AgentCommands) -> Result<String, ApiError> {
        match command {
            AgentCommands::Status { format } => {
                self.handle_agent_status(format.clone())
            }
            AgentCommands::List { format, role } => {
                self.handle_agent_list(format.clone(), role.as_deref())
            }
            AgentCommands::Show {
                agent_id,
                format,
                include_prompt,
            } => self.handle_agent_show(agent_id, format.clone(), *include_prompt),
            AgentCommands::Validate {
                agent_id,
                all,
                verbose,
            } => self.handle_agent_validate(agent_id.as_deref(), *all, *verbose),
            AgentCommands::Create {
                agent_id,
                role,
                prompt_path,
                interactive,
                non_interactive,
            } => self.handle_agent_create(
                agent_id,
                role.as_deref(),
                prompt_path.as_deref(),
                *interactive,
                *non_interactive,
            ),
            AgentCommands::Edit {
                agent_id,
                prompt_path,
                role,
                editor,
            } => self.handle_agent_edit(
                agent_id,
                prompt_path.as_deref(),
                role.as_deref(),
                editor.as_deref(),
            ),
            AgentCommands::Prompt { command } => self.handle_agent_prompt_command(command),
            AgentCommands::Remove { agent_id, force } => {
                self.handle_agent_remove(agent_id, *force)
            }
        }
    }

    fn handle_agent_prompt_command(
        &self,
        command: &AgentPromptCommands,
    ) -> Result<String, ApiError> {
        match command {
            AgentPromptCommands::Show { agent_id } => {
                self.handle_agent_prompt_show(agent_id)
            }
            AgentPromptCommands::Edit { agent_id, editor } => {
                self.handle_agent_prompt_edit(agent_id, editor.as_deref())
            }
        }
    }

    fn handle_agent_list(
        &self,
        format: String,
        role_filter: Option<&str>,
    ) -> Result<String, ApiError> {
        let registry = self.api.agent_registry().read();
        let result = AgentCommandService::list(&registry, role_filter)?;
        match format.as_str() {
            "json" => Ok(super::format_agent_list_result_json(&result)),
            _ => Ok(super::format_agent_list_result_text(&result)),
        }
    }

    fn handle_agent_show(
        &self,
        agent_id: &str,
        format: String,
        include_prompt: bool,
    ) -> Result<String, ApiError> {
        let registry = self.api.agent_registry().read();
        let result =
            AgentCommandService::show(&registry, agent_id, include_prompt)?;
        match format.as_str() {
            "json" => Ok(super::format_agent_show_result_json(&result)),
            _ => Ok(super::format_agent_show_result_text(&result)),
        }
    }

    fn handle_agent_validate(
        &self,
        agent_id: Option<&str>,
        all: bool,
        verbose: bool,
    ) -> Result<String, ApiError> {
        let registry = self.api.agent_registry().read();
        if all {
            let result = AgentCommandService::validate_all(&registry)?;
            if result.results.is_empty() {
                return Ok("No agents found to validate.".to_string());
            }
            Ok(super::format_validation_results_all(&result.results, verbose))
        } else {
            let id = agent_id.ok_or_else(|| {
                ApiError::ConfigError(
                    "Agent ID required unless --all is specified".to_string(),
                )
            })?;
            let result = AgentCommandService::validate_single(&registry, id)?;
            Ok(super::format_validation_result(&result.result, verbose))
        }
    }

    fn handle_agent_create(
        &self,
        agent_id: &str,
        role: Option<&str>,
        prompt_path: Option<&str>,
        interactive: bool,
        non_interactive: bool,
    ) -> Result<String, ApiError> {
        let is_interactive = interactive || (!non_interactive && role.is_none());

        let (final_role, final_prompt_path) = if is_interactive {
            self.create_agent_interactive(agent_id)?
        } else {
            let role_str = role.ok_or_else(|| {
                ApiError::ConfigError(
                    "Role is required in non-interactive mode. Use --role <role>".to_string(),
                )
            })?;
            let parsed_role = AgentCommandService::parse_role(role_str)?;
            let prompt = if parsed_role != crate::agent::AgentRole::Reader {
                Some(
                    prompt_path
                        .ok_or_else(|| {
                            ApiError::ConfigError(
                                "Prompt path is required for Writer agents. Use --prompt-path <path>"
                                    .to_string(),
                            )
                        })?
                        .to_string(),
                )
            } else {
                None
            };
            (parsed_role, prompt)
        };

        let mut registry = self.api.agent_registry().write();
        let result = AgentCommandService::create(
            &mut registry,
            agent_id,
            final_role,
            final_prompt_path,
        )?;
        let mut output = format!(
            "Agent created: {}\nConfiguration file: {}",
            result.agent_id,
            result.config_path.display()
        );
        if let Some(prompt_path) = result.prompt_path {
            output.push_str(&format!("\nPrompt file: {}", prompt_path.display()));
        }
        Ok(output)
    }

    fn create_agent_interactive(
        &self,
        _agent_id: &str,
    ) -> Result<(crate::agent::AgentRole, Option<String>), ApiError> {
        use dialoguer::{Input, Select};

        let role_selection = Select::new()
            .with_prompt("Agent role")
            .items(&["Reader", "Writer"])
            .default(1)
            .interact()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        let role = match role_selection {
            0 => crate::agent::AgentRole::Reader,
            1 => crate::agent::AgentRole::Writer,
            _ => unreachable!(),
        };

        let prompt_path = if role != crate::agent::AgentRole::Reader {
            let path: String = Input::new()
                .with_prompt("Prompt file path")
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            Some(path)
        } else {
            None
        };

        Ok((role, prompt_path))
    }

    fn handle_agent_edit(
        &self,
        agent_id: &str,
        prompt_path: Option<&str>,
        role: Option<&str>,
        editor: Option<&str>,
    ) -> Result<String, ApiError> {
        if prompt_path.is_some() || role.is_some() {
            let mut registry = self.api.agent_registry().write();
            let _ = AgentCommandService::update_flags(
                &mut registry,
                agent_id,
                prompt_path,
                role,
            )?;
        } else {
            self.edit_agent_with_editor(agent_id, editor)?;
        }
        Ok(format!("Agent updated: {}", agent_id))
    }

    fn edit_agent_with_editor(
        &self,
        agent_id: &str,
        editor: Option<&str>,
    ) -> Result<(), ApiError> {
        let config_path = self
            .api
            .agent_registry()
            .read()
            .agent_config_path(agent_id)?;

        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read config: {}", e)))?;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("merkle-agent-{}.toml", agent_id));

        std::fs::write(&temp_path, content.as_bytes())
            .map_err(|e| ApiError::ConfigError(format!("Failed to write temp file: {}", e)))?;
        self.open_editor_for_path(&temp_path, editor)?;

        let edited_content = std::fs::read_to_string(&temp_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read edited file: {}", e)))?;

        let agent_config: crate::agent::AgentConfig = toml::from_str(&edited_content)
            .map_err(|e| ApiError::ConfigError(format!("Invalid config after editing: {}", e)))?;

        let mut registry = self.api.agent_registry().write();
        AgentCommandService::persist_edited_config(&mut registry, agent_id, agent_config)?;

        let _ = std::fs::remove_file(&temp_path);
        Ok(())
    }

    fn resolve_agent_prompt_file_path(
        &self,
        agent_id: &str,
    ) -> Result<PathBuf, ApiError> {
        let prompt_path = {
            let registry = self.api.agent_registry().read();
            let result = AgentCommandService::show(&registry, agent_id, false)?;
            result.prompt_path.ok_or_else(|| {
                ApiError::ConfigError(format!(
                    "Agent '{}' has no prompt file path configured",
                    agent_id
                ))
            })?
        };
        Ok(PathBuf::from(prompt_path))
    }

    fn handle_agent_prompt_show(&self, agent_id: &str) -> Result<String, ApiError> {
        let prompt_path = self.resolve_agent_prompt_file_path(agent_id)?;
        let content = std::fs::read_to_string(&prompt_path).map_err(|e| {
            ApiError::ConfigError(format!(
                "Failed to read prompt file {}: {}",
                prompt_path.display(),
                e
            ))
        })?;

        Ok(format!(
            "Agent: {}\nPrompt file: {}\n\n{}",
            agent_id,
            prompt_path.display(),
            content
        ))
    }

    fn handle_agent_prompt_edit(
        &self,
        agent_id: &str,
        editor: Option<&str>,
    ) -> Result<String, ApiError> {
        let prompt_path = self.resolve_agent_prompt_file_path(agent_id)?;
        if let Some(parent) = prompt_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ApiError::ConfigError(format!(
                    "Failed to create prompt directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        if !prompt_path.exists() {
            std::fs::write(&prompt_path, b"").map_err(|e| {
                ApiError::ConfigError(format!(
                    "Failed to create prompt file {}: {}",
                    prompt_path.display(),
                    e
                ))
            })?;
        }

        self.open_editor_for_path(&prompt_path, editor)?;
        Ok(format!("Prompt updated: {}", prompt_path.display()))
    }

    fn open_editor_for_path(
        &self,
        path: &std::path::Path,
        editor: Option<&str>,
    ) -> Result<(), ApiError> {
        use std::process::Command;

        let editor_cmd = match editor {
            Some(ed) => ed.to_string(),
            None => std::env::var("EDITOR").map_err(|_| {
                ApiError::ConfigError(
                    "No editor specified and $EDITOR not set. Use --editor <editor>".to_string(),
                )
            })?,
        };

        let status = Command::new(&editor_cmd)
            .arg(path)
            .status()
            .map_err(|e| ApiError::ConfigError(format!("Failed to open editor: {}", e)))?;

        if !status.success() {
            return Err(ApiError::ConfigError(
                "Editor exited with non-zero status".to_string(),
            ));
        }

        Ok(())
    }

    fn handle_agent_remove(&self, agent_id: &str, force: bool) -> Result<String, ApiError> {
        if !force {
            use dialoguer::Confirm;
            let confirmed = Confirm::new()
                .with_prompt(format!("Remove agent '{}'?", agent_id))
                .interact()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

            if !confirmed {
                return Ok("Removal cancelled".to_string());
            }
        }

        let mut registry = self.api.agent_registry().write();
        let result = AgentCommandService::remove(&mut registry, agent_id)?;
        Ok(format!(
            "Removed agent: {}\nConfiguration file deleted: {}",
            result.agent_id,
            result.config_path.display()
        ))
    }

    fn handle_agent_status(&self, format: String) -> Result<String, ApiError> {
        use crate::workspace::{
            format_agent_status_text, AgentStatusEntry, AgentStatusOutput,
        };

        let registry = self.api.agent_registry().read();
        let entries_result = AgentCommandService::status(&registry)?;
        let entries: Vec<AgentStatusEntry> = entries_result
            .into_iter()
            .map(|e| AgentStatusEntry {
                agent_id: e.agent_id,
                role: e.role,
                valid: e.valid,
                prompt_path_exists: e.prompt_path_exists,
            })
            .collect();
        let valid_count = entries.iter().filter(|e| e.valid).count();
        if format == "json" {
            Ok(serde_json::to_string_pretty(&AgentStatusOutput {
                agents: entries.clone(),
                total: entries.len(),
                valid_count,
            })
            .map_err(|e| {
                ApiError::StorageError(crate::error::StorageError::InvalidPath(
                    e.to_string(),
                ))
            })?)
        } else {
            Ok(format_agent_status_text(&entries))
        }
    }

    fn handle_provider_command(
        &self,
        command: &ProviderCommands,
        session_id: &str,
    ) -> Result<String, ApiError> {
        match command {
            ProviderCommands::Status {
                format,
                test_connectivity,
            } => self.handle_provider_status(format.clone(), *test_connectivity),
            ProviderCommands::List {
                format,
                type_filter,
            } => self.handle_provider_list(format.clone(), type_filter.as_deref()),
            ProviderCommands::Show {
                provider_name,
                format,
                include_credentials,
            } => self.handle_provider_show(
                provider_name,
                format.clone(),
                *include_credentials,
            ),
            ProviderCommands::Validate {
                provider_name,
                test_connectivity,
                check_model,
                verbose,
            } => self.handle_provider_validate(
                provider_name,
                *test_connectivity,
                *check_model,
                *verbose,
            ),
            ProviderCommands::Test {
                provider_name,
                model,
                timeout,
            } => self.handle_provider_test(
                provider_name,
                model.as_deref(),
                *timeout,
                session_id,
            ),
            ProviderCommands::Create {
                provider_name,
                type_,
                model,
                endpoint,
                api_key,
                interactive,
                non_interactive,
            } => self.handle_provider_create(
                provider_name,
                type_.as_deref(),
                model.as_deref(),
                endpoint.as_deref(),
                api_key.as_deref(),
                *interactive,
                *non_interactive,
            ),
            ProviderCommands::Edit {
                provider_name,
                model,
                endpoint,
                api_key,
                editor,
            } => self.handle_provider_edit(
                provider_name,
                model.as_deref(),
                endpoint.as_deref(),
                api_key.as_deref(),
                editor.as_deref(),
            ),
            ProviderCommands::Remove {
                provider_name,
                force,
            } => self.handle_provider_remove(provider_name, *force),
        }
    }

    fn handle_provider_list(
        &self,
        format: String,
        type_filter: Option<&str>,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;

        let registry = self.api.provider_registry().read();
        let result = ProviderCommandService::run_list(&registry, type_filter)?;
        match format.as_str() {
            "json" => Ok(super::format_provider_list_result_json(&result)),
            _ => Ok(super::format_provider_list_result_text(&result)),
        }
    }

    fn handle_provider_show(
        &self,
        provider_name: &str,
        format: String,
        include_credentials: bool,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;

        let registry = self.api.provider_registry().read();
        let result = ProviderCommandService::run_show(
            &registry,
            provider_name,
            include_credentials,
        )?;
        match format.as_str() {
            "json" => Ok(super::format_provider_show_result_json(&result)),
            _ => Ok(super::format_provider_show_result_text(&result)),
        }
    }

    fn handle_provider_validate(
        &self,
        provider_name: &str,
        test_connectivity: bool,
        check_model: bool,
        verbose: bool,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;

        let registry = self.api.provider_registry().read();
        let result = ProviderCommandService::run_validate(
            &registry,
            provider_name,
            test_connectivity,
            check_model,
        )?;
        Ok(super::format_provider_validation_result(&result, verbose))
    }

    fn handle_provider_status(
        &self,
        format: String,
        test_connectivity: bool,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;
        use crate::workspace::{
            format_provider_status_text, ProviderStatusEntry, ProviderStatusOutput,
        };

        let registry = self.api.provider_registry().read();
        let entries_result =
            ProviderCommandService::run_status(&registry, test_connectivity)?;
        let entries: Vec<ProviderStatusEntry> = entries_result
            .into_iter()
            .map(|e| ProviderStatusEntry {
                provider_name: e.provider_name,
                provider_type: e.provider_type,
                model: e.model,
                connectivity: e.connectivity,
            })
            .collect();
        if format == "json" {
            Ok(serde_json::to_string_pretty(&ProviderStatusOutput {
                providers: entries.clone(),
                total: entries.len(),
            })
            .map_err(|e| {
                ApiError::StorageError(crate::error::StorageError::InvalidPath(
                    e.to_string(),
                ))
            })?)
        } else {
            Ok(format_provider_status_text(&entries, test_connectivity))
        }
    }

    fn handle_provider_test(
        &self,
        provider_name: &str,
        model_override: Option<&str>,
        timeout: u64,
        session_id: &str,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;

        let registry = self.api.provider_registry().read();
        let model_for_event = model_override.unwrap_or_else(|| {
            registry
                .get(provider_name)
                .map(|p| p.model.as_str())
                .unwrap_or("")
        });
        self.progress.emit_event_best_effort(
            session_id,
            "provider_request_sent",
            json!(ProviderLifecycleEventData {
                node_id: "provider_test".to_string(),
                agent_id: "provider_test".to_string(),
                provider_name: provider_name.to_string(),
                frame_type: model_for_event.to_string(),
                duration_ms: None,
                error: None,
                retry_count: Some(0),
            }),
        );
        let start = std::time::Instant::now();
        let result = ProviderCommandService::run_test(
            &registry,
            provider_name,
            model_override,
            timeout,
        )?;
        let elapsed_ms = start.elapsed().as_millis();
        if result.connectivity_ok {
            self.progress.emit_event_best_effort(
                session_id,
                "provider_response_received",
                json!(ProviderLifecycleEventData {
                    node_id: "provider_test".to_string(),
                    agent_id: "provider_test".to_string(),
                    provider_name: result.provider_name.clone(),
                    frame_type: result.model_checked.clone(),
                    duration_ms: Some(elapsed_ms),
                    error: None,
                    retry_count: Some(0),
                }),
            );
        } else {
            self.progress.emit_event_best_effort(
                session_id,
                "provider_request_failed",
                json!(ProviderLifecycleEventData {
                    node_id: "provider_test".to_string(),
                    agent_id: "provider_test".to_string(),
                    provider_name: result.provider_name.clone(),
                    frame_type: result.model_checked.clone(),
                    duration_ms: Some(elapsed_ms),
                    error: result.error_message.clone(),
                    retry_count: Some(0),
                }),
            );
        }
        Ok(super::format_provider_test_result(&result, Some(elapsed_ms)))
    }

    fn handle_provider_create(
        &self,
        provider_name: &str,
        type_: Option<&str>,
        model: Option<&str>,
        endpoint: Option<&str>,
        api_key: Option<&str>,
        interactive: bool,
        non_interactive: bool,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;

        let is_interactive = interactive || (!non_interactive && type_.is_none());

        let (provider_type, final_model, final_endpoint, final_api_key, default_options) =
            if is_interactive {
                self.create_provider_interactive()?
            } else {
                let type_str = type_.ok_or_else(|| {
                    ApiError::ConfigError(
                        "Provider type is required in non-interactive mode. Use --type <type>"
                            .to_string(),
                    )
                })?;

                let parsed_type =
                    ProviderCommandService::parse_provider_type(type_str)?;

                let model_name = model.ok_or_else(|| {
                    ApiError::ConfigError(
                        "Model is required in non-interactive mode. Use --model <model>"
                            .to_string(),
                    )
                })?;

                (
                    parsed_type,
                    model_name.to_string(),
                    endpoint.map(String::from),
                    api_key.map(String::from),
                    crate::provider::CompletionOptions::default(),
                )
            };

        let mut registry = self.api.provider_registry().write();
        let result = ProviderCommandService::run_create(
            &mut registry,
            provider_name,
            provider_type,
            final_model,
            final_endpoint,
            final_api_key,
            default_options,
        )?;
        Ok(format!(
            "Provider created: {}\nConfiguration file: {}",
            result.provider_name,
            result.config_path.display()
        ))
    }

    fn create_provider_interactive(
        &self,
    ) -> Result<
        (
            crate::config::ProviderType,
            String,
            Option<String>,
            Option<String>,
            crate::provider::CompletionOptions,
        ),
        ApiError,
    > {
        use crate::provider::commands::ProviderCommandService;
        use dialoguer::{Input, Select};

        let type_selection = Select::new()
            .with_prompt("Provider type")
            .items(&["openai", "anthropic", "ollama", "local"])
            .default(0)
            .interact()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        let provider_type = match type_selection {
            0 => crate::config::ProviderType::OpenAI,
            1 => crate::config::ProviderType::Anthropic,
            2 => crate::config::ProviderType::Ollama,
            3 => crate::config::ProviderType::LocalCustom,
            _ => unreachable!(),
        };

        let model: String = Input::new()
            .with_prompt("Model name")
            .interact_text()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        let default_endpoint =
            ProviderCommandService::default_endpoint(provider_type);

        let endpoint = if provider_type == crate::config::ProviderType::LocalCustom {
            Some(
                Input::new()
                    .with_prompt("Endpoint URL (required)")
                    .interact_text()
                    .map_err(|e| {
                        ApiError::ConfigError(format!("Failed to get user input: {}", e))
                    })?,
            )
        } else if let Some(default) = default_endpoint {
            let input: String = Input::new()
                .with_prompt(format!("Endpoint URL (optional, default: {})", default))
                .default(default)
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;
            Some(input)
        } else {
            None
        };

        let env_var = ProviderCommandService::required_api_key_env_var(provider_type)
            .unwrap_or("");

        let api_key = if provider_type == crate::config::ProviderType::Ollama
            || provider_type == crate::config::ProviderType::LocalCustom
        {
            None
        } else {
            let prompt = if !env_var.is_empty() {
                format!(
                    "API key (optional, will use {} env var if not set)",
                    env_var
                )
            } else {
                "API key (optional)".to_string()
            };

            let input: String = Input::new()
                .with_prompt(prompt)
                .allow_empty(true)
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

            if input.is_empty() {
                None
            } else {
                Some(input)
            }
        };

        let temperature: f32 = Input::new()
            .with_prompt("Default temperature (0.0-2.0, default: 1.0)")
            .default(1.0)
            .interact_text()
            .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

        let max_tokens: Option<u32> = {
            let input: String = Input::new()
                .with_prompt("Default max tokens (optional, press Enter to skip)")
                .allow_empty(true)
                .interact_text()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

            if input.is_empty() {
                None
            } else {
                input.parse().ok()
            }
        };

        let default_options = crate::provider::CompletionOptions {
            temperature: Some(temperature),
            max_tokens,
            ..Default::default()
        };

        Ok((provider_type, model, endpoint, api_key, default_options))
    }

    fn handle_provider_edit(
        &self,
        provider_name: &str,
        model: Option<&str>,
        endpoint: Option<&str>,
        api_key: Option<&str>,
        editor: Option<&str>,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;

        if model.is_some() || endpoint.is_some() || api_key.is_some() {
            let mut registry = self.api.provider_registry().write();
            ProviderCommandService::run_update_flags(
                &mut registry,
                provider_name,
                model,
                endpoint,
                api_key,
            )?;
        } else {
            self.edit_provider_with_editor(provider_name, editor)?;
        }
        Ok(format!("Provider updated: {}", provider_name))
    }

    fn edit_provider_with_editor(
        &self,
        provider_name: &str,
        editor: Option<&str>,
    ) -> Result<(), ApiError> {
        use std::process::Command;
        use crate::provider::commands::ProviderCommandService;

        let config_path = {
            let registry = self.api.provider_registry().read();
            ProviderCommandService::provider_config_path(&registry, provider_name)?
        };

        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read config: {}", e)))?;

        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("merkle-provider-{}.toml", provider_name));

        std::fs::write(&temp_path, content.as_bytes())
            .map_err(|e| ApiError::ConfigError(format!("Failed to write temp file: {}", e)))?;

        let editor_cmd = match editor {
            Some(ed) => ed.to_string(),
            None => std::env::var("EDITOR").map_err(|_| {
                ApiError::ConfigError(
                    "No editor specified and $EDITOR not set. Use --editor <editor>".to_string(),
                )
            })?,
        };

        let status = Command::new(&editor_cmd)
            .arg(&temp_path)
            .status()
            .map_err(|e| ApiError::ConfigError(format!("Failed to open editor: {}", e)))?;

        if !status.success() {
            return Err(ApiError::ConfigError(
                "Editor exited with non-zero status".to_string(),
            ));
        }

        let edited_content = std::fs::read_to_string(&temp_path)
            .map_err(|e| ApiError::ConfigError(format!("Failed to read edited file: {}", e)))?;

        let provider_config: crate::config::ProviderConfig = toml::from_str(&edited_content)
            .map_err(|e| ApiError::ConfigError(format!("Invalid config after editing: {}", e)))?;

        if let Some(ref config_name) = provider_config.provider_name {
            if config_name != provider_name {
                return Err(ApiError::ConfigError(format!(
                    "Provider name mismatch: config has '{}' but expected '{}'",
                    config_name, provider_name
                )));
            }
        }

        {
            let mut registry = self.api.provider_registry().write();
            ProviderCommandService::persist_provider_config(
                &mut registry,
                provider_name,
                &provider_config,
            )?;
        }

        let _ = std::fs::remove_file(&temp_path);
        Ok(())
    }

    fn handle_provider_remove(
        &self,
        provider_name: &str,
        force: bool,
    ) -> Result<String, ApiError> {
        use crate::provider::commands::ProviderCommandService;

        {
            let registry = self.api.provider_registry().read();
            let provider = registry.get_or_error(provider_name)?;
            if provider.provider_type == crate::provider::ProviderType::OpenAI
                || provider.provider_type == crate::provider::ProviderType::Anthropic
            {
                eprintln!(
                    "Warning: Provider '{}' may be in use by agents.",
                    provider_name
                );
            }
        }

        if !force {
            use dialoguer::Confirm;
            let confirmed = Confirm::new()
                .with_prompt(format!("Remove provider '{}'?", provider_name))
                .interact()
                .map_err(|e| ApiError::ConfigError(format!("Failed to get user input: {}", e)))?;

            if !confirmed {
                return Ok("Removal cancelled".to_string());
            }
        }

        let mut registry = self.api.provider_registry().write();
        let result = ProviderCommandService::run_remove(&mut registry, provider_name)?;
        Ok(format!(
            "Removed provider: {}\nConfiguration file deleted: {}",
            result.provider_name,
            result.config_path.display()
        ))
    }

    fn handle_init(&self, force: bool, list: bool) -> Result<String, ApiError> {
        if list {
            let preview = crate::init::list_initialization()?;
            Ok(super::format_init_preview(&preview))
        } else {
            let summary = crate::init::initialize_all(force)?;
            Ok(super::format_init_summary(&summary, force))
        }
    }

    fn handle_context_command(
        &self,
        command: &ContextCommands,
        session_id: &str,
    ) -> Result<String, ApiError> {
        match command {
            ContextCommands::Generate {
                node,
                path,
                path_positional,
                agent,
                provider,
                frame_type,
                force,
                no_recursive,
            } => {
                let path_merged = path.as_ref().or(path_positional.as_ref());
                let request = GenerateRequest {
                    node: node.clone(),
                    path: path_merged.cloned(),
                    agent: agent.clone(),
                    provider: provider.clone(),
                    frame_type: frame_type.clone(),
                    force: *force,
                    no_recursive: *no_recursive,
                };
                run_generate(
                    Arc::clone(&self.api),
                    &self.workspace_root,
                    Some(Arc::clone(&self.progress)),
                    Some(session_id),
                    &request,
                )
            }
            ContextCommands::Regenerate {
                node,
                path,
                path_positional,
                agent,
                provider,
                frame_type,
                recursive,
            } => {
                let path_merged = path.as_ref().or(path_positional.as_ref());
                let request = GenerateRequest {
                    node: node.clone(),
                    path: path_merged.cloned(),
                    agent: agent.clone(),
                    provider: provider.clone(),
                    frame_type: frame_type.clone(),
                    force: true,
                    no_recursive: !*recursive,
                };
                run_generate(
                    Arc::clone(&self.api),
                    &self.workspace_root,
                    Some(Arc::clone(&self.progress)),
                    Some(session_id),
                    &request,
                )
            }
            ContextCommands::Get {
                node,
                path,
                agent,
                frame_type,
                max_frames,
                ordering,
                combine,
                separator,
                format,
                include_metadata,
                include_deleted,
            } => {
                let context = get_node_for_cli(
                    self.api.as_ref(),
                    &self.workspace_root,
                    node.as_deref(),
                    path.as_ref().map(|p| p.as_path()),
                    agent.as_deref(),
                    frame_type.as_deref(),
                    *max_frames,
                    ordering,
                    *include_deleted,
                )?;
                let formatted = match format.as_str() {
                    "text" => super::format_context_text_output(
                        &context,
                        *include_metadata,
                        *combine,
                        separator,
                        *include_deleted,
                    ),
                    "json" => super::format_context_json_output(
                        &context,
                        *include_metadata,
                        *include_deleted,
                    ),
                    _ => Err(ApiError::ConfigError(format!(
                        "Invalid format: '{}'. Must be 'text' or 'json'.",
                        format
                    ))),
                }?;
                self.progress.emit_event_best_effort(
                    session_id,
                    "context_read_summary",
                    json!({
                        "node_id": hex::encode(context.node_id),
                        "frame_count": context.frames.len(),
                        "max_frames": max_frames,
                        "ordering": ordering,
                        "combine": combine,
                        "format": format
                    }),
                );
                Ok(formatted)
            }
        }
    }

    fn handle_watch(
        &self,
        debounce_ms: u64,
        batch_window_ms: u64,
        session_id: &str,
    ) -> Result<String, ApiError> {
        let config = if let Some(ref config_path) = self.config_path {
            ConfigLoader::load_from_file(config_path).map_err(|e| {
                ApiError::ConfigError(format!(
                    "Failed to load config from {}: {}",
                    config_path.display(),
                    e
                ))
            })?
        } else {
            ConfigLoader::load(&self.workspace_root).map_err(|e| {
                ApiError::ConfigError(format!("Failed to load config: {}", e))
            })?
        };

        {
            let mut registry = self.api.agent_registry().write();
            registry.load_from_config(&config).map_err(|e| {
                ApiError::ConfigError(format!("Failed to load agents from config: {}", e))
            })?;
        }

        let ignore_patterns = ignore::load_ignore_patterns(&self.workspace_root)
            .unwrap_or_else(|_| WalkerConfig::default().ignore_patterns);

        let mut watch_config = WatchConfig::default();
        watch_config.workspace_root = self.workspace_root.clone();
        watch_config.debounce_ms = debounce_ms;
        watch_config.batch_window_ms = batch_window_ms;
        watch_config.ignore_patterns = ignore_patterns;
        watch_config.session_id = Some(session_id.to_string());
        watch_config.progress = Some(self.progress.clone());

        let daemon = WatchDaemon::new(self.api.clone(), watch_config)?;
        tracing::info!("Starting watch mode daemon");
        daemon.start()?;
        Ok("Watch daemon stopped".to_string())
    }

    fn emit_command_summary(
        &self,
        session_id: &str,
        command: &Commands,
        result: Result<&String, &ApiError>,
        duration_ms: u128,
    ) {
        let ok = result.is_ok();
        let error = result.as_ref().err().map(|err| err.to_string());
        let (message, output_chars, error_chars, truncated) = match result {
            Ok(output) => (None, Some(output.chars().count()), None, None),
            Err(_) => {
                let error_text = error
                    .clone()
                    .unwrap_or_else(|| "command failed".to_string());
                let error_chars = error_text.chars().count();
                let (preview, was_truncated) = truncate_for_summary(&error_text);
                (Some(preview), None, Some(error_chars), Some(was_truncated))
            }
        };
        let descriptor = summary_descriptor(command);
        emit_command_summary(
            self.progress.as_ref(),
            session_id,
            &command_name(command),
            &descriptor,
            ok,
            duration_ms,
            error.as_deref(),
            message,
            output_chars,
            error_chars,
            truncated,
        );
    }
}
