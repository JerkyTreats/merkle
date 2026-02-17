# Workspace Migration Guide

Date: 2026-02-17

## Objective

Define a migration guide for workspace refactor work with clear dependency order, boundary control, and parity gates.

## Scope

This guide covers migration work owned by workspace specs in this folder.

- workspace lifecycle command orchestration extraction from CLI shell
- workspace status service extraction from `src/workspace_status.rs`
- workspace watch runtime and editor bridge extraction from legacy tooling modules
- integration with context generation hooks and telemetry hooks
- shared CLI cutover with agent provider config and context migration plans

## Related Specs

- [Workspace Lifecycle Services Spec](workspace_lifecycle_services.md)
- [Workspace Watch Runtime Spec](workspace_watch_runtime_spec.md)
- [Context Migration Plan](../context/context_migration_plan.md)
- [Config Migration Plan](../config/config_migration_plan.md)
- [Agent Migration Plan](../agent/agent_migration_plan.md)
- [Provider Migration Plan](../provider/provider_migration_plan.md)
- [Telemetry Migration Plan](../telemetry/telemetry_migration_plan.md)
- [CLI Migration Plan](../cli/cli_migration_plan.md)
- [Telemetry Event Engine Spec](../telemetry/telemetry_event_engine_spec.md)
- [Tooling Diffusion Map](../cli/tooling_diffusion_map.md)
- [Dependency Gate Checklist](../dependency_gate_checklist.md)
- [God Module Detangling Spec](../god_module_detangling_spec.md)
- [Src Module Structure Map](../src_module_structure_map.md)

## Dependency Inventory

### Hard dependencies

- workspace command handlers in `src/tooling/cli.rs` still own validate delete restore compact list deleted and ignore orchestration paths
- workspace status assembly and unified status shaping are still mixed in `src/workspace_status.rs`
- watch runtime and event batching remain in `src/tooling/watch.rs`
- editor bridge behavior remains in `src/tooling/editor.rs`
- workspace root and storage path resolution still depends on config loader and path helpers in `src/config.rs`
- watch queue submit hooks still depend on legacy context and queue surfaces

### Sequencing dependencies

- config composition facade and path modules should land before workspace lifecycle and watch services are wired in startup and CLI routes
- context orchestration and queue contracts should land before watch queue submit hook cutover
- telemetry contracts and session services should land before watch telemetry hook cutover
- workspace lifecycle and status extraction should land before shared CLI shell cutover so route handlers become thin
- unified status ownership split should consume agent and provider status contracts from their migration plans
- shared shell sequencing should follow CLI migration plan route waves and gates
- shell cutover should align with provider agent config and context shared cutover window

### Cross plan dependencies

- this plan depends on config migration for composition facade and workspace path contracts
- this plan depends on context migration for queue and generation contracts used by watch hooks
- this plan depends on telemetry migration for session and emission contracts used by watch and command summaries
- this plan consumes agent migration status contracts for unified status agent sections
- this plan consumes provider migration status and diagnostics contracts for unified status provider sections
- this plan provides workspace lifecycle status and watch service boundaries consumed during shared CLI cutover in other plans

### Cutover dependencies

- cut over workspace command routes in one phase and remove legacy CLI orchestration in the same phase window
- cut over watch runtime ownership in one phase and remove legacy watch and editor wrappers in the same phase window
- remove duplicate workspace policy from shell and legacy modules as soon as workspace services are active

## Difficulty Assessment

### Workspace lifecycle extraction

Difficulty: high

- delete restore compact and validate flows share tree store head index and ignore side effects
- command behavior spans text and json output paths with many edge cases
- mitigation is staged service extraction backed by characterization and parity suites

### Workspace status ownership split

Difficulty: medium

- workspace section logic and unified status shaping are currently co located
- status data depends on workspace records plus agent and provider sections
- mitigation is explicit status contracts per section and deterministic output checks

### Watch runtime and editor bridge extraction

Difficulty: high

- runtime lifecycle event intake batching and queue submit hooks are tightly coupled in legacy modules
- primary risk is drift in debounce batching ordering and shutdown behavior
- mitigation is event matrix characterization and strict runtime contract tests

## Migration Phases

