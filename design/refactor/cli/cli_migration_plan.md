# CLI Migration Plan

Date: 2026-02-17

## Objective

Define a migration plan for CLI refactor work with clear dependency order, boundary control, and parity gates.

## Scope

This plan covers migration work owned by CLI specs in this folder.

- CLI domain creation under `src/cli`
- parse route help and output envelope extraction from `src/tooling/cli.rs`
- command handler slimming for workspace agent provider context and status command families
- startup context creation and command execution policy cutover for config and telemetry integrations
- legacy `tooling` CLI wrapper and export removal after shared cutover

## Related Specs

- [CLI Shell Parse Route Help Spec](cli_shell_parse_route_help_spec.md)
- [CLI Presentation Formatting Spec](cli_presentation_formatting.md)
- [Tooling Diffusion Map](tooling_diffusion_map.md)
- [Agent Migration Plan](../agent/agent_migration_plan.md)
- [Provider Migration Plan](../provider/provider_migration_plan.md)
- [Config Migration Plan](../config/config_migration_plan.md)
- [Context Migration Plan](../context/context_migration_plan.md)
- [Workspace Migration Guide](../workspace/workspace_migration_guide.md)
- [Telemetry Migration Plan](../telemetry/telemetry_migration_plan.md)
- [Telemetry Event Engine Spec](../telemetry/telemetry_event_engine_spec.md)
- [Workspace Lifecycle Services Spec](../workspace/workspace_lifecycle_services.md)
- [Dependency Gate Checklist](../dependency_gate_checklist.md)
- [God Module Detangling Spec](../god_module_detangling_spec.md)
- [Src Module Structure Map](../src_module_structure_map.md)

## Dependency Inventory

### Hard dependencies

- clap command enums and top level CLI argument contracts are still defined in `src/tooling/cli.rs`
- `CliContext::new` still owns config load path resolution registry startup and progress runtime startup
- `execute` still owns session lifecycle and summary emission orchestration
- workspace agent provider context and unified status handlers still own domain orchestration logic
- text and json formatter helpers still live inline in the CLI file
- `src/bin/merkle.rs` still imports `merkle::tooling::cli` and builds logging config from CLI arguments
- `src/tooling.rs` still exports `Cli` `CliContext` and `Commands` through the legacy `tooling` surface

### Sequencing dependencies

- parse help route and presentation extraction can begin after characterization baseline
- workspace route cutover must wait for workspace lifecycle status and watch service contracts
- agent and provider route cutover must wait for agent and provider command service contracts
- context route cutover must wait for context query mutation and orchestration facade contracts
- execute lifecycle cutover must wait for telemetry session and summary mapper contracts
- startup and config path cutover must wait for config composition facade and path contract extraction
- shared shell cutover should follow route wave sequencing and gates from this plan across agent provider config context workspace and telemetry plans

### Cross plan dependencies

- this plan consumes workspace contracts produced by workspace migration
- this plan consumes agent and provider command contracts produced by agent and provider migrations
- this plan consumes context facade contracts produced by context migration
- this plan consumes composition contracts produced by config migration
- this plan consumes session and emission contracts produced by telemetry migration
- this plan provides route and output boundary contracts consumed by all command family migrations

### Cutover dependencies

- each command family should move in one route wave and drop legacy orchestration in the same wave
- `src/tooling/cli.rs` can remain as a thin wrapper during staged route waves
- remove legacy wrapper and `tooling` exports only after all route families run through `src/cli` entrypoints
- avoid long lived duplicate route tables across legacy and new CLI modules

## Difficulty Assessment

### Parse help and presentation extraction

Difficulty: medium

- parsing and help concerns are well scoped but currently mixed with route and orchestration code
- formatter helpers have broad output surface and many automation facing fields
- mitigation is snapshot driven extraction before route and service cutovers

### Command route and orchestration cutover

Difficulty: high

- one file currently mixes workspace agent provider context status and init command families
- each family has different dependency readiness and different parity surface
- mitigation is route wave staging by command family with strict dependency gates

### Startup and execution policy cutover

Difficulty: high

- `CliContext::new` currently blends composition runtime bootstrap and registry policy
- execute path currently blends telemetry session policy with command dispatch
- mitigation is separate startup and execution seams plus explicit contract tests before removal

## Migration Phases

