# Observability Context TUI Implementation Plan

## Overview

This plan covers active design work under `design/`.

Execution order is:

1. Bootstrap observability
2. Build context orchestration on that foundation
3. Integrate TUI against the observability contract

This order keeps runtime contracts stable before UI integration.

---

## Development Phases

### Phase 1 — Observability Bootstrap

**Goal**: Implement durable event infrastructure in sled and wire command session lifecycle.

**Status**: ✅ Completed

| Task | Status |
| --- | --- |
| Create progress module set under `src/progress/` | ✅ Completed |
| Implement `ProgressEvent` schema and serde support | ✅ Completed |
| Implement in process event bus | ✅ Completed |
| Implement sled store trees for sessions and events | ✅ Completed |
| Implement ingestor with monotonic per session sequence | ✅ Completed |
| Implement session start and end helpers | ✅ Completed |
| Wire CLI command entry to session lifecycle | ✅ Completed |
| Implement pruning for completed sessions | ✅ Completed |
| Add unit tests for sequence ordering and schema compatibility | ✅ Completed |
| Add integration tests for replay and interrupted session behavior | ✅ Completed |

**Exit Criteria:**

- Every user command produces session boundary events
- Event ordering is stable per session
- Replay from sled reproduces final session state
- TUI can follow active sessions from sled

**Dependencies:**

- None

**Documentation:**

- [Observability Specification](observability/observability_spec.md)
- [Feature Migration Specification](observability/feature_migration_spec.md)

---

### Phase 2 — Feature Migration to Observability

**Goal**: Migrate implemented features to event emission with stable payloads.

**Status**: ✅ Completed

| Task | Status |
| --- | --- |
| Migrate context generate command path | ✅ Completed |
| Migrate frame queue and provider lifecycle events | ✅ Completed |
| Migrate scan events | ✅ Completed |
| Migrate watch daemon events | ✅ Completed |
| Migrate regenerate and synthesize flows | ✅ Completed |
| Migrate context get summary events | ✅ Completed |
| Migrate workspace status and validate summaries | ✅ Completed |
| Migrate workspace mutation command summaries | ✅ Completed |
| Migrate agent and provider command summaries | ✅ Completed |
| Migrate init summary events | ✅ Completed |

**Progress Update:**

- Added session scoped event emit helpers in `src/progress/session.rs`
- Added typed event payload models in `src/progress/event.rs`
- Added queue event context and provider lifecycle emission in `src/frame/queue.rs`
- Migrated `context generate`, `scan`, `watch`, `regenerate`, `synthesize`, and `context get` paths in `src/tooling/cli.rs` and `src/tooling/watch.rs`
- Added command summary emission for command families including workspace, agent, provider, and init
- Expanded integration coverage in `tests/integration/progress_observability.rs` and `tests/integration/frame_queue.rs`
- Verified with targeted integration test runs and full library test runs

**Exit Criteria:**

- ✅ P0 and P1 migrations are implemented from migration spec
- ✅ Long running workflows emit useful progress events
- ✅ Failure paths still emit terminal session state
- ✅ Payload fields are stable and replay safe

**Dependencies:**

- Phase 1

**Documentation:**

- [Feature Migration Specification](observability/feature_migration_spec.md)

---

### Phase 3 — Context Orchestration

**Goal**: Implement generation plan and orchestrator execution model with queue integration.

**Status**: ✅ Completed

| Task | Status |
| --- | --- |
| Implement generation plan types | ✅ Completed |
| Implement path resolution and subtree collection | ✅ Completed |
| Implement deepest first level grouping | ✅ Completed |
| Implement single directory descendant readiness check | ✅ Completed |
| Implement head filtering and force semantics | ✅ Completed |
| Implement orchestrator module and result model | ✅ Completed |
| Implement failure policy behavior | ✅ Completed |
| Implement active plan first queue interaction rules | ✅ Completed |
| Emit context and orchestrator event families | ✅ Completed |
| Add integration coverage for overlap and dedupe scenarios | ✅ Completed |

**Progress Update:**

