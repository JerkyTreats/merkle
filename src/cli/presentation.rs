//! CLI presentation: text and json formatters per command family.

mod agent;
mod context;
mod init;
mod provider;
mod shared;

pub use agent::{
    format_agent_list_result_json, format_agent_list_result_text,
    format_agent_show_result_json, format_agent_show_result_text,
    format_validation_result, format_validation_results_all,
};
pub use context::{format_context_json_output, format_context_text_output};
pub use init::{format_init_preview, format_init_summary};
pub use provider::{
    format_provider_list_result_json, format_provider_list_result_text,
    format_provider_show_result_json, format_provider_show_result_text,
    format_provider_test_result, format_provider_validation_result,
};
pub use shared::{
    format_ignore_result, format_list_deleted_result, format_validate_result_text,
};
