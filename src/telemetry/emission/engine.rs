//! Best-effort emission of typed summary and command_summary events.

use crate::telemetry::emission::summary_mapper::SummaryCommandDescriptor;
use crate::telemetry::emission::summary_mapper::{
    command_summary_data, truncate_summary_message, typed_summary_event,
    COMMAND_SUMMARY_MESSAGE_MAX_CHARS,
};
use crate::telemetry::sessions::service::ProgressRuntime;
use serde_json::json;

/// Emit typed summary event if any and always command_summary. Best-effort; logs on failure.
pub fn emit_command_summary(
    runtime: &ProgressRuntime,
    session_id: &str,
    command_name: &str,
    descriptor: &SummaryCommandDescriptor,
    ok: bool,
    duration_ms: u128,
    error: Option<&str>,
    message: Option<String>,
    output_chars: Option<usize>,
    error_chars: Option<usize>,
    truncated: Option<bool>,
) {
    if let Some((event_type, data)) = typed_summary_event(descriptor, ok, duration_ms, error) {
        runtime.emit_event_best_effort(session_id, event_type, data);
    }
    let data = command_summary_data(
        command_name,
        ok,
        duration_ms,
        message,
        output_chars,
        error_chars,
        truncated,
    );
    runtime.emit_event_best_effort(session_id, "command_summary", json!(data));
}

/// Truncate error message for command_summary. Returns (message, was_truncated).
pub fn truncate_for_summary(value: &str) -> (String, bool) {
    truncate_summary_message(value, COMMAND_SUMMARY_MESSAGE_MAX_CHARS)
}
