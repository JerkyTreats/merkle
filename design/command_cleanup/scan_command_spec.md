# Scan Command Specification

## Overview

This document specifies the scan command that builds or refreshes the Merkle tree from the workspace filesystem. Scan populates the node store and is required before context operations. The Walker and TreeBuilder drive the execution flow. Scan reads and respects .gitignore and the per-workspace ignore list; see [ignore_list_spec.md](ignore_list_spec.md).

## Command structure

**Syntax**

```
merkle scan [--force]
```

**Options**

- `--force`: Rebuild the tree even if a root node already exists in the store. Without this flag, scan exits with a message if the workspace has already been scanned.

## Execution flow

1. **Resolve workspace root:** Use the configured workspace root (CLI or config).
2. **Load ignore sources:** By default, respect .gitignore and the per-workspace ignore list. Read `workspace_root.join(".gitignore")` if present; read `workspace_data_dir(workspace_root).join("ignore_list")` if the file exists. Merge with built-in Walker defaults into a single list of ignore patterns. Pass this list to the Walker via WalkerConfig. See [ignore_list_spec.md](ignore_list_spec.md).
3. **Build tree:** Create a TreeBuilder for the workspace root with the Walker config (ignore patterns); call `builder.build()` to walk the filesystem and compute the tree (Walker, node hashing, root hash). Walker skips paths matching .gitignore and the ignore list.
4. **Guard (no force):** If `--force` is not set, check whether the root node exists in the node store. If it exists, return a message such as "Tree already exists (root: …). Use --force to rebuild." and exit successfully without mutating the store.
5. **Populate store:** Call `NodeRecord::populate_store_from_tree()` with the node store and the built tree. This writes all node records and path→node_id mappings.
6. **Persistence:** Ensure the store is flushed if the backend requires it (e.g. Sled).
7. **Output:** Return a success message including node count and root hash (hex).

Scan does not write the ignore list; it only reads it when present.

## Required guards

- **Workspace root:** Must be valid and readable; TreeBuilder must be able to compute the root (or fail with a clear error).
- **Idempotency without force:** If root already exists and `--force` is not set, do not overwrite; exit with guidance.
- **Storage errors:** Propagate storage errors from populate_store_from_tree and store flush; do not leave a partial tree without reporting failure.

## Output

**Text (success):**

- Message including number of nodes scanned and root hash as hex, e.g. "Scanned N nodes (root: abc123…)."

**Text (no-op, already scanned):**

- When root exists and `--force` is not set: "Tree already exists (root: …). Use --force to rebuild."

**Errors:**

- Tree build failures (e.g. I/O, Walker errors) and store write failures must be reported with clear messages.

**JSON:** Not required for scan in this spec; can be added later via a shared `--format` option if desired.

## Implementation

- **CLI:** `Commands::Scan { force }` in `src/tooling/cli.rs`; dispatch to the scan handler.
- **Ignore loading:** Before building the tree, call logic that loads .gitignore and `workspace_data_dir(workspace_root).join("ignore_list")`; merge with WalkerConfig defaults; pass to TreeBuilder/Walker. See [ignore_list_spec.md](ignore_list_spec.md).
- **Tree building:** `TreeBuilder::new(workspace_root).with_config(walker_config)` or equivalent; Walker uses `WalkerConfig::ignore_patterns` in `src/tree/walker.rs`.
- **Store population:** `NodeRecord::populate_store_from_tree(store, &tree)`; store trait in `src/store/` (e.g. persistence module).
- **Workspace root:** Taken from `CliContext` (e.g. `self.workspace_root`). Use `crate::config::xdg::workspace_data_dir(workspace_root)` for the ignore list path.

## Tests required

- Integration: Scan empty or small workspace; verify node count and root hash; verify root exists in store and path lookups work.
- Integration: Scan again without `--force`; expect "already exists" message and no store change.
- Integration: Scan with `--force`; expect store to be repopulated and new root/node count.
- Unit or integration: Invalid or unreadable workspace root produces a clear error.
