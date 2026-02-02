# Workspace Validate Command Specification

## Overview

This document specifies **merkle workspace validate**, the command that checks workspace data integrity: node store, head index, and basis index consistency. The command reports errors and warnings; it does not modify data.

## Command structure

**Syntax**

```
merkle workspace validate [--format text|json]
```

**Options**

- `--format <text|json>`: Output format (default: text). When json, output a structured object with errors and warnings arrays plus summary fields.

## Execution flow

1. **Compute workspace root:** Use TreeBuilder to compute the current workspace root hash from the filesystem. If this fails, report an error and exit (e.g. "Failed to compute workspace root").
2. **Check root in store:** Look up the root hash in the node store. If missing, add a warning (e.g. "Root node not found in store - workspace may not be scanned"). If present, verify the stored record’s node_id matches the computed root; if not, add an error.
3. **Head index consistency:** For each node_id in the head index, for each head frame_id, verify the frame exists in frame storage. If a head frame is missing, add a warning (e.g. "Head frame X for node Y not found in storage").
4. **Basis index consistency:** For each entry in the basis index, verify referenced frame IDs exist in frame storage. If a frame is missing, add a warning (e.g. "Basis index frame X not found in storage").
5. **Optional: frame count:** Optionally count frame files on disk and include in the summary.
6. **Output:** Format errors and warnings per --format; if no errors and no warnings, report "Validation passed" with summary (root hash, node count, frames). Otherwise report "Validation completed with issues" and list errors and warnings.

## Required guards

- **Root computation failure:** Treat as a hard error; do not continue with store/head/basis checks without a valid root.
- **Read-only:** Validate must not modify the store, head index, basis index, or frame storage.
- **Locking:** Use read locks on head index and basis index when iterating; avoid deadlocks and release before calling frame storage.

## Output

**Text (success):**

- "Validation passed:" followed by root hash, node count, frame count, and "All checks passed."

**Text (issues):**

- "Validation completed with issues:" followed by root hash, node count, frame count; then "Errors (N):" and list; then "Warnings (N):" and list.

**JSON (optional):**

- Object with keys such as: `valid` (bool), `root_hash` (hex string), `node_count`, `frame_count`, `errors` (array of strings), `warnings` (array of strings).

**Errors:**

- Root computation failure; root record node_id mismatch. Other consistency failures may be errors or warnings per policy.

## Implementation

- **CLI:** `Commands::Validate` in `src/tooling/cli.rs`; add `--format` if not present; dispatch to validate handler.
- **Logic:** Handler in same file or a small validate module; uses `api.node_store()`, `api.head_index()`, `api.basis_index()`, `api.frame_storage()`; TreeBuilder for root computation.
- **Workspace root:** From CliContext (e.g. `self.workspace_root`).

## Tests required

- Integration: Validate after a successful scan; expect "Validation passed" and no errors/warnings (or only expected warnings if head/basis are empty).
- Integration: Validate when workspace not scanned (root missing in store); expect warning about root not found.
- Integration: Corrupt or remove a head-referenced frame; expect warning about head frame not found.
- Integration: If --format json is implemented, verify JSON shape and valid/errors/warnings fields.
- Unit: Error and warning message formatting.
