# Command Cleanup and Status Implementation Plan

## Overview

This document outlines the phased implementation plan for the command cleanup restructure and status command suite. It covers: display stack and workspace status; agent and provider status; unified merkle status; and detailed workspace command specs (scan, validate, watch, node delete). All specs live in design/command_cleanup/; [design/context/](../context/README.md) is reference only for agent/provider concepts.

---

## Development phases

### Phase 1 — Display stack and workspace status

**Goal:** Introduce a consistent display stack (comfy-table, styling crate) and implement workspace status output per [workspace_status_requirements.md](workspace_status_requirements.md).

**Tasks**

- Add comfy-table and one styling crate (owo-colors or colored) to Cargo.toml; pin versions.
- Implement workspace status section: tree (scanned/not scanned, root hash, node count), optional breakdown, context coverage per agent, top paths by node count (max 5).
- Use comfy-table for all tables; use styling crate for section headings and optional status colors; respect NO_COLOR and TTY.
- Ensure `merkle workspace status` (or equivalent) produces this section.

**Exit criteria**

- comfy-table and styling crate in use for workspace status output.
- Workspace status shows tree state, node count, optional breakdown, and per-agent context coverage when scanned.
- Output matches [workspace_status_requirements.md](workspace_status_requirements.md); text and JSON formats.

**Key changes**

- New dependencies: comfy-table, owo-colors or colored.
- Status handler for workspace section; shared formatting helpers.

**Dependencies:** None.

**Docs:** [workspace_status_requirements.md](workspace_status_requirements.md).

---

### Phase 2 — Agent status and provider status

**Goal:** Implement `merkle agent status` and `merkle provider status` per their specs; reuse display stack.

**Tasks**

- Remove top-level **validate-providers**; provider validation is **merkle provider validate** per [provider_validate_spec.md](provider_validate_spec.md).
- Implement `merkle agent status`: table (Agent, Role, Valid, Prompt); data from AgentRegistry::list_all() and validate_agent(); text and JSON.
- Implement `merkle provider status`: table (Provider, Type, Model, optional Connectivity); optional --test-connectivity; text and JSON.
- Reuse comfy-table and styling crate for headings and tables.

**Exit criteria**

- `merkle agent status` and `merkle provider status` produce output matching [agent_status_spec.md](agent_status_spec.md) and [provider_status_spec.md](provider_status_spec.md).
- Empty lists do not fail; validation and connectivity reused from existing code.

**Key changes**

- New CLI variants: Agent::Status, Provider::Status.
- Handlers that build status from registries and validation/connectivity logic.

**Dependencies:** Phase 1 (display stack).

**Docs:** [agent_status_spec.md](agent_status_spec.md), [provider_status_spec.md](provider_status_spec.md), [provider_validate_spec.md](provider_validate_spec.md).

---

### Phase 3 — Unified merkle status

**Goal:** Implement top-level `merkle status` that concatenates workspace, agents, and providers sections; support section filters.

**Tasks**

- Implement `merkle status` with optional --workspace-only, --agents-only, --providers-only; default: all three sections.
- Pass --breakdown and --test-connectivity through to workspace and provider sections.
- Prefer single status module producing all sections; top-level status and subcommands call same logic.
- Ensure `merkle workspace status` = workspace section only (alias or same logic as merkle status --workspace-only).

**Exit criteria**

- `merkle status` outputs all three sections by default; section filters work.
- Output of each section matches the dedicated subcommands (merkle workspace status, merkle agent status, merkle provider status).

**Key changes**

- Top-level Status command wires to unified status handler; section selection by flags.
- Shared status module or coordinated handlers for workspace, agent, provider.

**Dependencies:** Phase 1, Phase 2.

**Docs:** [status_command_spec.md](status_command_spec.md).

---

### Phase 4 — Workspace command specs and scan / validate / watch

**Goal:** Implement or align scan, validate, and watch with detailed specs; ensure CLI and behavior match specs.

**Tasks** (implement in this order: ignore list, scan, validate, watch; then add tests)

1. Implement or refine **ignore list** per [ignore_list_spec.md](ignore_list_spec.md): .gitignore and `$XDG_DATA_HOME/merkle/<workspace_path>/ignore_list`; scan reads both; `merkle workspace ignore [path]` lists or adds path; workspace delete appends path unless --no-ignore.
2. Implement or refine **merkle scan** per [scan_command_spec.md](scan_command_spec.md): load ignore list and .gitignore, pass to Walker; args, TreeBuilder, populate store, guards, output. Scan creates/refreshes the workspace tree.
3. Implement or refine **merkle workspace validate** per [validate_command_spec.md](validate_command_spec.md): store, head index, basis index consistency; errors/warnings; text and JSON.
4. Implement or refine **merkle watch** per [watch_command_spec.md](watch_command_spec.md): options (debounce, batch), daemon, file watcher, tree update, optional context regeneration; use same ignore sources as scan (no --ignore flag).
5. Add or update tests for each command as specified in the specs.

**Exit criteria**

- scan, validate, and watch behavior and CLI match [scan_command_spec.md](scan_command_spec.md), [validate_command_spec.md](validate_command_spec.md), [watch_command_spec.md](watch_command_spec.md).
- Required guards and output formats implemented; tests added.

