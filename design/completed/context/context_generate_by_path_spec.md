# Context generate by path specification

## 1. Overview

`merkle context generate` resolves a file system path, builds a `GenerationPlan`, and delegates execution to the `GenerationOrchestrator`.

Recursive mode targets the full subtree rooted at the target path. Recursive ordering is leaf to trunk so directory generation always runs after all deeper file and directory nodes are complete.

All generation emits structured events to session files through the event system in [design/observability/observability_spec.md](../observability/observability_spec.md). Real time monitoring is provided by [design/tui/tui_spec.md](../tui/tui_spec.md).

## 2. Path resolution and target

- **Path input:** Path is accepted through `--path <path>` or positional path. Path is canonicalized relative to workspace root and resolved by `NodeRecordStore.find_by_path`.
- **Path not in tree:** Return `PathNotInTree` and suggest `merkle scan` or `merkle watch`.
- **Target type:** Target node can be a file or directory.

## 3. Required generation request fields

Every generated item in a plan includes queue ready execution fields:

- `node_id`
- `path`
- `node_type`
- `agent_id`
- `provider_name`
- `frame_type`

Context Generate owns construction of these fields. The orchestrator uses these fields to submit queue requests and emit execution events.

## 4. Single node mode with --no-recursive

- **File target:** Generate one frame for the file node.
- **Directory target:** Generate one frame for the directory node.
- **Directory descendant readiness check:** For each descendant in the subtree, verify a head exists for the selected `frame_type`.
- **Readiness failure:** If any descendant head is missing and `--force` is not set, return an error listing missing paths or node ids.
- **Readiness success:** Submit a single item plan for the target directory.

## 5. Recursive subtree mode

- **Default mode:** Recursive is default for directory targets.
- **Scope:** File target resolves to one node. Directory target resolves to full subtree with files and directories.
- **Head filtering:** Without `--force`, skip nodes that already have a head for selected `frame_type`.
- **Forced regeneration:** With `--force`, include all nodes in subtree regardless of existing head.
- **Execution order:** Group by depth and execute deepest level first, then each parent level, ending at the subtree root. This is leaf to trunk ordering.
- **Directory readiness by order:** Directory nodes are generated after children because level ordering guarantees child frames exist before directory payload assembly.

Recursive mode does not run a separate missing subtree preflight gate. Missing heads are normal inputs for recursive generation.

## 6. Subtree collection and level grouping

- **Subtree:** Target node and all descendants, collected from `NodeRecord.children`.
- **Cycle safety:** Track visited node ids and reject cyclic traversal.
- **Depth groups:** Depth zero is subtree root. Max depth levels run first, then higher ancestors, ending at depth zero.
- **Shared helper:** Subtree collection and level grouping are implemented in shared helpers used by descendant checks and plan construction.

## 7. CLI surface

- `--path <path>`
- positional path
- `--node <node_id>`
- `--agent <agent_id>`
- `--provider <provider_name>`
- `--frame-type <frame_type>`
- `--force`
- `--no-recursive`

`--path` and positional path are mutually exclusive with `--node`.

All generation runs through orchestrator plus queue execution. CLI blocks until orchestrator returns.

## 8. Errors

- **Path not in tree:** Suggest `merkle scan` or `merkle watch`.
- **Directory single node readiness failure:** List missing descendant paths or node ids and suggest generating descendants or using `--force`.
- **Queue rejection:** Surface queue enqueue failure and suggest retry.
- **Plan validation failure:** Surface invalid plan structure before generation starts.

## 9. Output and exit status

On completion the CLI prints generated and failed totals plus session id.

Context Generate uses `GenerationResult` to set exit status:

- success when `total_failed` is zero
- failure when `total_failed` is nonzero
- failure when orchestrator returns error before any result

## 10. Required tests

### Unit tests

- Path canonicalization and lookup resolution
- File target with recursive default generates single node plan
- Directory target with recursive default builds full subtree plan with deepest first levels
- Directory target with `--no-recursive` enforces descendant readiness for selected `frame_type`
- Head filtering skips nodes with existing heads when `--force` is not set
- `--force` includes all nodes regardless of existing heads
- Plan items include `node_id`, `path`, `node_type`, `agent_id`, `provider_name`, `frame_type`

### Integration tests

- Recursive directory generation executes leaf to trunk and produces directory frames after child frames
- Two recursive plans for the same folder deduplicate shared nodes in queue and both callers receive completion
- Plan B for higher branch started during Plan A subbranch execution reuses Plan A pending and completed work for shared `node_id + agent_id + frame_type` keys
- High priority single file request for file already present in active recursive plan queue deduplicates and does not regenerate
- Non overlapping direct single file request submitted during active plan remains queued until active plan completion
- `--frame-type` with non default value drives both head checks and payload child frame selection
- Mixed success run returns nonzero failed count and nonzero exit status

## 11. Related docs

- [generation_pipeline_spec.md](generation_pipeline_spec.md)
- [llm_payload_spec.md](llm_payload_spec.md)
- [generation_orchestrator_spec.md](generation_orchestrator_spec.md)
- [design/observability/observability_spec.md](../observability/observability_spec.md)
- [design/tui/tui_spec.md](../tui/tui_spec.md)
- [design/completed/context/context_generate_command.md](../completed/context/context_generate_command.md)