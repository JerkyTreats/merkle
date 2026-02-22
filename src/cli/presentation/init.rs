//! Init command presentation: preview and summary formatters.

use crate::agent::AgentStorage;
use crate::init::{InitPreview, InitSummary};
use std::path::PathBuf;

pub fn format_init_preview(preview: &InitPreview) -> String {
    let mut output = String::from("Initialization Preview:\n\n");

    if !preview.prompts.is_empty() {
        output.push_str("Would create prompts:\n");
        for prompt in &preview.prompts {
            output.push_str(&format!("  - {}\n", prompt));
        }
        output.push('\n');
    }

    if !preview.agents.is_empty() {
        output.push_str("Would create agents:\n");
        for agent in &preview.agents {
            output.push_str(&format!("  - {}.toml\n", agent));
        }
        output.push('\n');
    }

    if preview.prompts.is_empty() && preview.agents.is_empty() {
        output.push_str("All default agents and prompts already exist.\n");
    } else {
        output.push_str("Run 'merkle init' to perform initialization.\n");
    }
    output
}

pub fn format_init_summary(summary: &InitSummary, force: bool) -> String {
    let mut output = String::from("Initializing Merkle configuration...\n\n");

    if !summary.prompts.created.is_empty() || !summary.prompts.skipped.is_empty() {
        let prompts_dir = crate::config::xdg::prompts_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/.config/merkle/prompts/".to_string());
        output.push_str(&format!("Created prompts directory: {}\n", prompts_dir));
        for prompt in &summary.prompts.created {
            if force {
                output.push_str(&format!("  ✓ {} (overwritten)\n", prompt));
            } else {
                output.push_str(&format!("  ✓ {}\n", prompt));
            }
        }
        for prompt in &summary.prompts.skipped {
            output.push_str(&format!("  ⊘ {} (already exists, skipped)\n", prompt));
        }
        output.push('\n');
    }

    if !summary.agents.created.is_empty() || !summary.agents.skipped.is_empty() {
        let agents_dir = crate::agent::XdgAgentStorage::new()
            .agents_dir()
            .map(|p: PathBuf| p.display().to_string())
            .unwrap_or_else(|_| "~/.config/merkle/agents/".to_string());
        output.push_str(&format!("Created agents directory: {}\n", agents_dir));
        for agent in &summary.agents.created {
            let role_str = match agent.as_str() {
                "reader" => "Reader",
                "code-analyzer" => "Writer",
                "docs-writer" => "Writer",
                _ => "Unknown",
            };
            if force {
                output.push_str(&format!(
                    "  ✓ {}.toml ({}) (overwritten)\n",
                    agent, role_str
                ));
            } else {
                output.push_str(&format!("  ✓ {}.toml ({})\n", agent, role_str));
            }
        }
        for agent in &summary.agents.skipped {
            let role_str = match agent.as_str() {
                "reader" => "Reader",
                "code-analyzer" => "Writer",
                "docs-writer" => "Writer",
                _ => "Unknown",
            };
            output.push_str(&format!(
                "  ⊘ {}.toml ({}) (already exists, skipped)\n",
                agent, role_str
            ));
        }
        output.push('\n');
    }

    if !summary.prompts.errors.is_empty() || !summary.agents.errors.is_empty() {
        output.push_str("Errors:\n");
        for error in &summary.prompts.errors {
            output.push_str(&format!("  ✗ {}\n", error));
        }
        for error in &summary.agents.errors {
            output.push_str(&format!("  ✗ {}\n", error));
        }
        output.push('\n');
    }

    let all_valid = summary
        .validation
        .results
        .iter()
        .all(|(_, is_valid, _)| *is_valid);
    if all_valid {
        output.push_str("Validation:\n");
        output.push_str("  ✓ All agents validated successfully\n\n");
    } else {
        output.push_str("Validation:\n");
        for (agent_id, is_valid, errors) in &summary.validation.results {
            if *is_valid {
                output.push_str(&format!("  ✓ {} validated\n", agent_id));
            } else {
                output.push_str(&format!("  ✗ {} validation failed:\n", agent_id));
                for error in errors {
                    output.push_str(&format!("    - {}\n", error));
                }
            }
        }
        output.push('\n');
    }

    if summary.prompts.created.is_empty() && summary.agents.created.is_empty() && !force {
        output.push_str("All default agents already exist. Use --force to re-initialize.\n");
    } else {
        output.push_str("Initialization complete! You can now use:\n");
        output.push_str("  - merkle agent list          # List all agents\n");
        output.push_str("  - merkle agent show <id>     # View agent details\n");
        output.push_str("  - merkle context generate    # Generate context frames\n");
    }
    output
}
