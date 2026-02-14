# Phase 2 Observability Implementation Review

| ID | Severity | Finding | Evidence | Open Question | Fixed Status |
| --- | --- | --- | --- | --- | --- |
| F1 | High | Async `context generate` can violate session terminal ordering and likely drop async work because command session closes before queue work lifecycle is complete. | `src/tooling/cli.rs`, `tests/integration/context_cli.rs`, `tests/integration/progress_observability.rs`, `design/observability/observability_spec.md:71` | [Q1](#q1) | Fixed |
| F2 | High | Required command-specific summary event families are missing across most command surfaces; only generic `command_summary` is emitted. | `design/observability/feature_migration_spec.md:51`, `design/observability/feature_migration_spec.md:61`, `src/tooling/cli.rs:3177`, `src/tooling/cli.rs:3205`, `tests/integration/progress_observability.rs:135` | - | Fixed |
| F3 | High | `provider test` command does not emit provider lifecycle events required by migration spec. | `design/observability/feature_migration_spec.md:199`, `src/tooling/cli.rs:2360`, `src/tooling/cli.rs:2385`, `src/tooling/cli.rs:2441`, `tests/integration/progress_observability.rs:315` | - | Fixed |
| F4 | Medium | Queue dedupe is partial: checks only pending queue items and misses in-flight duplicates; sync enqueue path has no dedupe. | `src/frame/queue.rs:287`, `src/frame/queue.rs:377`, `design/observability/feature_migration_spec.md:96` | - | Not fixed |
| F5 | Medium | `enqueue_batch` does not emit per-request `request_enqueued` events. | `src/frame/queue.rs:468`, `design/observability/feature_migration_spec.md:103` | - | Not fixed |
| F6 | Medium | `scan_progress` is emitted once, not on batch cadence by node count. | `src/tooling/cli.rs:1153`, `design/observability/feature_migration_spec.md:119` | - | Not fixed |
| F7 | Medium | Context-generate payloads omit required identity fields from spec, notably `path`. | `design/observability/feature_migration_spec.md:86`, `src/tooling/cli.rs:2673`, `src/tooling/cli.rs:2690` | - | Not fixed |
| F8 | Low | Event timestamp format is numeric epoch millis, not ISO 8601 with milliseconds as specified. | `src/progress/event.rs:36`, `src/progress/event.rs:142`, `design/observability/observability_spec.md:77` | - | Not fixed |
| F9 | Low | `command_summary` stores raw output/error text; this is not a stable metric-focused summary shape and can produce large payloads. | `src/tooling/cli.rs:2885`, `src/tooling/cli.rs:2887`, `design/observability/feature_migration_spec.md:166` | [Q2](#q2) | Not fixed |

## Progress Snapshot (February 14, 2026)

- Findings fixed: `3/9` (`F1`, `F2`, `F3`)
- Findings remaining: `6/9` (`F4`, `F5`, `F6`, `F7`, `F8`, `F9`)
- High-severity findings: `3/3 fixed`
- Medium-severity findings: `0/4 fixed`
- Low-severity findings: `0/2 fixed`

## Decisions Made

| ID | Decision | Scope Impact | Related Findings | Related Open Question | Status |
| --- | --- | --- | --- | --- | --- |
| D1 | Keep `context generate` as a blocking CLI call and stream progress from observability events during execution. | Prioritize minimal corrective work on existing contracts and event coverage; no workflow redesign required for this phase. | F1 | [Q1](#q1) | Implemented |
| D2 | Remove CLI `--async` from `context generate` until durable non-blocking job/session semantics are designed and approved. | Eliminates ambiguous session lifecycle behavior and keeps implementation aligned with existing blocking orchestrator specs. | F1 | [Q1](#q1) | Implemented |
| D3 | Keep this effort constrained to correcting and expanding what is already specified and implemented; avoid net-new architectural ground. | Focus execution on spec-alignment gaps in existing Phase 2/3 boundaries and observability payload/event completeness. | F2, F3, F4, F5, F6, F7, F8, F9 | - | Accepted |
| D4 | Keep `command_summary` as a backward-compatible fallback and add command-family typed summary events additively. | Low-risk path: no schema break, no replay migration, and supports incremental rollout of required typed summary coverage. | F2, F9 | [Q2](#q2) | Implemented |


## Open Questions

<a id="q1"></a>
### Q1
Should async `context generate` be durable across command exit, or should async mode be scoped only to active command lifetime?

Current status: Closed for current scope by decisions [D1](#decisions-made) and [D2](#decisions-made). `context generate` is blocking-only for now; durable async behavior is deferred until future requirements.

<a id="q2"></a>
### Q2
Should `command_summary` remain as a fallback event while adding command-family-specific summary events, or be replaced entirely by typed summary families?

Current status: Closed for current scope by decision [D4](#decisions-made). `command_summary` remains for compatibility while typed summary events are added incrementally.

## Successful Fix Criteria

| Finding | Successful Fix Looks Like | Failure-State Tests (Where Applicable) |
| --- | --- | --- |
| F1 | `context generate` runs in blocking mode only for this scope, and queue/provider lifecycle events occur before `session_ended` in the same session stream. | Integration: execute `context generate` and assert `session_ended` is last event; assert no queue/provider events appear after `session_ended`. CLI surface test: `--async` is rejected/removed. |
| F2 | Required command-family typed summary events are emitted for migrated command groups, while `command_summary` remains for compatibility. | Integration: per command family, assert typed summary event is present and ordered before `session_ended`; assert `command_summary` is still present. |
| F3 | `provider test` emits provider lifecycle events with stable payload shape (`provider_request_sent` and either `provider_response_received` or `provider_request_failed`). | Integration success path: assert `provider_request_sent` + `provider_response_received`. Integration failure path (bad endpoint/model): assert `provider_request_sent` + `provider_request_failed`. |
| F4 | Queue dedupe semantics match spec identity and behavior, including in-flight coalescing for sync callers. | Integration overlap tests: duplicate requests (including in-flight) produce single provider execution and dedupe events; both callers receive completion. |
| F5 | `enqueue_batch` emits `request_enqueued` per submitted item (or documented equivalent with deterministic per-item visibility). | Unit/integration: enqueue N batch items and assert N corresponding enqueue events are persisted for that session. |
| F6 | `scan_progress` reflects batch cadence by node count rather than a single end-of-build emission. | Integration on large fixture: assert multiple `scan_progress` events are emitted with monotonic progress and terminal `scan_completed`. |
| F7 | Context-generate event payloads include required identity fields from spec (including `path` where required). | Payload-shape integration: assert `plan_constructed`/related context events contain required fields (`node_id`, `path`, `agent_id`, `provider_name`, `frame_type`). |
| F8 | Timestamp format is aligned to documented event contract or documentation is updated to match implementation with explicit decision. | Unit: validate timestamp format contract used by emitted events. Integration: sampled events conform to chosen contract. |
| F9 | Typed summary events are metric-focused and bounded in size; `command_summary` fallback does not carry unbounded command output payloads. | Unit/integration: assert summary payload includes metrics fields; assert output/error fields are truncated, omitted, or bounded by policy. |

### Cross-Cutting Acceptance Gate

- All new tests for failure states are deterministic and do not require network access unless explicitly mocked.
- Per-session sequence remains monotonic with no gaps and `session_ended` remains terminal for completed/failed sessions.
- Replay of persisted events reconstructs the same terminal state asserted by command exit status.
