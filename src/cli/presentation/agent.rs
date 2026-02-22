//! Agent command presentation: list, show, validation text/json.

use crate::agent::{AgentListResult, AgentRole, AgentShowResult, ValidationResult};
use serde_json::json;

pub fn format_agent_list_result_text(result: &AgentListResult) -> String {
    let agents = &result.agents;
    if agents.is_empty() {
        return "No agents found.\n\nNote: Agents are provider-agnostic. Providers are selected at runtime.".to_string();
    }
    let mut output = String::from("Available Agents:\n");
    for item in agents {
        let role_str = match item.role {
            AgentRole::Reader => "Reader",
            AgentRole::Writer => "Writer",
        };
        output.push_str(&format!("  {:<20} {:<10}\n", item.agent_id, role_str));
    }
    output.push_str(&format!("\nTotal: {} agent(s)\n\nNote: Agents are provider-agnostic. Providers are selected at runtime.", agents.len()));
    output
}

pub fn format_agent_list_result_json(result: &AgentListResult) -> String {
    let agent_list: Vec<_> = result
        .agents
        .iter()
        .map(|item| {
            json!({
                "agent_id": item.agent_id,
                "role": match item.role {
                    AgentRole::Reader => "Reader",
                    AgentRole::Writer => "Writer",
                },
            })
        })
        .collect();
    let out = json!({ "agents": agent_list, "total": result.agents.len() });
    serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
}

pub fn format_agent_show_result_text(result: &AgentShowResult) -> String {
    let role_str = match result.role {
        AgentRole::Reader => "Reader",
        AgentRole::Writer => "Writer",
    };
    let mut output = format!("Agent: {}\n", result.agent_id);
    output.push_str(&format!("Role: {}\n", role_str));
    output.push_str("Prompt: [see config]\n");
    if let Some(prompt) = &result.prompt_content {
        output.push_str("\nPrompt Content:\n");
        output.push_str(prompt);
    }
    output
}

pub fn format_agent_show_result_json(result: &AgentShowResult) -> String {
    let role_str = match result.role {
        AgentRole::Reader => "Reader",
        AgentRole::Writer => "Writer",
    };
    let mut out = json!({
        "agent_id": result.agent_id,
        "role": role_str,
    });
    if let Some(p) = &result.prompt_content {
        out["prompt_content"] = json!(p);
    }
    serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
}

pub fn format_validation_result(result: &ValidationResult, verbose: bool) -> String {
    let mut output = format!("Validating agent: {}\n\n", result.agent_id);

    if result.errors.is_empty() && result.checks.iter().all(|(_, passed)| *passed) {
        output.push_str("✓ All validation checks passed\n\n");
    } else {
        for (description, passed) in &result.checks {
            if *passed {
                output.push_str(&format!("✓ {}\n", description));
            } else {
                output.push_str(&format!("✗ {}\n", description));
            }
        }
        if !result.errors.is_empty() {
            output.push_str("\n");
            for error in &result.errors {
                output.push_str(&format!("✗ {}\n", error));
            }
        }
        output.push_str("\n");
    }

    if verbose {
        output.push_str(&format!(
            "Validation summary: {}/{} checks passed\n",
            result.passed_checks(),
            result.total_checks()
        ));
        if !result.errors.is_empty() {
            output.push_str(&format!("Errors found: {}\n", result.errors.len()));
        }
    } else if result.is_valid() {
        output.push_str(&format!(
            "Validation passed: {}/{} checks\n",
            result.passed_checks(),
            result.total_checks()
        ));
    } else {
        output.push_str(&format!(
            "Validation failed: {} error(s) found\n",
            result.errors.len()
        ));
    }
    output
}

pub fn format_validation_results_all(
    results: &[(String, ValidationResult)],
    verbose: bool,
) -> String {
    let mut output = String::from("Validating all agents:\n\n");
    let mut valid_count = 0;
    let mut invalid_count = 0;

    for (agent_id, result) in results {
        if result.is_valid() {
            valid_count += 1;
            if verbose {
                output.push_str(&format!(
                    "✓ {}: All checks passed ({}/{} checks)\n",
                    agent_id,
                    result.passed_checks(),
                    result.total_checks()
                ));
            } else {
                output.push_str(&format!("✓ {}: Valid\n", agent_id));
            }
        } else {
            invalid_count += 1;
            output.push_str(&format!("✗ {}: Validation failed\n", agent_id));
            if verbose {
                for (description, passed) in &result.checks {
                    if !passed {
                        output.push_str(&format!("  ✗ {}\n", description));
                    }
                }
                for error in &result.errors {
                    output.push_str(&format!("  ✗ {}\n", error));
                }
            }
        }
    }
    output.push_str(&format!(
        "\nSummary: {} valid, {} invalid (out of {} total)\n",
        valid_count,
        invalid_count,
        results.len()
    ));
    output
}
