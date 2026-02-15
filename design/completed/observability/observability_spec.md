# Observability specification

## 1. Purpose

Observability is designed for the TUI as the primary customer.

The event system provides a stable stream that supports:

- real time TUI progress rendering
- reliable replay
- session history and drill down in TUI views

The engine remains headless. All rendering is owned by the TUI.

## 2. TUI first requirements

The observability system shall satisfy these requirements for TUI behavior:

- **Freshness:** TUI sees newly committed events quickly during active work
- **Ordering:** TUI sees strict per session order from monotonic sequence values
- **Completeness:** replay from storage reconstructs the same final session state as live run
- **Stability:** event schema changes remain forward compatible
- **Recoverability:** TUI can restart and rebuild state from persistent storage with no in memory dependency

## 3. Storage decision

Primary persistent store is `sled`.

No additional embedded database is introduced for observability.

Persistent observability data is stored in `sled` and consumed directly by the TUI.

## 4. Architecture

```
Context Generate --emit--> EventBus --ingest--> EventStore sled
Orchestrator    --emit-->
Queue           --emit-->
Scanner         --emit-->
Watcher         --emit-->
TUI reader      <--follow and replay-- EventStore sled
```

- **EventBus:** in process channel for low latency fan out
- **EventIngestor:** single writer task that assigns sequence and commits to `sled`
- **EventStore:** durable store in `sled`
- **TUI reader:** reads active and completed sessions from `sled`

## 5. Data model in sled

Use dedicated trees:

- `obs_sessions`
- `obs_events`
- `obs_session_meta`

Key and value model:

- `obs_sessions` key `session_id`
- `obs_sessions` value includes command metadata, start timestamp, end timestamp, status
- `obs_events` key `session_id:seq`
- `obs_events` value is serialized event payload
- `obs_session_meta` key `session_id`
- `obs_session_meta` value includes next sequence and latest status snapshot

Sequence rules:

- `seq` starts at `1` for each session
- `seq` increases by `1` for every committed event in that session
- `session_started` is always `seq 1`
- `session_ended` is final sequence for completed sessions

## 6. Event format

Each stored event contains:

- `ts` in ISO 8601 with milliseconds
- `session` session id
- `seq` monotonic sequence in session
- `type` snake case event type
- `data` event payload object

Consumers ignore unknown event types and unknown fields.

## 7. Event catalog

### Session

- `session_started`
- `session_ended`

### Context Generate

- `plan_constructed`
- `descendant_check_started`
- `descendant_check_passed`
- `descendant_check_failed`
- `node_skipped`

### Orchestrator

- `generation_started`
- `level_started`
- `node_generation_started`
- `node_generation_completed`
- `node_generation_failed`
- `level_completed`
- `generation_completed`
- `generation_failed`

### Queue

- `request_enqueued`
- `request_deduplicated`
- `request_processing`
- `queue_stats`

### Provider

- `provider_request_sent`
- `provider_response_received`
- `provider_request_failed`
- `provider_request_retrying`

### Scanner

- `scan_started`
- `scan_progress`
- `scan_completed`

### Watcher

- `watch_started`
- `file_changed`
- `batch_processed`

## 8. Delivery and performance guarantees

The system guarantees:

- sequence order is preserved for each session
- committed events are durable in `sled`
- active follow in TUI can present updates with low latency

Operational target for TUI responsiveness:

- median emit to visible latency under `150 ms` on local machine
- ninety fifth percentile emit to visible latency under `500 ms` on local machine

## 9. Failure behavior

- Ingest commit error marks session failed and records best effort failure event
- Partial sessions remain readable by TUI
- Missing `session_ended` marks session active or interrupted

## 10. Session lifecycle

- Command start creates session record and commits `session_started`
- Events commit through command execution
- Command completion commits `session_ended`
- Pruning removes old completed sessions by age and count policy
- Active or interrupted sessions are not pruned

## 11. Implementation location

- `src/progress/event.rs`
- `src/progress/bus.rs`
- `src/progress/store.rs`
- `src/progress/ingestor.rs`
- `src/progress/session.rs`
- `src/progress/mod.rs`

## 12. Required tests

### Unit tests

- Event serialization to valid JSON
- Event deserialization equivalence
- Unknown fields ignored
- Timestamp format validation
- Session id uniqueness under rapid creation
- Sequence assignment monotonic per session
- Key encoding preserves lexical order for `session_id:seq`

### Integration tests

- `session_started` commits as sequence `1`
- Ten emitted events commit as ten sequential records
- `session_ended` commits as final record in completed session
- Follow query reads newly committed events during active session
- Replay query rebuilds final session state for completed session
- Missing `session_ended` session remains readable and marked interrupted
- Pruning removes oldest completed sessions by count
- Pruning removes expired completed sessions by age
- Active and interrupted sessions are excluded from pruning

### TUI contract tests

- Provider request sent and provider response received update node status from waiting to complete
- Node generation failed renders failure detail with error message
- Level started and level completed update progress model correctly
- Session history view can list, open, and replay a session from `sled`

## 13. Recommended crate usage

Current crate stack supports this design:

- `sled` for durable event store
- `tokio` for ingest worker and follow loops
- `serde` and `serde_json` for event schema
- `tracing` and `tracing-subscriber` for diagnostics

Optional enhancement:

- `metrics` and Prometheus exporter for runtime counters

## 14. Related docs

- [feature_migration_spec.md](feature_migration_spec.md)
- [design/context/generation_pipeline_spec.md](../context/generation_pipeline_spec.md)
- [design/context/generation_orchestrator_spec.md](../context/generation_orchestrator_spec.md)
- [design/context/context_generate_by_path_spec.md](../context/context_generate_by_path_spec.md)
- [design/tui/tui_spec.md](../tui/tui_spec.md)