1. Characterization baseline
- add parse matrix tests for top level and nested commands plus invalid argument cases
- add help snapshot tests for top level and nested `--help` outputs
- add command output snapshots for text and json envelopes on representative command families
- capture command route to dependency map for current handlers and helper functions

2. CLI domain foundation
- add `src/cli/mod.rs`
- add `src/cli/parse.rs`
- add `src/cli/route.rs`
- add `src/cli/help.rs`
- add `src/cli/output.rs`
- add `src/cli/presentation/mod.rs` and presentation submodules
- keep `src/tooling/cli.rs` as short delegating wrapper during migration

3. Parse and help extraction
- move clap command declarations into `src/cli/parse.rs`
- move help text ownership and tests into `src/cli/help.rs`
- keep binary call sites stable through wrapper exports while imports migrate

4. Presentation extraction
- move text and json formatter helpers into `src/cli/presentation`
- keep output mode selection in CLI route layer only
- keep json field names and section ordering stable for automation compatibility

5. Route contract extraction
- define one route table in `src/cli/route.rs`
- enforce one command variant to one service call mapping
- move command specific error mapping to `src/cli/output.rs` and keep route handlers thin

6. Execute lifecycle seam extraction
- add CLI execution wrapper that delegates telemetry session start finish prune and summary mapping
- move execute policy out of command handlers and into telemetry facade calls
- keep command completion behavior stable when telemetry emission fails

7. Command family route waves
- wave one moves workspace command routes after workspace lifecycle and status services are ready
- wave two moves agent and provider command routes after command services are ready
- wave three moves context command routes after context query mutation and orchestration contracts are ready
- wave four moves unified status assembly after workspace agent and provider status contracts are ready
- wave five moves watch and init command routes after workspace watch and config startup dependencies are ready

8. Startup and binary cutover
- split CLI argument surface from runtime bootstrap responsibilities
- route config and path loading through config composition facade contracts
- update `src/bin/merkle.rs` imports from `merkle::tooling::cli` to `merkle::cli` once wrappers are ready

9. Shared shell cutover
- align final route swaps with agent provider config context workspace and telemetry shared cutover windows
- keep one owner per command family route after each wave
- block merge of new orchestration logic into CLI boundary modules during this window

10. Legacy removal and boundary seal
- remove legacy `src/tooling/cli.rs` orchestration and formatter code paths
- remove `tooling` CLI exports from `src/tooling.rs` and update `src/lib.rs` exports
- remove stale helper methods and route tables from legacy modules
- enforce CLI boundary rules in tests and review checks

## Guardrails

- CLI owns parse route help output envelope and boundary error mapping only
- domain services own business orchestration persistence and generation policies
- telemetry failures remain best effort and do not block command completion
- json field stability is required for automation facing outputs
- remove duplicate CLI logic at each phase boundary instead of carrying parallel owners

## Test Strategy

### Behavior parity

- parse and help parity across top level and nested command families
- route parity for workspace agent provider context status watch and init commands
- output parity for text and json envelopes across representative commands
- execute parity for telemetry session ordering and command summary emission

### Boundary checks

- route tests confirm one service call per command variant
- guard tests confirm CLI does not call repository writes directly
- guard tests confirm CLI does not own context queue provider diagnostics or workspace watch runtime policy
- guard tests confirm startup path uses config composition facade contracts only

### Contract checks

- deterministic json field and key presence checks for automation facing outputs
- deterministic error envelope checks for validation not found and runtime failures
- deterministic command summary payload checks after telemetry delegation
- deterministic command output ordering checks for list and status responses

## Exit Criteria

- CLI ownership is fully under `src/cli` modules
- `src/tooling/cli.rs` is removed or reduced to compatibility free reexports only
- `src/bin/merkle.rs` imports CLI from final module ownership
- CLI handlers own parse route output and boundary error mapping only
- no command family orchestration remains in CLI boundary modules
- characterization parity boundary and contract suites pass

## Risks And Mitigation

- risk: route drift during staged command family waves
- mitigation: one route table owner and command variant coverage tests

- risk: output drift for automation facing json fields
- mitigation: snapshot and explicit field presence tests before and after each wave

- risk: startup behavior drift during composition cutover
- mitigation: startup contract tests with default and overridden config paths

- risk: merge churn in shared shell cutover windows
- mitigation: dependency gates and command family wave sequencing
