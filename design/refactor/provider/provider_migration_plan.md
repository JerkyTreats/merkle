# Provider Migration Plan

Date: 2026-02-17

## Objective

Define a migration plan for provider refactor work with clear dependency order, boundary control, and parity gates.

## Scope

This plan covers migration work owned by provider specs in this folder.

- provider domain creation under `src/provider`
- provider config schema and validation ownership handoff
- provider repository extraction for config persistence
- provider diagnostics and command workflow extraction from CLI
- provider client and generation contract extraction

## Related Specs

- [Provider Diagnostics Connectivity Spec](provider_diagnostics_connectivity.md)
- [Config Composition Root Spec](../config/config_composition_root_spec.md)
- [Config Migration Plan](../config/config_migration_plan.md)
- [Agent Migration Plan](../agent/agent_migration_plan.md)
- [Context Migration Plan](../context/context_migration_plan.md)
- [Workspace Migration Guide](../workspace/workspace_migration_guide.md)
- [Telemetry Migration Plan](../telemetry/telemetry_migration_plan.md)
- [Agent Provider Config Management Commands Spec](../agent/agent_provider_config_management_commands.md)
- [CLI Migration Plan](../cli/cli_migration_plan.md)
- [Tooling Diffusion Map](../cli/tooling_diffusion_map.md)
- [Dependency Gate Checklist](../dependency_gate_checklist.md)
- [God Module Detangling Spec](../god_module_detangling_spec.md)
- [Src Module Structure Map](../src_module_structure_map.md)

## Dependency Inventory

### Hard dependencies

- provider types and validation still live in `src/config.rs`
- provider registry persistence still lives in `src/provider.rs`
- provider status validate and test command workflows still live in `src/tooling/cli.rs`
- provider transport clients and registry aggregate still live in one monolithic file `src/provider.rs`
- config and CLI callers currently depend on `crate::config` provider types

### Sequencing dependencies

- provider domain model and validation contracts must land before config composition delegation can stabilize
- provider repository ports must land before command and diagnostics service extraction
- diagnostics extraction should align with command service extraction to share one connectivity policy owner
- context generation paths should consume provider ports and services during context migration cutover
- workspace status ownership split should consume provider contracts during workspace migration
- provider command summary emission should delegate to telemetry summary mapper contracts during CLI cutover
- shared shell sequencing should follow CLI migration plan route waves and gates
- CLI shell slimming is last and should align with agent config and workspace shell cutovers

### Cross plan dependencies

- this plan provides provider domain contracts consumed by config migration
- this plan provides provider services consumed by agent migration integration phases
- this plan provides provider service and port contracts consumed by context migration generation orchestration phases
- this plan provides provider status and diagnostics contracts consumed by workspace migration
- this plan aligns with telemetry migration for provider summary and diagnostics event contracts
- wrapper removal should happen in one shared cutover window with agent config and workspace plans

## Difficulty Assessment

### Provider contract extraction

Difficulty: high

- provider schema and validation are referenced by config loader registry and CLI formatting paths
- contract move touches config provider and shell modules in one wave
- mitigation is staged extraction with parity tests for validation and output contracts

### Diagnostics and command workflow extraction

Difficulty: high

- provider status validate test create edit and remove flows are mixed with shell routing
- connectivity policy and runtime creation logic are duplicated across command handlers
- mitigation is one provider application owner for diagnostics and one for command workflows

### Client and generation contract extraction

Difficulty: medium

- transport clients already exist but are co located with registry and persistence concerns
- generation call paths need a stable provider port contract for client resolution
- mitigation is incremental port adoption with characterization tests on client behavior

## Migration Phases

1. Characterization baseline
- add parity tests for provider list show status validate test and config command outputs in text and json
- add parity tests for provider client creation and model listing behavior
- capture current provider route to dependency map for shell handlers

2. Provider domain foundation
- add `src/provider/domain/mod.rs`
- add `src/provider/domain/config.rs`
- add `src/provider/domain/validation.rs`
- move provider schema and validation ownership out of `src/config.rs` into provider domain
- update call sites in provider and CLI modules to use provider domain contracts

3. Provider repository extraction
- add `src/provider/ports/repository.rs`
- add `src/provider/infra/repository/xdg.rs`
- move provider load save delete and path policies out of `ProviderRegistry`
- route persistence through repository port from provider services and registry owners

4. Diagnostics service extraction
- add `src/provider/application/diagnostics_service.rs`
- move connectivity and model checks out of CLI handlers
- centralize timeout and error mapping policy for status validate and test flows

5. Command service extraction
- add `src/provider/application/command_service.rs`
- move create edit remove validate and status orchestration out of CLI handlers
- move post mutation reload policy into provider command service

6. Client and generation contract extraction
- add `src/provider/ports/client.rs`
- add `src/provider/application/generation_service.rs`
- split provider client implementations into `src/provider/infra/clients`
- route generation facing provider resolution through provider ports and generation service

7. Cross plan integration and CLI cutover
- wire config composition paths to provider domain contracts and repository port only
- route agent provider dependencies to provider application services and ports
- route context generation dependencies to provider application services and ports
- reduce provider command handlers in `src/tooling/cli.rs` to parse route and output selection behavior

8. Wrapper removal and boundary seal
- remove legacy provider orchestration exports from `src/provider.rs`
- remove direct provider persistence and diagnostics logic from CLI code paths
- enforce provider port usage for cross domain callers

## Guardrails

- provider domain owns provider schema validation repository and diagnostics policy
- config domain owns source loading precedence merge and composition only
- CLI shell owns parse route and output mapping only
- use one owner per concern and remove duplicated policy quickly at each phase boundary
- keep automation facing json field names stable during migration

## Test Strategy

### Behavior parity

- provider list show status validate test parity in text and json
- provider create edit remove parity for command responses and side effects
- provider client model listing parity across provider types

### Boundary checks

- route tests confirm CLI makes one provider service call per command variant
- guard tests confirm CLI does not call provider repository writes directly
- guard tests confirm config and agent callers use provider contracts rather than provider internals

### Contract checks

- deterministic validation error envelope checks
- deterministic diagnostics and command json field checks
- deterministic repository path resolution and filename policy checks

## Exit Criteria

- provider schema and validation ownership moved to `src/provider/domain`
- provider persistence moved to provider repository port and XDG adapter
- provider diagnostics and command orchestration removed from `src/tooling/cli.rs`
- config and agent callers consume provider contracts and services only
- characterization parity and boundary suites pass

## Risks And Mitigation

- risk: provider output drift in text and json responses
- mitigation: characterization snapshots and parity tests before and after each phase

- risk: connectivity behavior drift across status validate and test flows
- mitigation: shared diagnostics service owner with deterministic timeout policy tests

- risk: cross domain boundary regression after cutover
- mitigation: guard tests for provider port usage and review ownership checks

- risk: merge churn in `src/tooling/cli.rs` during staged extraction
- mitigation: move by provider command family and keep shell handlers thin
