# Config Migration Plan

Date: 2026-02-17

## Objective

Define a migration plan for config refactor work with clear dependency order, boundary control, and parity gates.

## Scope

This plan covers migration work owned by config specs in this folder.

- config composition root extraction from `src/config.rs`
- source loading and precedence migration into config composition service
- workspace storage path and XDG path ownership cleanup
- integration with provider and agent domain config contracts
- CLI and startup cutover to config composition contracts only

## Related Specs

- [Config Composition Root Spec](config_composition_root_spec.md)
- [Provider Migration Plan](../provider/provider_migration_plan.md)
- [Agent Migration Plan](../agent/agent_migration_plan.md)
- [Context Migration Plan](../context/context_migration_plan.md)
- [Workspace Migration Guide](../workspace/workspace_migration_guide.md)
- [Telemetry Migration Plan](../telemetry/telemetry_migration_plan.md)
- [Agent Provider Config Management Commands Spec](../agent/agent_provider_config_management_commands.md)
- [CLI Migration Plan](../cli/cli_migration_plan.md)
- [Tooling Diffusion Map](../cli/tooling_diffusion_map.md)
- [Dependency Gate Checklist](../dependency_gate_checklist.md)
- [Src Module Structure Map](../src_module_structure_map.md)
- [God Module Detangling Spec](../god_module_detangling_spec.md)

## Dependency Inventory

### Hard dependencies

- source loading precedence merge and domain schema are still mixed in `src/config.rs`
- startup and command routes load config through `ConfigLoader` in `src/bin/merkle.rs` and `src/tooling/cli.rs`
- provider and agent callers still reference `crate::config` policy and helpers directly
- workspace storage and XDG helper policy still share one mixed module surface

### Sequencing dependencies

- provider domain foundation and provider repository extraction must complete before config contract adoption
- agent domain foundation and agent repository extraction must complete before config contract adoption
- characterization must land before source adapter extraction so precedence and merge behavior stay stable
- context shell cutover should consume config composition facade rather than direct source loading policy
- workspace lifecycle and watch startup cutover should consume config composition facade and path modules
- shared CLI startup and command cutover should align with telemetry migration cutover windows
- shared shell sequencing should follow CLI migration plan route waves and gates
- shell and startup cutover is last so one composition entry point is enforced at the end

### Cross plan dependencies

- this plan consumes provider domain contracts produced by provider migration
- this plan consumes agent domain contracts produced by agent migration
- this plan provides one composition facade consumed by context migration command and startup cutover phases
- this plan provides one composition facade consumed by provider and agent CLI cutover phases
- this plan provides one composition facade consumed by workspace migration lifecycle and watch startup phases
- this plan aligns with telemetry migration at shared CLI startup and command cutover stages

### Cutover dependencies

- cut over call sites in one phase and remove legacy access in the same phase
- do not keep long lived wrappers for moved merge and source loading policy
- remove duplicated config policy from shell and domain modules as soon as composition paths are active

## Difficulty Assessment

### Composition boundary extraction

Difficulty: high

- `ConfigLoader` currently owns source loading and also mixed policy used by multiple domains
- composition extraction requires coordinated call site updates in startup CLI and domains
- mitigation is phased extraction with parity matrix for source ordering and merge outcomes

### Cross domain contract adoption

Difficulty: medium

- config migration depends on provider and agent contract stability from separate plans
- primary risk is temporary duplication when old `crate::config` paths coexist with new domain contracts
- mitigation is prerequisite gates and short lived compatibility windows

### Path policy migration

Difficulty: medium

- XDG and storage path helpers are shared by startup CLI prompt and domain persistence flows
- primary risk is path resolution drift for storage and prompt locations
- mitigation is deterministic path tests for default and overridden homes

## Migration Phases

1. Characterization baseline
- add parity tests for precedence and merge behavior across workspace file global file and environment
- add parity tests for validation envelopes consumed from provider and agent contracts
- add path parity tests for storage resolution and XDG helper behavior

2. Cross plan prerequisite gate
- confirm provider domain foundation and repository extraction milestones are complete
- confirm agent domain foundation and repository extraction milestones are complete
- lock shared provider and agent config contract fixtures used by composition tests

3. Domain contract adoption in config
- remove provider and agent schema ownership from `src/config.rs`
- import provider and agent domain config contracts into config composition paths
- update startup and CLI call sites to consume domain owned contracts through config composition

4. Config source adapter extraction
- add `src/config/sources/workspace_file.rs`
- add `src/config/sources/global_file.rs`
- add `src/config/sources/environment.rs`
- move source specific parsing and read errors out of monolithic loader paths

5. Config composition service extraction
- add `src/config/composition/service.rs`
- add `src/config/composition/merge_policy.rs`
- slim `ConfigLoader` into composition facade logic only
- keep domain validation delegated to provider and agent contracts

6. Workspace path and XDG module extraction
- add `src/config/workspace/storage_paths.rs`
- add `src/config/paths/xdg_root.rs`
- move workspace storage resolution and XDG root helpers out of mixed config module paths
- update prompt and config callers to use new path modules

7. Shared shell and startup cutover
- update `src/bin/merkle.rs` and `src/tooling/cli.rs` to call config composition facade only
- remove direct config merge and source policy from CLI command handlers
- align this cutover window with provider agent and context CLI cutover phases

8. Legacy removal and boundary seal
- remove dead legacy config paths from `src/config.rs`
- remove duplicate config helpers from non config modules
- enforce domain contracts to prevent reintroduction of cross domain leakage

## Guardrails

- keep config domain focused on source loading precedence merge and composition only
- keep provider and agent domains responsible for schema validation and repository policy
- keep shell ownership limited to parse route and output mapping
- keep json field names stable for automation facing outputs
- remove duplicate policy at phase boundaries instead of carrying parallel paths

## Test Strategy

### Behavior parity

- precedence parity for workspace file global file and environment source ordering
- merge parity for defaults override rules and conflict outcomes
- provider and agent validation parity using domain contract fixtures
- startup and command load parity through composition facade paths

### Boundary checks

- guard tests confirm config domain does not own provider or agent validation rules
- guard tests confirm provider and agent domains do not own source precedence logic
- route tests confirm CLI and binary startup call composition facade rather than source internals

### Contract checks

- deterministic error envelope checks for source read parse merge and domain validation failures
- deterministic json field checks for config driven status and validate outputs
- deterministic path resolution checks for storage dirs prompt paths and XDG homes

## Exit Criteria

- config domain is composition root only under `src/config`
- provider and agent contracts are consumed from domain modules only
- CLI and startup use one config composition facade path
- legacy mixed concerns are removed from `src/config.rs`
- characterization parity and boundary suites pass

## Risks And Mitigation

- risk: precedence drift during source adapter split
- mitigation: characterization matrix and deterministic merge snapshots

- risk: contract mismatch with provider or agent migration outputs
- mitigation: prerequisite gate checks and shared contract fixtures

- risk: path behavior drift for prompt and storage locations
- mitigation: path resolution tests with default and overridden XDG homes

- risk: shell regains config orchestration logic
- mitigation: route guard tests and boundary checks in review templates
