# Workspace Status Requirements: Tree vs Context Readiness

## Display stack

All status output (workspace, agent, provider, and unified `merkle status`) uses a shared display stack for consistent tables and styling.

**comfy-table**

- Use for all tabular status output: workspace breakdown, context coverage, top paths by node count; agent status table; provider status table.
- Ensures consistent column alignment, optional borders, and UTF-8 styling.
- Add to `Cargo.toml` as a dependency; pin version (e.g. 21.x or 22.x).

**Lightweight styling crate**

- Use for section headings, section labels, and optional color (e.g. success/warning for validation or connectivity).
- Options: **owo-colors** (zero-allocation, trait-based) or **colored** (simple API). Specify one and pin version in `Cargo.toml`.
- Conventions: headings bold/underline; optional green/red for status indicators; respect `NO_COLOR` and TTY detection so scripts and pipes get plain text.

**Cross-cutting:** All status commands (workspace, agent, provider, and unified `merkle status`) use this stack so output is consistent.

---

## Goal

Provide a clear **status** between "merkle tree setup" (after `merkle scan`) and "LLM context initiated" (frames generated). Users must be able to:

1. See **how big** the tree is (node count, and where the bulk lives).
2. Guard against **very large paths** (e.g. `node_modules`) by seeing counts and adjusting ignores before generating context.
3. See **context coverage per agent**: how many nodes have a context frame for each agent vs how many do not.

Each agent has its own frame type (e.g. `context-code-analyzer`); coverage is therefore **per agent**, not global.

---

## User Workflow

1. User runs **`merkle scan`** → tree is built, node store populated.
2. User runs **status** (or a dedicated "readiness" command) → sees:
   - Tree size (total nodes, optional breakdown by path).
   - Per-agent: nodes with a head frame vs nodes without (and optionally %).
   - Top 5 path prefixes by node count (root first) so the user can see where the bulk lives.
3. User adjusts **ignore patterns** (config or CLI) if needed, then **rescan** (or a future "scan with configurable ignores").
4. User runs **context generate** (or future batch) with confidence they are not generating context for 50k `node_modules` files.

---

## Requirements

### 1. Tree size and shape

| ID | Requirement | Notes |
|----|-------------|--------|
| T1 | **Total node count** | Exact count of nodes in the store (files + directories). Today status only shows "scanned" vs "not scanned"; we need a number. |
| T2 | **Optional: breakdown by top-level path** | For each direct child of the workspace root (e.g. `src`, `node_modules`, `docs`), show node count. Enables "node_modules has 47k nodes, consider ignoring." May be behind a flag (e.g. `--breakdown`) or config. |
| T3 | **Efficient computation** | Total count must not require a full tree rebuild. Iterate node store or maintain a count; breakdown may require grouping by path prefix from stored `NodeRecord.path`. |

### 2. Context coverage per agent

| ID | Requirement | Notes |
|----|-------------|--------|
| C1 | **Per-agent coverage** | For each **Writer/Synthesis agent** (from registry): show **nodes with head frame** for that agent’s frame type and **nodes without**. |
| C2 | **Frame type → agent** | Coverage is keyed by frame type (e.g. `context-code-analyzer`). Map frame type to agent ID for display (config or convention: `context-<agent_id>`). |
| C3 | **Denominator = store nodes** | "Nodes without" = total nodes in store minus nodes that have a head for that agent’s frame type. Head index is `(node_id, frame_type) → frame_id`; node store is the source of truth for "all nodes." |
| C4 | **Summary line per agent** | At least: `agent_id`, `nodes_with_frame`, `nodes_without_frame`, and optionally `coverage_pct`. |

### 3. Guarding large paths (ignore)

| ID | Requirement | Notes |
|----|-------------|--------|
| I1 | **Ignore patterns apply at scan time** | By default scan respects **.gitignore** (workspace root) and a **per-workspace ignore list** at `$XDG_DATA_HOME/merkle/<workspace_path>/ignore_list`. Walker receives merged patterns (built-in defaults + .gitignore + ignore_list). See [ignore_list_spec.md](ignore_list_spec.md). |
| I2 | **Status reflects post-scan tree** | Status shows counts for the **current** tree (already built with whatever ignores were in effect). If the user wants to exclude more paths, they add ignores (or run `merkle workspace delete` to add a path to the ignore list) and **rescan**; status then shows the new counts. |
| I3 | **Top paths by node count** | List up to **5** path prefixes sorted by total node count (descending). Root (workspace) is always first; then the next four heaviest path prefixes. No percentage or threshold; fixed limit 5. |

### 4. Status output shape

| ID | Requirement | Notes |
|----|-------------|--------|
| S1 | **Clear phases** | Output should make it obvious: (1) tree ready? (2) how big? (3) per-agent context readiness. |
| S2 | **State-dependent output** | When the workspace has **not** been scanned, tree size, breakdown, and context coverage are unavailable; show only that the tree is not ready and what to do next. When scanned, show full tree and coverage sections. |
| S3 | **Stable, parseable option** | Optional machine-readable output (e.g. JSON) for scripts or UIs: `--format json`. |
| S4 | **No breaking change to existing status** | Existing `merkle status` can be extended with the above, or a new subcommand (e.g. `merkle status context` or `merkle readiness`) can be added. Prefer extending `merkle status` with extra sections/flags so one command stays the source of truth. |

### 5. Non-goals (out of scope for this doc)

- **Pre-scan estimate** ("how many nodes would we get if we scanned with these ignores?"): could be a future feature (dry-run walk).
- **Generating context** from this command: status is read-only; generation stays in `context generate` or a future batch command.

---

## Data and APIs

