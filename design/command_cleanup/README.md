# Workspace Commands and Command Cleanup

## Overview

This directory contains specifications for workspace-oriented commands and the command cleanup restructure for the Merkle filesystem state management tool. The suite covers scan, status, validate, watch, and node delete. Agent and provider concepts are referenced from design/context/; only workspace and status command specs live here.

---

## Commands

**CLI placement:** Top-level: `merkle scan` (creates/refreshes workspace tree), `merkle watch`. Under workspace: `merkle workspace status`, `merkle workspace validate`, `merkle workspace ignore`, `merkle workspace delete`.

| Command | CLI | Use |
|--------|-----|-----|
| **scan** | merkle scan | Build or refresh the Merkle tree from the workspace filesystem; required before context operations. See [scan_command_spec.md](scan_command_spec.md). |
| **status** | merkle workspace status | Unified status: workspace summary, agents, providers. See [status_command_spec.md](status_command_spec.md) and [workspace_status_requirements.md](workspace_status_requirements.md). |
| **validate** | merkle workspace validate | Check workspace data integrity: store, head index, basis index consistency. See [validate_command_spec.md](validate_command_spec.md). |
| **watch** | merkle watch | Run the file-watcher daemon so the tree (and optionally context) stays updated on filesystem changes. See [watch_command_spec.md](watch_command_spec.md). |
| **ignore** | merkle workspace ignore [path] | With no path: pretty-list the ignore list. With path: add path so future scans skip it. Does not delete nodes. See [ignore_list_spec.md](ignore_list_spec.md). |
| **delete** | merkle workspace delete | Remove a node and its descendants (the branch) by path or `--node <id>`. See [node_deletion_spec.md](node_deletion_spec.md). |

For commands to remove or keep (get-node, get-text, put-frame, list-frames, get-head, validate-providers), see [command_list.md](command_list.md).

---

## Key concepts

**Tree:** Merkle tree over the workspace; built by scan and updated by watch. Node store holds node records; root hash and node count describe size and shape.

**Node store:** Index of "current tree" nodes; path to NodeID mapping. Populated by scan; optionally pruned by node delete.

**Ignore list:** Per-workspace list of paths/patterns at `$XDG_DATA_HOME/merkle/<workspace_path>/ignore_list`. Scan reads it and .gitignore by default. `merkle workspace ignore` with no path pretty-lists the contents; `merkle workspace ignore <path>` adds a path without deleting; `merkle workspace delete` adds the deleted path unless `--no-ignore`. See [ignore_list_spec.md](ignore_list_spec.md).

**Head index:** Maps (node_id, frame_type) to the current head frame ID. Status reports context coverage per agent using head index and node store.

**Path resolution:** Path-based commands canonicalize relative to the workspace root and look up NodeID in the node store; see [design/context/](../context/README.md) for path resolution notes.

---

## Terminology

- **Workspace:** The directory that is the root of the Merkle tree (e.g. project root).
- **Scan:** One-time or forced rebuild of the tree from the filesystem; populates the node store.
- **Status:** Summary of workspace (tree size, context coverage), agents (validation, prompt path), and providers (connectivity).
- **Validate:** Integrity check of store, head index, and basis index.
- **Watch:** Long-lived daemon that watches the workspace and updates the tree (and optionally triggers regeneration).

---

## Path resolution

Path-based commands canonicalize paths relative to the configured workspace root, look up NodeID in the node store, and fail with PathNotInTree if the path is not in the tree. Error guidance should suggest running `merkle scan` or starting `merkle watch` if the tree may be outdated.

---

## Related documentation

**Implementation plan**

- [PLAN.md](PLAN.md) — Phased implementation for display stack, status commands, and workspace command specs.

**Status and display**

- [workspace_status_requirements.md](workspace_status_requirements.md) — Workspace status content, tree size, context coverage, display stack (comfy-table, styling crate).
- [status_command_spec.md](status_command_spec.md) — Unified merkle status and workspace section behavior.
- [agent_status_spec.md](agent_status_spec.md) — merkle agent status.
- [provider_status_spec.md](provider_status_spec.md) — merkle provider status.

**Workspace commands**

- [scan_command_spec.md](scan_command_spec.md) — merkle scan.
- [validate_command_spec.md](validate_command_spec.md) — merkle workspace validate.
- [watch_command_spec.md](watch_command_spec.md) — merkle watch.

**Ignore list**

- [ignore_list_spec.md](ignore_list_spec.md) — .gitignore and per-workspace ignore list; XDG location; scan reads; workspace ignore adds path; workspace delete appends unless --no-ignore.

**Node deletion**

- [node_deletion_spec.md](node_deletion_spec.md) — Node delete behavior, cascade, frame blob policy.
- [node_deletion_and_append_only.md](node_deletion_and_append_only.md) — Append-only policy and node index deletion.

**Command list**

- [command_list.md](command_list.md) — Remove vs keep list for low-level and reorganized commands.

**Context (reference only)**

- [design/context/](../context/README.md) — Agent and provider concepts, AgentRegistry, ProviderRegistry, agent/provider CLI; use as reference when implementing status and workspace commands. No specs for those commands live in context/.
