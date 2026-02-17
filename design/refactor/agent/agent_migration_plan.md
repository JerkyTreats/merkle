# Agent Migration Plan

Date: 2026-02-17

## Objective

Define a migration plan for agent refactor work with clear dependency order, risk control, and parity gates.

## Scope

This plan covers migration work owned by agent specs in this folder.

- agent domain creation for config schema validation and repository policy
- agent to context adapter ownership migration
- agent config command workflow extraction
- command handler slimming in CLI shell
- integration with provider and config migration cutovers

## Related Specs

- [Agent Context Adapter Boundary Spec](agent_context_adapter_boundary_spec.md)
- [Agent Provider Config Management Commands Spec](agent_provider_config_management_commands.md)
- [Provider Migration Plan](../provider/provider_migration_plan.md)
- [Config Migration Plan](../config/config_migration_plan.md)
- [Context Migration Plan](../context/context_migration_plan.md)
- [Workspace Migration Guide](../workspace/workspace_migration_guide.md)
- [Telemetry Migration Plan](../telemetry/telemetry_migration_plan.md)
- [Config Composition Root Spec](../config/config_composition_root_spec.md)
- [CLI Migration Plan](../cli/cli_migration_plan.md)
- [Tooling Diffusion Map](../cli/tooling_diffusion_map.md)
- [Context Domain Structure Spec](../context/context_domain_structure.md)
- [Dependency Gate Checklist](../dependency_gate_checklist.md)
- [God Module Detangling Spec](../god_module_detangling_spec.md)

## Dependency Inventory

### Hard dependencies

- CLI command handlers currently own agent command orchestration in `src/tooling/cli.rs`
- agent config schema and validation still live in `src/config.rs`
- agent config persistence paths are still static helpers in `src/agent.rs`
- adapter contract and implementation still live in `src/tooling/adapter.rs`

### Sequencing dependencies

- agent domain config and validation contracts must land before config composition can delegate agent policy
- agent repository extraction must happen before agent command service extraction
- provider command and diagnostics services should be consumed from provider migration rather than rebuilt here
- context query mutation and orchestration contracts should be consumed from context migration during adapter and CLI cutover
- workspace status and shared shell cutover should align with workspace migration
- agent command summary emission should delegate to telemetry summary mapper contracts during CLI cutover
- shared shell sequencing should follow CLI migration plan route waves and gates
- CLI shell slimming is last and should align with provider config and workspace shell cutovers

### Cross plan dependencies

- this plan depends on provider migration for provider command and diagnostics service ownership
- this plan depends on context migration for context facade and service contracts used by adapter paths
- this plan provides agent domain contracts and repository ports consumed by config migration
- this plan aligns with workspace migration for unified status section ownership and shared CLI cutover
- this plan aligns with telemetry migration for command summary and session emission contracts
- wrapper removal should happen in one shared cutover window with provider config and workspace plans

### Compatibility dependencies

- `src/tooling/adapter.rs` remains wrapper until all call sites move
- `src/tooling/cli.rs` remains wrapper surface until route and presentation modules absorb behavior
- agent registry public methods remain available during staged repository delegation

## Difficulty Assessment

### Agent domain contract migration

Difficulty: high

- agent schema and validation are referenced by config loader registry and CLI command paths
- ownership handoff touches both domain and shell call sites
- mitigation is staged move with parity tests for validation and command output contracts

### Adapter boundary migration

Difficulty: medium

- coupling is concentrated and bounded to adapter imports and a small set of call sites
- primary risk is queue wait timeout drift in generate flow
- mitigation is characterization for read write generate parity before move

### Command workflow migration

Difficulty: medium

- command surface spans create edit remove validate and status flows in one CLI module
- direct file path and save delete calls are spread across command and formatting paths
- mitigation is phased extraction with strict service and repository boundaries

## Migration Phases

1. Characterization baseline
- add parity tests for adapter read write generate behavior
- add parity tests for agent command text and json outputs
- add parity tests for unified status agent section behavior
- capture current route to dependency map for agent handlers

2. Agent domain foundation
- add `src/agent/domain/config.rs`
- add `src/agent/domain/validation.rs`
- move agent schema and validation ownership out of `src/config.rs`
- update `src/agent.rs` and CLI call sites to use agent domain contracts

3. Agent repository extraction
- add `src/agent/ports/repository.rs`
- add `src/agent/infra/repository/xdg.rs`
- move agent file load save delete and prompt path policy behind agent repository port
- keep existing registry static methods as short delegating wrappers during migration

4. Agent command service extraction
- add `src/agent/application/command_service.rs`
- move create edit remove validate and status workflows into service
- move post mutation reload policy into service

5. Adapter ownership extraction
- add `src/agent/ports/context_adapter.rs`
- add `src/agent/adapters/context_api.rs`
- move `AgentAdapter` and `ContextApiAdapter` out of `src/tooling/adapter.rs`
- keep `src/tooling/adapter.rs` as compatibility reexport wrapper

6. Cross plan integration and CLI cutover
- route provider dependent flows to provider application services from provider migration
- route context adapter and command dependencies through context facade and context service contracts from context migration
- route config loads through config composition facade from config migration
- reduce agent command handlers in `src/tooling/cli.rs` to parse route and output selection behavior

7. Wrapper removal and boundary seal
- remove legacy adapter exports from `src/tooling.rs`
- remove legacy agent command orchestration code paths from `src/tooling/cli.rs`
- remove legacy agent persistence wrappers after caller migration
- enforce domain contracts to prevent reintroduction of cross domain leakage

## Guardrails

- preserve behavior with compatibility wrappers until parity suite is green
- avoid cross domain internal access from CLI adapter and service callers
- use one owner per concern across domain repository service and adapter layers
- keep json field names stable for automation facing outputs

## Test Strategy

### Behavior parity

- adapter read write generate parity
- agent create edit remove validate and status parity
- unified status parity for agent section outputs

### Boundary checks

- CLI route tests confirm one service call per agent command variant
- guard tests confirm CLI does not call agent repository writes directly
- guard tests confirm provider and config dependencies are consumed through domain contracts only

### Contract checks

- deterministic error mapping checks for agent command service
- deterministic timeout behavior checks for adapter generate path
- deterministic json field checks for agent command and status outputs

## Exit Criteria

- agent schema and validation ownership moved to `src/agent/domain`
- agent repository ownership moved to `src/agent/ports` and `src/agent/infra`
- adapter ownership moved to `src/agent/ports` and `src/agent/adapters`
- agent command orchestration removed from `src/tooling/cli.rs`
- compatibility wrappers removed after caller migration and passing parity suite

## Risks And Mitigation

- risk: output drift in text and json command responses
- mitigation: snapshot and parity tests before and after each phase

- risk: runtime behavior drift in adapter generate flow
- mitigation: targeted characterization tests for timeout and queue wait paths

- risk: migration merge conflicts in `src/tooling/cli.rs`
- mitigation: phase by command family and keep wrappers thin

- risk: boundary drift between provider config and agent services
- mitigation: cross plan route guard tests and ownership checks
