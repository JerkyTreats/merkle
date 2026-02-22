//! CLI domain: parse, route, help, output, and presentation only.
//! No domain orchestration; single route table dispatches to domain services.

mod help;
mod output;
mod parse;
mod presentation;
mod route;

pub use help::{command_name, summary_descriptor};
pub use output::map_error;
pub use parse::{
    AgentCommands, Cli, Commands, ContextCommands, ProviderCommands, WorkspaceCommands,
};
pub use presentation::{
    format_context_json_output, format_context_text_output,
    format_ignore_result, format_init_preview, format_init_summary,
    format_list_deleted_result, format_validate_result_text,
    format_agent_list_result_json, format_agent_list_result_text,
    format_agent_show_result_json, format_agent_show_result_text,
    format_validation_result, format_validation_results_all,
    format_provider_list_result_json, format_provider_list_result_text,
    format_provider_show_result_json, format_provider_show_result_text,
    format_provider_test_result, format_provider_validation_result,
};
pub use route::RunContext;
