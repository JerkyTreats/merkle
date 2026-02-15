# Observability feature migration specification

## 1. Purpose

This specification defines how implemented runtime features migrate to the observability architecture in `design/observability/observability_spec.md`.

The goals are:

- full session visibility for TUI workflows
- durable replay for active and completed work
- consistent event naming and payload shape across features
- low friction migration using existing module boundaries

## 2. Migration policy

- every user visible command creates a session record
- long running workflows emit progress events
- short workflows emit session boundary events and summary data
- feature code emits domain events and never writes to storage directly
- progress module owns sequence assignment and storage commits

## 3. Common integration pattern

For each migrated feature:

1. CLI entry creates `session_id`
2. CLI entry emits `session_started`
3. feature emits domain events through optional event handle
4. feature returns result or error
5. CLI entry emits `session_ended`

Required module additions:

- `src/progress/event.rs`
- `src/progress/bus.rs`
- `src/progress/store.rs`
- `src/progress/ingestor.rs`
- `src/progress/session.rs`

## 4. Feature migration matrix

| Feature | Current modules | Session type | Required events | Priority |
| --- | --- | --- | --- | --- |
| Context generate by path and node | `src/tooling/cli.rs` `src/tooling/adapter.rs` `src/frame/queue.rs` | command session | session, context generate, queue, provider | P0 |
| Frame queue processing | `src/frame/queue.rs` | shared under parent command session | queue, provider | P0 |
| Scan | `src/tooling/cli.rs` `src/tree/builder.rs` `src/tree/walker.rs` | command session | session, scanner | P0 |
| Watch daemon | `src/tooling/watch.rs` | daemon session | session, watcher, queue, provider | P0 |
| Context get | `src/tooling/cli.rs` `src/api.rs` | command session | session, context read summary | P1 |
| Regenerate | `src/tooling/cli.rs` `src/api.rs` `src/regeneration.rs` | command session | session, regeneration, queue, provider | P1 |
| Synthesize branch | `src/tooling/cli.rs` `src/api.rs` `src/synthesis.rs` | command session | session, synthesis | P1 |
| Workspace status | `src/tooling/cli.rs` `src/workspace_status.rs` | command session | session, status summary | P2 |
| Workspace validate | `src/tooling/cli.rs` | command session | session, validate summary | P2 |
| Workspace delete | `src/tooling/cli.rs` `src/api.rs` | command session | session, workspace mutation | P2 |
| Workspace restore | `src/tooling/cli.rs` `src/api.rs` | command session | session, workspace mutation | P2 |
| Workspace compact | `src/tooling/cli.rs` `src/api.rs` | command session | session, workspace maintenance | P2 |
| Workspace list deleted | `src/tooling/cli.rs` | command session | session, list summary | P3 |
| Workspace ignore add and list | `src/tooling/cli.rs` `src/ignore.rs` | command session | session, config mutation summary | P3 |
| Unified status | `src/tooling/cli.rs` | command session | session, status summary | P2 |
| Agent commands | `src/tooling/cli.rs` `src/agent.rs` | command session | session, config mutation summary | P3 |
| Provider commands | `src/tooling/cli.rs` `src/provider.rs` | command session | session, config mutation summary, provider test | P3 |
| Init | `src/tooling/cli.rs` `src/init.rs` | command session | session, init summary | P3 |

## 5. Feature specific migration details

### 5.1 Context generate

Scope:

- includes path mode and node mode
- includes sync and queued paths
- includes force and frame type variants

Migration steps:

- add optional event handle to generate entry path
- emit `plan_constructed` once plan data is created
- emit `node_skipped` for head reuse
- propagate event handle into queue requests
- emit queue and provider events from queue worker

Required payload fields:

- `session_id`
- `plan_id` when available
- `node_id`
- `path`
- `agent_id`
- `provider_name`
- `frame_type`
- `duration_ms` for completed and failed events

### 5.2 Frame queue

Scope:

- dedupe logic
- retry logic
- provider request lifecycle

Migration steps:

- extend `GenerationRequest` with optional event metadata
- emit `request_enqueued`
- emit `request_deduplicated`
- emit `request_processing`
- emit provider request lifecycle events around provider calls
- emit `queue_stats` on periodic cadence and on major state transitions

### 5.3 Scan

Scope:

- full tree build path
- force and no change paths

Migration steps:

- emit `scan_started` before walk
- emit `scan_progress` on batch cadence by node count
- emit `scan_completed` with totals and duration

### 5.4 Watch

Scope:

- daemon start
- file system change batches
- optional regeneration and ensure frames paths

Migration steps:

- emit `watch_started` on daemon start
- emit `file_changed` for normalized events
- emit `batch_processed` after each debounce batch
- when queue work runs, reuse queue and provider event emission

### 5.5 Regenerate

Migration steps:

- add regeneration event family in progress event catalog
- emit per node decision events for changed and skipped paths
- reuse queue and provider events for generated nodes

### 5.6 Synthesize branch

Migration steps:

- add synthesis event family
- emit start and completed events with child count and duration
- emit failure event with error detail

### 5.7 Read and status oriented commands

Commands:

- context get
- workspace status
- workspace validate
- unified status
- list deleted

Migration steps:

- emit session boundary events
- emit one summary event with command specific metrics

### 5.8 Workspace mutation commands

Commands:

- delete
- restore
- compact
- ignore add

Migration steps:

- emit session boundary events
- emit mutation summary with target count and duration
- emit failure summary on error

### 5.9 Agent and provider management

Commands:

- list
- show
- create
- edit
- remove
- validate
- test

Migration steps:

- emit session boundary events
- emit config mutation summary events where writes occur
- for provider test emit provider request lifecycle events

### 5.10 Init

Migration steps:

- emit session boundary events
- emit init summary with created and skipped item counts

## 6. Payload compatibility rules

- required fields are stable once introduced
- optional fields are additive
- consumers ignore unknown fields
- event type names are append only

## 7. Rollout phases

### Phase P0

- progress core modules
- context generate integration
- queue and provider integration
- scan integration
- watch integration

### Phase P1

- regenerate integration
- synthesize integration
- context get summary integration

### Phase P2

- workspace status and validate integration
- unified status integration
- delete restore compact integration

### Phase P3

- list and config workflows
- agent workflows
- provider workflows
- init workflow

## 8. Migration test requirements

For every migrated feature:

- session boundary events exist and are ordered
- replay reconstructs expected final state
- command failure still emits terminal session state
- event payload includes required identity fields

Feature group tests:

- queue dedupe scenarios emit dedupe event and single provider lifecycle
- active watch plus generate overlap emits coherent interleaved sessions
- scan plus generate parallel runs maintain per session sequence order

## 9. Ownership and implementation map

- CLI session lifecycle wiring: `src/tooling/cli.rs`
- Queue and provider events: `src/frame/queue.rs`
- Scan events: `src/tree/builder.rs`
- Watch events: `src/tooling/watch.rs`
- Regenerate events: `src/regeneration.rs` and `src/api.rs`
- Synthesis events: `src/synthesis.rs` and `src/api.rs`
- Progress storage and sequencing: `src/progress/`

## 10. Related docs

- [observability_spec.md](observability_spec.md)
- [design/tui/tui_spec.md](../tui/tui_spec.md)
- [design/context/generation_pipeline_spec.md](../context/generation_pipeline_spec.md)
- [design/context/generation_orchestrator_spec.md](../context/generation_orchestrator_spec.md)
