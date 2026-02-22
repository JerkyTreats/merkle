//! CLI help and command-name contract for telemetry and routing.

use crate::cli::parse::{
    AgentCommands, Commands, ContextCommands, ProviderCommands, WorkspaceCommands,
};
use crate::telemetry::emission::SummaryCommandDescriptor;

/// Command name string for session and telemetry (e.g. "workspace.status", "agent.list").
pub fn command_name(command: &Commands) -> String {
    match command {
        Commands::Scan { .. } => "scan".to_string(),
        Commands::Workspace { command } => format!("workspace.{}", workspace_command_name(command)),
        Commands::Status { .. } => "status".to_string(),
        Commands::Validate => "validate".to_string(),
        Commands::Watch { .. } => "watch".to_string(),
        Commands::Agent { command } => format!("agent.{}", agent_command_name(command)),
        Commands::Provider { command } => format!("provider.{}", provider_command_name(command)),
        Commands::Init { .. } => "init".to_string(),
        Commands::Context { command } => format!("context.{}", context_command_name(command)),
    }
}

pub fn workspace_command_name(command: &WorkspaceCommands) -> &'static str {
    match command {
        WorkspaceCommands::Status { .. } => "status",
        WorkspaceCommands::Validate { .. } => "validate",
        WorkspaceCommands::Ignore { .. } => "ignore",
        WorkspaceCommands::Delete { .. } => "delete",
        WorkspaceCommands::Restore { .. } => "restore",
        WorkspaceCommands::Compact { .. } => "compact",
        WorkspaceCommands::ListDeleted { .. } => "list_deleted",
    }
}

pub fn context_command_name(command: &ContextCommands) -> &'static str {
    match command {
        ContextCommands::Generate { .. } => "generate",
        ContextCommands::Get { .. } => "get",
    }
}

pub fn provider_command_name(command: &ProviderCommands) -> &'static str {
    match command {
        ProviderCommands::Status { .. } => "status",
        ProviderCommands::List { .. } => "list",
        ProviderCommands::Show { .. } => "show",
        ProviderCommands::Create { .. } => "create",
        ProviderCommands::Edit { .. } => "edit",
        ProviderCommands::Remove { .. } => "remove",
        ProviderCommands::Validate { .. } => "validate",
        ProviderCommands::Test { .. } => "test",
    }
}

pub fn agent_command_name(command: &AgentCommands) -> &'static str {
    match command {
        AgentCommands::Status { .. } => "status",
        AgentCommands::List { .. } => "list",
        AgentCommands::Show { .. } => "show",
        AgentCommands::Create { .. } => "create",
        AgentCommands::Edit { .. } => "edit",
        AgentCommands::Remove { .. } => "remove",
        AgentCommands::Validate { .. } => "validate",
    }
}

/// Summary descriptor for telemetry emission. CLI boundary only; telemetry never imports Commands.
pub fn summary_descriptor(command: &Commands) -> SummaryCommandDescriptor {
    match command {
        Commands::Workspace { command } => match command {
            WorkspaceCommands::Status { format, breakdown } => {
                SummaryCommandDescriptor::WorkspaceStatus {
                    format: format.clone(),
                    breakdown: *breakdown,
                }
            }
            WorkspaceCommands::Validate { format } => SummaryCommandDescriptor::WorkspaceValidate {
                format: format.clone(),
            },
            WorkspaceCommands::Delete {
                path,
                node,
                dry_run,
                no_ignore,
            } => SummaryCommandDescriptor::WorkspaceDelete {
                target_path: path.is_some(),
                target_node: node.is_some(),
                dry_run: *dry_run,
                no_ignore: *no_ignore,
            },
            WorkspaceCommands::Restore {
                path,
                node,
                dry_run,
            } => SummaryCommandDescriptor::WorkspaceRestore {
                target_path: path.is_some(),
                target_node: node.is_some(),
                dry_run: *dry_run,
            },
            WorkspaceCommands::Compact {
                ttl,
                all,
                keep_frames,
                dry_run,
            } => SummaryCommandDescriptor::WorkspaceCompact {
                ttl_days: *ttl,
                all: *all,
                keep_frames: *keep_frames,
                dry_run: *dry_run,
            },
            WorkspaceCommands::ListDeleted { older_than, format } => {
                SummaryCommandDescriptor::WorkspaceListDeleted {
                    older_than_days: *older_than,
                    format: format.clone(),
                }
            }
            WorkspaceCommands::Ignore {
                path,
                dry_run,
                format,
            } => SummaryCommandDescriptor::WorkspaceIgnore {
                has_path: path.is_some(),
                dry_run: *dry_run,
                format: format.clone(),
            },
        },
        Commands::Status {
            format,
            workspace_only,
            agents_only,
            providers_only,
            breakdown,
            test_connectivity,
        } => {
            let include_all = !*workspace_only && !*agents_only && !*providers_only;
            SummaryCommandDescriptor::StatusUnified {
                format: format.clone(),
                include_workspace: include_all || *workspace_only,
                include_agents: include_all || *agents_only,
                include_providers: include_all || *providers_only,
                breakdown: *breakdown,
                test_connectivity: *test_connectivity,
            }
        }
        Commands::Validate => SummaryCommandDescriptor::ValidateWorkspace,
        Commands::Agent { command } => SummaryCommandDescriptor::AgentAction {
            action: agent_command_name(command).to_string(),
            mutation: matches!(
                command,
                AgentCommands::Create { .. }
                    | AgentCommands::Edit { .. }
                    | AgentCommands::Remove { .. }
            ),
        },
        Commands::Provider { command } => SummaryCommandDescriptor::ProviderAction {
            action: provider_command_name(command).to_string(),
            mutation: matches!(
                command,
                ProviderCommands::Create { .. }
                    | ProviderCommands::Edit { .. }
                    | ProviderCommands::Remove { .. }
            ),
        },
        Commands::Init { force, list } => SummaryCommandDescriptor::Init {
            force: *force,
            list_only: *list,
        },
        _ => SummaryCommandDescriptor::None,
    }
}