- **Node count**: Iterate `NodeRecordStore` (e.g. Sled) or add/maintain a count. No existing "node count" API today; status currently infers "scanned" by checking if root exists.
- **Head index**: `HeadIndex` is `(NodeID, frame_type) → FrameID`. To get "nodes with head for frame_type T": iterate `heads` and count distinct node_ids where frame_type == T. "Nodes without" = total nodes − that count.
- **Path breakdown**: `NodeRecord` has `path: PathBuf`. Group by first path component (relative to workspace root) and count. Requires iterating the store.
- **Agent list**: `AgentRegistry::list_all()` (or list Writer/Synthesis only); frame type per agent from config/convention (`context-<agent_id>` unless overridden).

---

## Output states

Status output depends on whether the workspace has been scanned.

| State | Tree section | Context coverage | Top paths (by node count, max 5) |
|-------|--------------|------------------|----------------------------------|
| **Not scanned** | Show "Scanned: no" only; no root hash, no node count, no breakdown. | Omitted. | Omitted. |
| **Scanned** | Show root hash, total nodes, optional top-level breakdown. | Show per-agent table. | Show up to 5 paths, root first, then next heaviest. |

When not scanned, the command should direct the user to run `merkle scan` first.

---

## Example output (human-readable)

### Not scanned

```
Workspace Status

Tree
  Scanned: no

Run `merkle scan` to build the tree, then run status again for node count and context coverage.
```

### Scanned (full output)

Tables are used for breakdown, context coverage, and top paths by node count so the output is scannable and consistent.

```
Workspace Status

Tree
  Root hash: abc123...
  Total nodes: 52
  Scanned: yes

  Top-level breakdown

  | Path     | Nodes |
  |----------|-------|
  | src/     | 38    |
  | config/  | 8     |
  | docs/    | 6     |

Context coverage

  | Agent           | With frame | Without | Coverage |
  |-----------------|------------|---------|----------|
  | code-analyzer   | 52         | 0       | 100%     |
  | docs-writer     | 12         | 40      | 23%      |
  | synthesis-agent | 0          | 52      | 0%       |

Top paths by node count

  | Path     | Nodes |
  |----------|-------|
  | .        | 52    |
  | src/     | 38    |
  | config/  | 8     |
  | docs/    | 6     |
```

---

## JSON output and schema

When `--format json` is used, workspace status output has the following shape. Types: string, number, boolean, array, object.

**Not scanned**

- `scanned`: boolean, false.
- `message`: string, e.g. "Run merkle scan to build the tree."
- `tree`, `context_coverage`, `top_paths_by_node_count`: omitted or null.

**Example (not scanned)**

```json
{
  "scanned": false,
  "message": "Run merkle scan to build the tree."
}
```

**Scanned**

- `scanned`: boolean, true.
- `tree`: object with `root_hash` (string), `total_nodes` (number), optional `breakdown` (array of `{ "path": string, "nodes": number }`).
- `context_coverage`: array of objects: `agent_id` (string), `nodes_with_frame` (number), `nodes_without_frame` (number), optional `coverage_pct` (number or string).
- `top_paths_by_node_count`: array of up to 5 objects: `path` (string), `nodes` (number). Root first (e.g. path "." or workspace root), then next four heaviest by node count.

**Example (scanned)**

```json
{
  "scanned": true,
  "tree": {
    "root_hash": "abc123...",
    "total_nodes": 52,
    "breakdown": [
      { "path": "src/", "nodes": 38 },
      { "path": "config/", "nodes": 8 },
      { "path": "docs/", "nodes": 6 }
    ]
  },
  "context_coverage": [
    { "agent_id": "code-analyzer", "nodes_with_frame": 52, "nodes_without_frame": 0, "coverage_pct": 100 },
    { "agent_id": "docs-writer", "nodes_with_frame": 12, "nodes_without_frame": 40, "coverage_pct": 23 },
    { "agent_id": "synthesis-agent", "nodes_with_frame": 0, "nodes_without_frame": 52, "coverage_pct": 0 }
  ],
  "top_paths_by_node_count": [
    { "path": ".", "nodes": 52 },
    { "path": "src/", "nodes": 38 },
    { "path": "config/", "nodes": 8 },
    { "path": "docs/", "nodes": 6 }
  ]
}
```

Implementers and scripts can rely on these keys and types for parsing. Optional fields may be omitted when empty or when a flag (e.g. `--breakdown`) is not set.

---

## Dependencies

- **Ignore list and .gitignore**: Scan reads .gitignore and the per-workspace ignore list at `$XDG_DATA_HOME/merkle/<workspace_path>/ignore_list`. After viewing status, the user can add paths (e.g. via `merkle workspace delete` or by editing the ignore list), rescan, and see reduced counts. See [ignore_list_spec.md](ignore_list_spec.md).
- **Per-agent coverage**: Depends on agent registry (list Writer/Synthesis agents) and head index (count by frame_type). Node store must support iteration for total count and optional path breakdown.

---

## Summary

| Area | Requirements |
|------|---------------|
| **Tree** | Total node count (T1); optional breakdown by top-level path (T2); efficient (T3). |
| **Context** | Per-agent: nodes with/without head frame, optional % (C1–C4). |
| **Ignore** | Configurable ignore patterns at scan time (I1); status reflects current tree (I2); top 5 paths by node count, root first (I3). |
| **Output** | Clear phases (S1); state-dependent (S2): not-scanned shows minimal output; optional JSON (S3); extend existing status or add subcommand (S4). Tables for breakdown, coverage, and top paths by node count. |

This gives users a clear picture between "tree set up" and "LLM context initiated," and lets them trim large paths via ignores before generating context. Test requirements for workspace status output are in [status_command_spec.md](status_command_spec.md) and the workspace command specs (scan, validate, watch).
