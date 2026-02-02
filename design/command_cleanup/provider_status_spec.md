# Provider Status Command Specification

## Overview

This document specifies `merkle provider status`, a summary view of all providers: count and optional connectivity. It complements `merkle provider list` and `merkle provider show`; status is a quick health/readiness check. For provider form and registry behavior, see design/context/provider/provider_cli_spec.md as reference only.

## Command structure

**Syntax**

```
merkle provider status [--format text|json] [--test-connectivity]
```

**Options**

- `--format <text|json>`: Output format (default: text).
- `--test-connectivity`: When set, run a lightweight connectivity check per provider and report OK / Fail / Skipped in the table. May be slow.

## Purpose

Provide a quick health/readiness summary: how many providers exist and optionally whether each can reach its API. Scripts and users can run status before context generation to confirm providers are configured and reachable.

## Output

### Text format

- Section heading (styled via the display stack; see workspace_status_requirements.md).
- Table via comfy-table: **Provider** | **Type** | **Model** | **Connectivity** (if `--test-connectivity`: OK / Fail / Skipped; otherwise omitted or blank).
- Summary line: "Total: N providers."

**Example**

```
Providers

  | Provider        | Type      | Model          | Connectivity |
  |-----------------|-----------|----------------|--------------|
  | openai-gpt4     | openai    | gpt-4          | OK           |
  | anthropic-claude| anthropic | claude-3-opus  | OK           |
  | local-ollama    | ollama    | llama2         | Skipped     |

Total: 3 providers.
```

### JSON format

Structured object:

- `providers`: array of objects with `provider_name`, `provider_type`, `model`, and optional `connectivity` when `--test-connectivity` was used (e.g. "ok", "fail", "skipped").
- `total`: number of providers.

**Example**

```json
{
  "providers": [
    { "provider_name": "openai-gpt4", "provider_type": "openai", "model": "gpt-4", "connectivity": "ok" },
    { "provider_name": "anthropic-claude", "provider_type": "anthropic", "model": "claude-3-opus", "connectivity": "ok" },
    { "provider_name": "local-ollama", "provider_type": "ollama", "model": "llama2", "connectivity": "skipped" }
  ],
  "total": 3
}
```

## Data source

- `ProviderRegistry` list (e.g. list_all or equivalent).
- When `--test-connectivity`: run a lightweight connectivity check per provider; reuse existing test logic where possible.

## Guards

- Empty list is valid; do not fail the command.
- Connectivity failures are reported in the table (e.g. Fail), not as command failure unless desired for a specific exit code policy.

## Implementation notes

- New `Provider` subcommand variant `Status`.
- Reuse optional existing connectivity test logic.
- Use comfy-table and the styling crate per workspace_status_requirements.md.

## Tests required

- Unit tests: status aggregation from registry.
- Integration tests: `merkle provider status` with zero, one, and multiple providers; text and JSON; with and without `--test-connectivity`; empty providers dir (no failure).
