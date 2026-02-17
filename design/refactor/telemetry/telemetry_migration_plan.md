# Telemetry Migration Plan

Date: 2026-02-17

## Objective

Define a migration plan for telemetry refactor work with clear dependency order, boundary control, and parity gates.

## Scope

This plan covers migration work owned by telemetry specs in this folder.

- telemetry domain creation under `src/telemetry`
- session lifecycle and prune policy extraction from CLI execution paths
- command summary mapping extraction from CLI handlers
- event routing and sink ownership migration out of `src/progress`
- integration with workspace watch and context generation event hooks

## Related Specs

- [Telemetry Event Engine Spec](telemetry_event_engine_spec.md)
- [Workspace Migration Guide](../workspace/workspace_migration_guide.md)
- [Workspace Watch Runtime Spec](../workspace/workspace_watch_runtime_spec.md)
- [Context Migration Plan](../context/context_migration_plan.md)
- [Agent Migration Plan](../agent/agent_migration_plan.md)
- [Provider Migration Plan](../provider/provider_migration_plan.md)
- [Config Migration Plan](../config/config_migration_plan.md)
- [CLI Migration Plan](../cli/cli_migration_plan.md)
- [Tooling Diffusion Map](../cli/tooling_diffusion_map.md)
- [Dependency Gate Checklist](../dependency_gate_checklist.md)
- [God Module Detangling Spec](../god_module_detangling_spec.md)
- [Src Module Structure Map](../src_module_structure_map.md)

## Dependency Inventory

### Hard dependencies

- session start finish and prune policy are still owned by `execute` in `src/tooling/cli.rs`
- command summary mapping is still owned by `emit_command_summary` and `typed_summary_event` in `src/tooling/cli.rs`
- telemetry contracts routing ingestion and store are still grouped under `src/progress`
- `src/progress/session.rs` currently depends on CLI command enums for command name mapping
- workspace watch runtime still passes progress runtime hooks through legacy wiring in `src/tooling/watch.rs`
- context generation paths still emit events through progress runtime hooks from CLI and generation paths

### Sequencing dependencies

- telemetry domain contracts and facade should land before workspace watch and context generation hook cutovers
- telemetry routing and sink extraction should land before session and emission services to avoid split ownership
- summary mapper extraction should align with provider agent context and workspace command cutovers so payload shapes stay stable
- shared CLI cutover for telemetry should align with provider agent config context and workspace shell cutovers
- shared shell sequencing should follow CLI migration plan route waves and gates
- compatibility wrappers should remain short lived and removed in one boundary seal window

### Cross plan dependencies

- this plan provides telemetry contracts and emission services consumed by workspace watch migration
- this plan provides summary mapping contracts consumed by agent provider context and workspace command cutovers
- this plan depends on context migration for generation event source contracts
- this plan depends on workspace migration for watch runtime event source contracts
- this plan aligns with config migration in shared CLI startup and command cutover windows

### Cutover dependencies

- cut over telemetry session ownership in one phase and remove CLI session policy in the same phase window
- cut over typed summary mapping in one phase and remove CLI summary mapping helpers in the same phase window
- remove duplicated event routing policy from legacy progress and shell modules as soon as telemetry services are active

## Difficulty Assessment

### Session lifecycle extraction

Difficulty: high

- session lifecycle is tightly coupled to command execution success and failure paths
- prune policy currently runs inline with command completion behavior
- mitigation is characterization tests for session ordering status and prune behavior before and after extraction

### Summary mapping extraction

Difficulty: high

- typed summary events cover workspace status validate mutation and config command families
- payload shapes are automation facing and must stay stable across migrations
- mitigation is deterministic summary contract tests for event type and payload fields

### Routing and sink ownership split

Difficulty: medium

- routing ingestion and persistence already exist but are named and grouped under legacy progress paths
- risk is temporary duplication while both progress and telemetry surfaces exist
- mitigation is strict compatibility wrapper window with clear removal gate

## Migration Phases