- Added `src/generation/` module with `GenerationPlan`, `GenerationItem`, `GenerationResult`, `LevelSummary`, `FailurePolicy`, and `PlanPriority` in `plan.rs`; validation and serde round-trip unit tests added
- Implemented `GenerationOrchestrator` in `orchestrator.rs` with `QueueSubmitter` trait, level-ordered execution, and unit tests for `Continue` and `StopOnLevelFailure` policies
- Refactored `context generate` in `src/tooling/cli.rs`: added `--no-recursive`, `build_generation_plan`, `collect_subtree_levels`, `find_missing_descendant_heads`; recursive mode uses deepest-first levels; single-directory mode enforces descendant readiness unless `--force`; plan execution delegated to orchestrator
- Updated queue in `src/frame/queue.rs`: dedupe identity is `node_id + agent_id + frame_type`; added `GenerationRequestOptions` and `enqueue_and_wait_with_options`; head short-circuit and force semantics; plan-aware request ordering
- Context events: `plan_constructed`, `descendant_check_started` / `descendant_check_passed` / `descendant_check_failed`, `node_skipped`; orchestrator events: `generation_started`, `level_started`, `node_generation_started` / `completed` / `failed`, `level_completed`, `generation_completed` / `generation_failed`
- Integration tests updated for new `ContextCommands::Generate` shape and queue request options; library and integration test suites pass

**Exit Criteria:**

- Recursive directory generation is leaf to trunk
- Orchestrator controls submission order and level barriers
- Queue dedupe and completed head reuse behavior match spec
- Context generation emits required events for TUI session view

**Dependencies:**

- Phase 1
- Phase 2 for queue and provider events

**Documentation:**

- [Generation Pipeline Specification](context/generation_pipeline_spec.md)
- [Generation Orchestrator Specification](context/generation_orchestrator_spec.md)
- [Context Generate By Path Specification](context/context_generate_by_path_spec.md)
- [LLM Payload Specification](context/llm_payload_spec.md)

---

### Phase 4 — TUI Integration

**Goal**: Implement TUI consumption of sled observability store and interactive workflows.

**Status**: 🚧 In Progress

| Task | Status |
| --- | --- |
| Implement session discovery from sled | ⬜ Pending |
| Implement active follow by sequence cursor | ⬜ Pending |
| Implement replay mode from stored events | ⬜ Pending |
| Implement session state model updates from events | ⬜ Pending |
| Implement dashboard session widgets | ⬜ Pending |
| Implement session view progress and activity panels | ⬜ Pending |
| Implement command bar execution with session wiring | ⬜ Pending |
| Implement session history operations | ⬜ Pending |
| Add headless integration coverage for follow and replay | ⬜ Pending |

**Exit Criteria:**

- TUI reads and follows sessions from sled
- Replay is consistent with live session outcomes
- Session view renders provider lifecycle and node state transitions
- TUI command execution creates and follows new sessions

**Dependencies:**

- Phase 1
- Phase 2
- Phase 3 for full context generation flow visibility

**Documentation:**

- [TUI Specification](tui/tui_spec.md)
- [TUI Session State Specification](tui/tui_session_state_spec.md)
- [Tree Browser Specification](tui/tree_browser_spec.md)

---

## Implementation Order Summary

1. Observability core modules and session lifecycle
2. Queue and provider event migration
3. Scan and watch event migration
4. Context generate migration
5. Context orchestration implementation
6. Remaining command summary migrations
7. TUI follow and replay integration
8. TUI interactive workflow integration

---

## Testing Strategy

### Unit Tests

- Event schema serialization and forward compatibility behavior
- Sequence generation and key ordering behavior
- Session lifecycle helper behavior
- Context plan construction and ordering logic
- TUI state reducer behavior from event streams

### Integration Tests

- Session boundary events for each command family
- Replay accuracy for completed and interrupted sessions
- Queue dedupe and completed head reuse scenarios
- Recursive context generation leaf to trunk behavior
- TUI follow latency and state update behavior

### CLI and TUI Workflow Tests

- `context generate` session progression with queue and provider events
- `scan` and `watch` observability progression
- TUI command bar start then auto follow new session
- Session history list open replay flow

---

## Success Criteria

- Observability is durable and sled backed
- Event contracts are stable and forward compatible
- Context orchestration behavior matches current specs
- TUI is a first class consumer with reliable follow and replay
- Feature migrations are complete for implemented command surface
- Completed artifacts remain under `design/completed/` with no active edits

---

## Related Documentation

- [Observability Specification](observability/observability_spec.md)
- [Feature Migration Specification](observability/feature_migration_spec.md)
- [Context Design README](context/README.md)
- [Generation Pipeline Specification](context/generation_pipeline_spec.md)
- [Generation Orchestrator Specification](context/generation_orchestrator_spec.md)
- [Context Generate By Path Specification](context/context_generate_by_path_spec.md)
- [LLM Payload Specification](context/llm_payload_spec.md)
- [TUI Design README](tui/README.md)
- [TUI Specification](tui/tui_spec.md)