**Key changes**

- CLI and handlers in src/tooling/cli.rs and related modules; tree builder, store, head index, watch daemon as specified.

**Dependencies:** Existing tree and store implementation; Phase 1–3 only if status touches workspace.

**Docs:** [ignore_list_spec.md](ignore_list_spec.md), [scan_command_spec.md](scan_command_spec.md), [validate_command_spec.md](validate_command_spec.md), [watch_command_spec.md](watch_command_spec.md).

---

### Phase 5 — Node delete

**Goal:** Implement node deletion per [node_deletion_spec.md](node_deletion_spec.md) and [node_deletion_and_append_only.md](node_deletion_and_append_only.md).

**Tasks**

- Add NodeRecordStore::delete (and path cleanup as specified); HeadIndex::remove_heads_for_node; basis index and frame storage policy (delete blobs by default, --keep-frames option).
- Add CLI **merkle workspace delete** with path or `--node <id>`; cascade semantics; no confirmation prompts.
- Tests for cascade, head/basis index updates, and frame blob policy.

**Exit criteria**

- Node delete removes node and descendants from node store and head index; frame blob policy and --keep-frames work as specified.
- Spec and append-only doc requirements satisfied.

**Key changes**

- Store and index APIs for deletion; new command and guards.

**Dependencies:** Node store and head index implementation.

**Docs:** [node_deletion_spec.md](node_deletion_spec.md), [node_deletion_and_append_only.md](node_deletion_and_append_only.md).

---

### Phase 6 — Remove deprecated commands

**Goal:** Remove deprecated CLI commands once all replacements are in place. See [command_list.md](command_list.md) for the full remove vs keep list.

**Tasks**

- Remove top-level **validate-providers**; replacement is `merkle provider validate` (Phase 2).
- Remove low-level, node-ID-based commands: **get-node**, **get-text**, **put-frame**, **list-frames**, **get-head**. These are not exposed under `merkle context`; no direct replacement for script use — path-based flows use `merkle context get --path`, `merkle context generate`, etc.
- Remove CLI variants, dispatch branches, and any handlers used only by the removed commands. Update help and docs to drop references.
- Optional: one release prior to removal, emit a deprecation warning when a removed command is invoked (e.g. "get-node is deprecated; use merkle context get --path or --node") so scripts can migrate.

**Exit criteria**

- `merkle validate-providers` and the five low-level commands are no longer available.
- Help and documentation do not reference the removed commands.

**Key changes**

- CLI: remove command variants and their dispatch; prune help text.
- No new user-facing behavior; removal only.

**Dependencies:** Phase 2 (so `merkle provider validate` exists before validate-providers is removed); Phases 1–5 complete so status, workspace, and context workflows are in place.

**Docs:** [command_list.md](command_list.md).

---

## Implementation order summary

1. **Phase 1: Display stack and workspace status** — Foundation for all status output.
2. **Phase 2: Agent status and provider status** — Dedicated status subcommands.
3. **Phase 3: Unified merkle status** — Single entry point and section filters.
4. **Phase 4: Scan, validate, watch specs** — Workspace command behavior and tests.
5. **Phase 5: Node delete** — Deletion API and CLI.
6. **Phase 6: Remove deprecated commands** — Drop validate-providers and low-level get-node, get-text, put-frame, list-frames, get-head.

---

## Related documentation

**Spec docs with test requirements:** Each command spec has a "Tests required" section. [agent_status_spec.md](agent_status_spec.md), [provider_status_spec.md](provider_status_spec.md), [provider_validate_spec.md](provider_validate_spec.md), [status_command_spec.md](status_command_spec.md), [ignore_list_spec.md](ignore_list_spec.md), [scan_command_spec.md](scan_command_spec.md), [validate_command_spec.md](validate_command_spec.md), [watch_command_spec.md](watch_command_spec.md), [node_deletion_spec.md](node_deletion_spec.md). The delete spec includes unit, integration, consistency, and edge-case tests.

- [README.md](README.md) — Overview and command list.
- [command_list.md](command_list.md) — Remove vs keep commands.
- [workspace_status_requirements.md](workspace_status_requirements.md) — Workspace status and display stack.
- [status_command_spec.md](status_command_spec.md) — Unified status.
- [agent_status_spec.md](agent_status_spec.md), [provider_status_spec.md](provider_status_spec.md) — Agent and provider status.
- [provider_validate_spec.md](provider_validate_spec.md) — merkle provider validate; replaces top-level validate-providers.
- [ignore_list_spec.md](ignore_list_spec.md) — Ignore list and .gitignore; XDG location; scan, workspace ignore, and workspace delete behavior.
- [scan_command_spec.md](scan_command_spec.md), [validate_command_spec.md](validate_command_spec.md), [watch_command_spec.md](watch_command_spec.md) — Workspace commands (merkle scan, merkle workspace validate, merkle watch).
- [node_deletion_spec.md](node_deletion_spec.md), [node_deletion_and_append_only.md](node_deletion_and_append_only.md) — merkle workspace delete and append-only policy.