1. Characterization baseline
- add parity tests for workspace validate delete restore compact list deleted and ignore outputs in text and json
- add parity tests for workspace status section and unified status workspace section behavior
- add parity tests for watch runtime event batching filtering startup and shutdown behavior
- capture route to dependency map for workspace handlers in CLI

2. Workspace domain foundation
- add `src/workspace/mod.rs`
- add `src/workspace/facade.rs`
- add `src/workspace/types.rs`
- add module roots for lifecycle status and watch areas
- wire initial facade delegation with no behavior change

3. Lifecycle service extraction
- add `src/workspace/lifecycle_service.rs`
- move validate delete restore compact list deleted and ignore orchestration out of `src/tooling/cli.rs`
- keep store head index and ignore side effects owned by workspace lifecycle service

4. Status service extraction
- add `src/workspace/status_service.rs`
- move workspace section assembly out of `src/workspace_status.rs`
- split unified status section shaping so workspace consumes agent and provider status contracts

5. Watch events and editor bridge extraction
- add `src/workspace/watch/mod.rs`
- add `src/workspace/watch/events.rs`
- add `src/workspace/watch/editor_bridge.rs`
- move event normalization filtering and editor bridge behavior out of legacy tooling modules

6. Watch runtime extraction and context integration
- add `src/workspace/watch/runtime.rs`
- move watch lifecycle and batching ownership out of `src/tooling/watch.rs`
- route queue submit hooks through context contracts from context migration
- route telemetry emit hooks through telemetry contracts

7. Cross plan integration
- route workspace startup and command config loads through config composition facade
- route unified status agent and provider sections through domain contracts from agent and provider migrations
- keep text and json response contracts deterministic across all workspace commands

8. Shared shell cutover
- reduce workspace handlers in `src/tooling/cli.rs` to parse route and output selection only
- remove direct workspace orchestration and watch runtime setup from CLI paths
- align this cutover with provider agent config and context cutover phases

9. Legacy removal and boundary seal
- remove legacy workspace lifecycle methods from `src/tooling/cli.rs`
- remove legacy watch and editor wrappers from `src/tooling/watch.rs` and `src/tooling/editor.rs`
- slim or remove migrated ownership from `src/workspace_status.rs`
- enforce workspace domain contracts to prevent cross domain leakage

## Guardrails

- workspace domain owns lifecycle status assembly for workspace section and watch runtime policy
- context domain owns generation processing behavior and queue execution semantics
- config domain owns source loading precedence merge and workspace path composition policy
- CLI shell owns parse route and output mapping only
- keep automation facing json field names stable during migration

## Test Strategy

### Behavior parity

- workspace validate delete restore compact list deleted and ignore parity in text and json
- workspace status and unified status workspace section parity
- watch runtime parity for create modify remove rename debounce and batch behavior

### Boundary checks

- route tests confirm one workspace service call per workspace command variant
- guard tests confirm CLI does not own workspace lifecycle or watch runtime policy
- guard tests confirm workspace watch queue hooks call context contracts rather than context internals
- guard tests confirm workspace status service consumes agent and provider contracts rather than internals

### Contract checks

- deterministic error envelope checks for lifecycle status and watch runtime failures
- deterministic json field checks for workspace command and status outputs
- deterministic ordering checks for deleted list and watch event batches

## Exit Criteria

- workspace lifecycle orchestration is owned by `src/workspace/lifecycle_service.rs`
- workspace status assembly for workspace section is owned by `src/workspace/status_service.rs`
- workspace watch runtime and editor bridge are owned by `src/workspace/watch`
- CLI workspace handlers own parse route and output selection only
- legacy workspace orchestration watch and status ownership is removed from legacy modules
- characterization parity and boundary suites pass

## Risks And Mitigation

- risk: behavior drift in delete restore compact ignore side effects
- mitigation: characterization snapshots and deterministic side effect assertions

- risk: watch runtime drift in debounce batching and shutdown behavior
- mitigation: event matrix tests and runtime lifecycle contract checks

- risk: unified status regression during section ownership split
- mitigation: section level parity tests for workspace agent and provider outputs

- risk: merge churn in shared CLI cutover window
- mitigation: command family staging and strict route ownership checks
