# Context Migration Plan

Date: 2026-02-17

## Objective

Define a migration plan for context refactor work with clear dependency order, boundary control, and parity gates.

## Scope

This plan covers migration work owned by context specs in this folder.

- context domain creation under `src/context`
- query and read policy extraction from legacy API and helper modules
- mutation lifecycle extraction from legacy API paths
- generation orchestration and queue lifecycle extraction from CLI and legacy modules
- shared shell cutover and removal of legacy context surfaces

## Related Specs

- [Context Domain Structure Spec](context_domain_structure.md)
- [Context Generation Orchestration Spec](context_generation_orchestration.md)
- [Context Query API Spec](../api/context_query_api.md)
- [Provider Migration Plan](../provider/provider_migration_plan.md)
- [Agent Migration Plan](../agent/agent_migration_plan.md)
- [Config Migration Plan](../config/config_migration_plan.md)
- [Workspace Migration Guide](../workspace/workspace_migration_guide.md)
- [Telemetry Migration Plan](../telemetry/telemetry_migration_plan.md)
- [CLI Migration Plan](../cli/cli_migration_plan.md)
- [Tooling Diffusion Map](../cli/tooling_diffusion_map.md)
- [Dependency Gate Checklist](../dependency_gate_checklist.md)
- [God Module Detangling Spec](../god_module_detangling_spec.md)
- [Src Module Structure Map](../src_module_structure_map.md)

## Dependency Inventory

### Hard dependencies

- context query mutation and lifecycle logic are mixed in `src/api.rs`
- view policy and composition logic are split across `src/views.rs` and `src/composition.rs`
- generation orchestration is split across `src/tooling/cli.rs` and `src/generation`
- queue lifecycle policy remains in `src/frame/queue.rs` and is wired from CLI handlers
- context command handlers in `src/tooling/cli.rs` still own orchestration and result mapping

### Sequencing dependencies

- context query and mutation contracts must land before shell cutover can be stable
- context queue and orchestration extraction must happen before removing CLI orchestration logic
- provider contracts should be consumed from provider migration during generation cutover
- agent adapter integration should consume context contracts during agent migration cutover
- config composition facade should be consumed from config migration during shell and startup cutover
- workspace watch runtime hooks should consume context queue and orchestration contracts during shared cutover
- context generation event emission should consume telemetry contracts during shared cutover
- shared shell sequencing should follow CLI migration plan route waves and gates
- shell cutover should align with provider agent and config shared cutover window

### Cross plan dependencies

- this plan depends on provider migration for provider service and port contracts used by context generation
- this plan provides context contracts consumed by agent adapter and command flows
- this plan depends on config migration for composition facade usage in command and startup paths
- this plan provides context queue and orchestration contracts consumed by workspace watch migration
- this plan aligns with telemetry migration for generation event emission contracts
- this plan aligns with config workspace and telemetry migrations at shared CLI cutover and boundary seal stages

### Legacy removal dependencies

- keep temporary wrappers short and remove them in the same phase window after call sites move
- do not keep a long lived domain named `api` after context cutover is complete
- remove duplicated policy from CLI and legacy modules as soon as context services are active

## Difficulty Assessment

### Query and mutation extraction

Difficulty: high

- `src/api.rs` currently mixes read write and lifecycle policy with shared helper types
- extraction touches many callers and a broad test surface
- mitigation is staged extraction with characterization tests for query mutation and lifecycle paths

### Queue and orchestration extraction

Difficulty: high

- orchestration policy runtime lifecycle and queue wiring are split across multiple modules
- primary risk is behavior drift in recursive generation ordering and completion handling
- mitigation is deterministic parity tests for generation summaries ordering and queue waits

### Shell cutover and legacy removal

Difficulty: medium

- context handlers in CLI currently own route orchestration and output shaping concerns
- primary risk is merge churn with provider agent and config shell migrations
- mitigation is one shared cutover window and strict route guard tests

## Migration Phases

1. Characterization baseline
- add parity tests for context get flows by node and by path
- add parity tests for mutation lifecycle flows including tombstone restore and compact
- add parity tests for context generate flows including recursive ordering and skip behavior
- capture route to dependency map for context handlers in CLI

2. Context domain foundation
- add `src/context/mod.rs`
- add `src/context/facade.rs`
- add `src/context/types.rs`
- add module roots for query mutation orchestration and queue areas
- wire initial facade delegation with no behavior change

3. Query and view extraction
- add `src/context/query/service.rs`
- add `src/context/query/types.rs`
- add `src/context/query/view_policy.rs`
- add `src/context/query/composition.rs`
- add `src/context/query/head_queries.rs`
- move query and read helper logic out of `src/api.rs` `src/views.rs` and `src/composition.rs`

4. Mutation and lifecycle extraction
- add `src/context/mutation/service.rs`
- move write and lifecycle policy from `src/api.rs` into context mutation service
- keep deterministic head update and persistence order in one context owner

5. Orchestration and queue extraction
- add `src/context/orchestration/service.rs`
- add `src/context/orchestration/plan.rs`
- add `src/context/queue/runtime.rs`
- add `src/context/queue/request.rs`
- move generation orchestration from CLI and `src/generation` into context orchestration modules
- move queue lifecycle wiring policy from legacy paths into context queue runtime

6. Cross plan integration
- route provider dependent generation flows through provider service and port contracts
- route agent adapter and command dependencies through context facade and context contracts
- keep shared contracts deterministic for CLI text and json paths

7. Shared shell cutover
- reduce context command handlers in `src/tooling/cli.rs` to parse route and output selection only
- remove `build_generation_plan` and context orchestration logic from CLI
- align this cutover with provider agent and config cutover phases

8. Legacy removal and boundary seal
- remove legacy context policy paths from `src/api.rs` and related helper modules
- remove remaining context orchestration and queue policy from legacy locations
- remove the legacy `api` module surface from final domain structure
- enforce context domain contracts to prevent cross domain leakage

## Guardrails

- context domain owns query mutation queue and orchestration policy
- provider domain owns provider services and transport contracts
- agent domain owns adapter contracts and command ownership for agent flows
- config domain owns source loading precedence merge and composition only
- CLI owns parse route and output mapping only
- keep automation facing json field names stable during migration

## Test Strategy

### Behavior parity

- context get parity for node and path selectors
- context generate parity for single target and recursive target flows
- mutation lifecycle parity for tombstone restore and compact outcomes
- queue parity for request dedupe retry and wait completion behavior

### Boundary checks

- route tests confirm one context service call per context command variant
- guard tests confirm CLI does not own context orchestration queue or mutation policy
- guard tests confirm provider integration uses provider contracts only
- guard tests confirm agent integration uses context contracts only

### Contract checks

- deterministic error envelope checks for query mutation and generate failures
- deterministic json field checks for context text and json adapters
- deterministic ordering checks for frame selection and generation summaries

## Exit Criteria

- context concerns are owned by `src/context` modules
- context orchestration and queue lifecycle ownership removed from `src/tooling/cli.rs`
- query and mutation core policy removed from `src/api.rs`
- no long lived domain named `api` remains in final refactor structure
- characterization parity and boundary suites pass

## Risks And Mitigation

- risk: behavior drift in frame selection ordering and generation summaries
- mitigation: characterization snapshots and deterministic ordering assertions

- risk: runtime lifecycle regressions during queue extraction
- mitigation: focused lifecycle tests for start stop and sync wait paths

- risk: merge churn in CLI due shared cutovers across plans
- mitigation: single shared cutover window and command family staging

- risk: boundary leakage from shell or legacy modules back into context policy
- mitigation: route guard tests and ownership checks in review templates
