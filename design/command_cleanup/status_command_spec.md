# Unified Status Command Specification

## Overview

This document specifies the **unified** `merkle status` command that combines workspace, agent, and provider status into one output. It also defines how `merkle workspace status` relates to the unified command. All status output uses the display stack described in workspace_status_requirements.md.

## Command structure

**Syntax**

```
merkle status [--workspace-only | --agents-only | --providers-only] [--format text|json] [--breakdown] [--test-connectivity]
```

**Options**

- `--workspace-only`: Output only the workspace section.
- `--agents-only`: Output only the agents section.
- `--providers-only`: Output only the providers section.
- `--format <text|json>`: Output format (default: text).
- `--breakdown`: When present, include workspace top-level path breakdown (see workspace_status_requirements.md).
- `--test-connectivity`: When present, pass through to provider status so connectivity is tested and shown.

If none of `--workspace-only`, `--agents-only`, or `--providers-only` is set, all three sections are output.

## Behavior

By default, output three sections in order:

1. **Workspace** — Content as in workspace_status_requirements.md: tree (scanned/not scanned, root hash, node count, optional breakdown, heavy paths), context coverage per agent when scanned. Use comfy-table and the styling crate.
2. **Agents** — Same content as `merkle agent status`: table of agents with validation and prompt status.
3. **Providers** — Same content as `merkle provider status`: table of providers with API key and optional connectivity when `--test-connectivity` is set.

Filtering: `--workspace-only`, `--agents-only`, or `--providers-only` output only that section so scripts or users can request a subset.

## CLI placement

- **merkle status** — Unified command (default: all three sections; filter with the above flags).
- **merkle workspace status** — Workspace section only. Equivalent to `merkle status --workspace-only`; may be implemented as an alias or by calling the same workspace-status logic.
- **merkle agent status** — Agents section only; same output as the agents section in `merkle status`.
- **merkle provider status** — Providers section only; same output as the providers section in `merkle status`.

So: `merkle status` is the unified entry point; `merkle workspace status`, `merkle agent status`, and `merkle provider status` remain and their output is identical to the corresponding section in `merkle status`.

## Implementation

Recommend approach (b) for single source of truth:

- Have a single status module (or coordinated handlers) that can produce all three sections.
- Top-level `merkle status` uses that module and concatenates the requested sections.
- `merkle workspace status`, `merkle agent status`, and `merkle provider status` call the same logic for their respective section(s).

Alternative (a): Call the same logic as the three subcommands and concatenate from the top-level status handler. Either way, the text and JSON shape of each section must match the dedicated subcommands.

Flags such as `--breakdown` and `--test-connectivity` are passed through to the workspace and provider sections as specified in their respective specs.

## Tests required

- Integration: `merkle status` with no filter (all three sections); with each of `--workspace-only`, `--agents-only`, `--providers-only`; with `--breakdown` and `--test-connectivity`; text and JSON.
- Consistency: Output of each section in `merkle status` matches `merkle workspace status`, `merkle agent status`, and `merkle provider status` respectively.
