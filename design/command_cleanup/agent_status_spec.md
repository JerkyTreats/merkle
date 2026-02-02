# Agent Status Command Specification

## Overview

This document specifies `merkle agent status`, a summary view of all agents: count, validation state, and prompt path existence. It complements `merkle agent list` (full list) and `merkle agent show` (single agent); status is a quick health/readiness check. For agent form and registry behavior, see design/context/agents/agent_cli_spec.md as reference only.

## Command structure

**Syntax**

```
merkle agent status [--format text|json]
```

**Options**

- `--format <text|json>`: Output format (default: text)

## Purpose

Provide a quick health/readiness summary: how many agents exist, how many pass validation, and whether each agent's prompt path exists. Scripts and users can run status before context generation to confirm agents are configured correctly.

## Output

### Text format

- Section heading (styled via the display stack; see workspace_status_requirements.md).
- Table via comfy-table with columns: **Agent** | **Role** | **Valid** | **Prompt** (path exists / missing).
- Optional summary line: "Total: N agents, M valid."

**Example**

```
Agents

  | Agent           | Role   | Valid | Prompt   |
  |-----------------|--------|-------|----------|
  | code-analyzer   | Writer | yes   | exists   |
  | docs-writer     | Writer | yes   | exists   |
  | synthesis-agent | Synthesis | no | missing  |

Total: 3 agents, 2 valid.
```

### JSON format

Structured object:

- `agents`: array of objects with `agent_id`, `role`, `valid` (bool), `prompt_path_exists` (bool).
- `total`: number of agents.
- `valid_count`: number of agents that pass validation.

**Example**

```json
{
  "agents": [
    { "agent_id": "code-analyzer", "role": "Writer", "valid": true, "prompt_path_exists": true },
    { "agent_id": "docs-writer", "role": "Writer", "valid": true, "prompt_path_exists": true },
    { "agent_id": "synthesis-agent", "role": "Synthesis", "valid": false, "prompt_path_exists": false }
  ],
  "total": 3,
  "valid_count": 2
}
```

## Data source

- `AgentRegistry::list_all()` (or list by role).
- For each agent: run validation (config + prompt file) and report valid/invalid and prompt path existence.
- Reuse existing `AgentRegistry::validate_agent()` where possible.

## Guards

- None beyond requiring the XDG agents directory to exist.
- Empty list is valid: show "No agents configured" (or equivalent) rather than failing.

## Implementation notes

- CLI variant under the `Agent` subcommand (e.g. `Status`).
- Handler builds table or JSON from registry plus validation results.
- Use comfy-table for the text table and the chosen styling crate for the section heading; see workspace_status_requirements.md.

## Tests required

- Unit tests: status aggregation from list_all + validate_agent results.
- Integration tests: `merkle agent status` with zero, one, and multiple agents; text and JSON output; empty XDG agents dir (no failure).