1. Characterization baseline
- add parity tests for session started and ended ordering across success and failure commands
- add parity tests for prune trigger behavior and completed session retention limits
- add parity tests for typed summary events and `command_summary` payload fields
- capture current command family to summary event map from CLI handlers

2. Telemetry domain foundation
- add `src/telemetry/mod.rs`
- add `src/telemetry/facade.rs`
- add `src/telemetry/types.rs`
- add module roots for contracts sessions emission routing and sinks
- keep `src/progress` as thin compatibility wrapper surface during migration

3. Contract extraction
- add `src/telemetry/contracts/event.rs`
- add `src/telemetry/contracts/summary.rs`
- move event and summary contract ownership out of `src/progress/event.rs`
- route call sites through telemetry contracts with no payload behavior change

4. Routing and sink extraction
- add `src/telemetry/routing/bus.rs`
- add `src/telemetry/routing/ingestor.rs`
- add `src/telemetry/sinks/store.rs`
- add `src/telemetry/sinks/tui.rs`
- add `src/telemetry/sinks/otel.rs`
- move routing ingestion and persistent sink ownership out of `src/progress`

5. Session service extraction
- add `src/telemetry/sessions/service.rs`
- add `src/telemetry/sessions/policy.rs`
- move session start finish interrupted and prune behavior out of CLI execution paths
- remove direct dependency from session service to CLI command enums

6. Emission engine and summary mapper extraction
- add `src/telemetry/emission/engine.rs`
- add `src/telemetry/emission/summary_mapper.rs`
- move `emit_command_summary` and `typed_summary_event` policy out of CLI handlers
- keep event names and payload fields stable for compatibility

7. Cross plan integration
- route context generation event hooks through telemetry facade contracts
- route workspace watch event hooks through telemetry facade contracts
- route provider agent and workspace command summary emission through telemetry mapper contracts
- keep deterministic text and json command behavior with best effort telemetry failure policy

8. Shared CLI cutover
- reduce telemetry logic in `src/tooling/cli.rs` to delegation only
- remove inline session lifecycle and summary mapping policy from CLI execute paths
- align this cutover with provider agent config context and workspace shell cutovers

9. Legacy removal and boundary seal
- remove compatibility wrappers from `src/progress` after call site migration
- remove telemetry policy ownership from CLI and legacy modules
- enforce telemetry domain contracts to prevent cross domain leakage

## Guardrails

- telemetry domain owns session lifecycle summary mapping emission routing and sink policies
- CLI shell owns parse route command delegation and output mapping only
- workspace and context domains own business behavior and emit telemetry through contracts only
- keep existing event names and payload fields stable during migration
- keep telemetry failures best effort and non blocking for command completion

## Test Strategy

### Behavior parity

- parity for session lifecycle ordering status and prune behavior
- parity for typed summary event type selection by command family
- parity for `command_summary` truncation and payload size fields
- parity for watch and generation event emission through telemetry facade

### Boundary checks

- route tests confirm CLI does not own telemetry session or summary policies
- guard tests confirm workspace and context call telemetry contracts not telemetry internals
- guard tests confirm telemetry does not own provider context or workspace business orchestration

### Contract checks

- deterministic event ordering checks per session
- deterministic json field checks for summary events and session events
- deterministic sink write and read checks for persistent telemetry store

## Exit Criteria

- telemetry event engine ownership is under `src/telemetry`
- CLI execute paths do not own session lifecycle or summary mapping policy
- `src/progress` is removed or remains as a compatibility free reexport surface only
- workspace watch and context generation use telemetry contracts for event emission
- characterization parity and boundary suites pass

## Risks And Mitigation

- risk: session ordering regressions around command success and failure boundaries
- mitigation: lifecycle integration tests and deterministic sequence assertions

- risk: summary payload drift across command families during mapper extraction
- mitigation: snapshot style contract tests for event type and payload fields

- risk: sink failures affecting command behavior
- mitigation: best effort emission with explicit failure handling checks

- risk: merge churn in shared CLI cutover window
- mitigation: staged extraction by session then summary then wrapper removal
