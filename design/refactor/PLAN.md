# Refactor Migration Phased Development Plan

Date: 2026-02-17

## Overview

This plan converts the per domain migration docs into one durable execution order.

The order is dependency driven, not folder driven.

- First establish contract foundations and repository ownership
- Then complete shared composition and telemetry policy ownership
- Then move context and workspace domain owners
- Then cut over CLI routes in planned waves
- Then remove legacy surfaces in the same phase window

Related docs:
- [God Module Detangling Spec](god_module_detangling_spec.md)
- [Src Module Structure Map](src_module_structure_map.md)
- [Phase 1 Implementation Plan](phase1_implementation_plan.md)
- [Phase 2 Implementation Plan](phase2_implementation_plan.md)
- [Dependency Gate Checklist](dependency_gate_checklist.md)
- [CLI Migration Plan](cli/cli_migration_plan.md)
- [Provider Migration Plan](provider/provider_migration_plan.md)
- [Agent Migration Plan](agent/agent_migration_plan.md)
- [Config Migration Plan](config/config_migration_plan.md)
- [Context Migration Plan](context/context_migration_plan.md)
- [Workspace Migration Guide](workspace/workspace_migration_guide.md)
- [Telemetry Migration Plan](telemetry/telemetry_migration_plan.md)

---

## Migration rules

Apply these rules when implementing domain extraction and refactors.

| Rule | Statement |
|------|-----------|
| Behavior over layer | Name submodules by what they do, not by technical layer. Prefer `profile`, `storage`, `identity`, `registry` over `domain`, `repository`, `infra`, `ports`. |
| Behavior over pattern | Name modules and types by behavior, not by design pattern. Prefer `storage` over `repository`, `XdgAgentStorage` over `XdgAgentRepository`. |
| Model separate from aggregate | Extract shared types into a dedicated module. Keep aggregates focused on their behavior. Example: `identity` holds AgentRole, AgentIdentity; `registry` holds the in-memory aggregate. |
| Domain owns schema | The domain that defines a concept owns its schema. Config consumes via re-export; it does not define or duplicate domain types. |
| Parent file convention | Use `parent.rs` plus `parent/child.rs` instead of `parent/mod.rs`. Rust 2018+ module style. |
| Align with sibling domains | When naming, match sibling domains for consistency. Example: `profile` for config shape aligns provider and agent. |

---

## Development phases

| Phase | Goal | Dependencies | Completion |
|-------|------|--------------|------------|
| 1 | Characterization baseline and shared gates | None | Completed local |
| 2 | Provider foundation and repository ownership | Phase 1 | Completed local |
| 3 | Agent foundation and repository ownership | Phase 1, Phase 2 | Completed local |
| 4 | Config composition root and path ownership | Phase 2, Phase 3 | Completed local |
| 5 | Telemetry foundation and policy services | Phase 1 | Completed local |
| 6 | Context query mutation generation and queue ownership | Phase 2, Phase 4, Phase 5 | In progress |
| 7 | Provider and agent command workflows plus adapter cutover | Phase 2, Phase 3, Phase 4, Phase 6 | Planned |
| 8 | Workspace lifecycle status and watch ownership | Phase 4, Phase 5, Phase 6, Phase 7 | Planned |
| 9 | CLI route waves and startup execution cutover | Phase 4, Phase 5, Phase 6, Phase 7, Phase 8 | Planned |
| 10 | Legacy removal and boundary seal | Phase 1 to Phase 9 | Planned |

---

### Phase 1 — Characterization baseline and shared gates

| Field | Value |
|-------|--------|
| Goal | Lock behavior and output contracts before extraction work. |
| Dependencies | None |
| Docs | phase1_implementation_plan.md and all migration plans in this folder |
| Completion | Completed local |

| Task | Completion |
|------|------------|
| Add parity suites for parse help route output and command summaries. | Completed |
| Add parity suites for provider agent context workspace telemetry command families in text and json. | Completed |
| Add deterministic ordering checks for status list watch and generation outputs. | Completed |
| Publish one dependency gate checklist used by every phase below. | Completed |

| Exit criterion | Completion |
|----------------|------------|
| Baseline parity suites are green and stable in CI. | Pending CI |
| Dependency gate checklist is published and referenced by all migration streams. | Completed |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Unblocks safe extraction work by freezing current behavior and output contracts. | Completed local |

---

### Phase 2 — Provider foundation and repository ownership

| Field | Value |
|-------|--------|
| Goal | Make provider domain the owner of provider schema validation repository and client ports. |
| Dependencies | Phase 1 |
| Docs | provider/provider_migration_plan.md and phase2_implementation_plan.md |
| Completion | Completed local |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Move provider schema and validation ownership from config into provider domain modules. | Completed local |
| 2 | Extract provider repository port and XDG adapter, and route persistence through that port. | Completed local |
| 3 | Extract diagnostics and command services for status validate test create edit remove flows. | Completed local |
| 4 | Extract provider client port and generation service contracts for context use. | Completed local |
| 5 | Remove legacy provider persistence and diagnostics ownership from old paths in the same phase window. | Completed local |

| Exit criterion | Completion |
|----------------|------------|
| Provider contracts required by config context workspace and CLI are available and tested. | Completed local |
| Provider persistence and diagnostics no longer rely on mixed legacy ownership. | Completed local |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Satisfies provider prerequisites required by config adoption and context generation integration. | Completed local |

#### Phase 2 — Provider implementation notes

Applied migration rules. Resulting structure: `provider.rs` parent plus `provider/profile.rs` and `profile/`, `provider/storage.rs` and `storage/`, `provider/clients.rs` and `clients/`, `provider/commands.rs`, `provider/diagnostics.rs`, `provider/generation.rs`. Uses `parent.rs` plus `parent/child.rs` convention throughout. Storage replaces repository per behavior-over-pattern rule. Profile owns schema; XdgProviderStorage implements ProviderStorage.

---

### Phase 3 — Agent foundation and repository ownership

| Field | Value |
|-------|--------|
| Goal | Make agent domain the owner of agent schema validation and repository policy. |
| Dependencies | Phase 1, Phase 2 |
| Docs | agent/agent_migration_plan.md |
| Completion | Completed local |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Move agent schema and validation ownership from config into agent domain modules. | Completed local |
| 2 | Extract agent repository port and XDG adapter for load save delete and prompt path policy. | Completed local |
| 3 | Keep registry focused on in memory aggregate behavior only. | Completed local |
| 4 | Remove legacy mixed ownership from old paths in the same phase window. | Completed local |

| Exit criterion | Completion |
|----------------|------------|
| Agent contracts required by config and CLI migration are available and tested. | Completed local |
| Agent persistence paths are owned by agent repository port and adapter modules. | Completed local |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Satisfies agent prerequisites required by config composition adoption. | Completed local |

#### Phase 3 — Agent implementation notes

Applied Migration rules. Resulting structure: `agent.rs` plus `agent/profile.rs`, `agent/identity.rs`, `agent/storage.rs`, `agent/prompt.rs`, `agent/registry.rs`. Config re-exports AgentConfig from agent; profile owns the schema.

---

### Phase 4 — Config composition root and path ownership

| Field | Value |
|-------|--------|
| Goal | Reduce config domain to one composition root with source precedence merge and path composition only. |
| Dependencies | Phase 2, Phase 3 |
| Docs | config/config_migration_plan.md |
| Completion | Completed local |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Adopt provider and agent domain contracts in config composition paths. | Completed local |
| 2 | Extract config sources by behavior for workspace file global file and environment. | Completed local |
| 3 | Extract composition service and merge policy modules. | Completed local |
| 4 | Extract workspace storage path and XDG root modules. | Completed local |
| 5 | Remove direct config policy ownership from CLI and non config modules in the same phase window. | Completed local |

| Exit criterion | Completion |
|----------------|------------|
| One config composition facade is available for startup and command paths. | Completed local |
| Provider and agent validation policy is no longer owned by config modules. | Completed local |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Provides config facade and path contracts required by context workspace and CLI cutovers. | Completed local |

#### Phase 4 — Implementation notes

Applied Migration rules. Resulting structure: `config.rs` parent plus `config/facade.rs`, `config/sources.rs` and `sources/`, `config/merge.rs` and `merge/`, `config/workspace.rs` and `workspace/`, `config/paths.rs` and `paths/`. ConfigLoader delegates to MergeService; XDG helpers in paths/xdg_root; StorageConfig in workspace/storage_paths. Duplicate resolve_prompt_path and PromptCache removed from config.

---

### Phase 5 — Telemetry foundation and policy services

| Field | Value |
|-------|--------|
| Goal | Move telemetry contracts routing sinks sessions and summary mapping into telemetry domain ownership. |
| Dependencies | Phase 1 |
| Docs | telemetry/telemetry_migration_plan.md |
| Completion | Completed local |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Add telemetry domain root facade and shared types. | Completed local |
| 2 | Extract event and summary contracts. | Completed local |
| 3 | Extract routing and sink ownership from legacy progress modules. | Completed local |
| 4 | Extract session lifecycle service and policy from CLI execution paths. | Completed local |
| 5 | Extract emission engine and summary mapper from CLI handlers. | Completed local |
| 6 | Remove legacy telemetry policy ownership in the same phase window. | Completed local |

| Exit criterion | Completion |
|----------------|------------|
| Telemetry contracts and services are stable and consumed through telemetry facade only. | Completed local |
| CLI execute path no longer owns session lifecycle or summary mapping policy. | Completed local |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Provides telemetry contracts needed by context generation hooks workspace watch hooks and CLI cutover. | Completed local |

#### Phase 5 — Implementation notes

Telemetry domain under `src/telemetry.rs` and `src/telemetry/` owns events, sessions, routing, sinks, emission, and summary. Facade re-exports ProgressRuntime, PrunePolicy, SessionStatus, event types, new_session_id, and now_millis. All call sites in CLI, frame queue, generation orchestrator, and watch use `crate::telemetry` only. Legacy `src/progress` removed; Phase 10 task 3 completed in same window.

---

### Phase 6 — Context query mutation generation and queue ownership

| Field | Value |
|-------|--------|
| Goal | Move context query mutation generation queue and frame model and storage into context domain ownership. Behavior-named structure: frame query mutation generation queue under `src/context`. |
| Dependencies | Phase 2, Phase 4, Phase 5 |
| Docs | context/context_migration_plan.md, context/context_domain_structure.md |
| Completion | In progress |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Add context domain root facade and shared types. | Completed local |
| 2 | Move `src/frame` into `src/context/frame` so frame model storage set and id are owned by context domain; queue runtime lands at `src/context/queue`. | Completed local |
| 3 | Extract query service view policy composition and head queries. | In progress; query service and view_policy under `src/context/query`; composition and head_queries pending |
| 4 | Extract mutation and lifecycle service from legacy API paths with deterministic update order. | Planned |
| 5 | Extract generation plan and executor into `src/context/generation` and queue runtime into `src/context/queue`; remove from CLI and legacy modules. | Completed local |
| 6 | Route provider dependent generation through provider contracts and services from Phase 2. | Completed local |
| 7 | Route telemetry generation events through telemetry contracts from Phase 5. | Completed local |
| 8 | Remove legacy context policy and top-level `src/frame` from old modules in the same phase window. | Completed local for frame and generation; no top-level `src/frame` or `src/generation`; legacy context policy in api remains for Phase 10 |

| Exit criterion | Completion |
|----------------|------------|
| Context contracts consumed by agent adapter and workspace watch are available and tested. | Completed local |
| Frame model and storage live under `src/context/frame`; no top-level `src/frame`. | Completed local |
| Context generation and queue policy are no longer owned by CLI or mixed legacy paths. | Completed local |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Provides context facade queue and generation contracts required by agent and workspace cutovers. | Completed local |

#### Phase 6 — Implementation notes

Context domain under `src/context` with `mod.rs`, `facade.rs`, `types.rs`. Frame model and storage under `src/context/frame/`; queue under `src/context/queue/`. Generation plan and executor under `src/context/generation/` with `plan.rs` and `executor.rs`; type renamed from GenerationOrchestrator to GenerationExecutor; QueueSubmitter trait and FrameGenerationQueue impl live in context. CLI uses `crate::context::generation` and `crate::context::queue` only. Top-level `src/generation` and `src/frame` removed. Query has `context/query/service.rs` and `context/query/view_policy.rs`; composition and head_queries not yet extracted. Mutation extraction pending.

---

### Phase 7 — Provider and agent command workflows plus adapter cutover

| Field | Value |
|-------|--------|
| Goal | Complete provider and agent command service ownership and move adapter boundary to domain contracts. |
| Dependencies | Phase 2, Phase 3, Phase 4, Phase 6 |
| Docs | provider/provider_migration_plan.md, agent/agent_migration_plan.md |
| Completion | Planned |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Finalize provider command workflow ownership and diagnostics ownership in provider application services. | Planned |
| 2 | Finalize agent command workflow ownership in agent application services. | Planned |
| 3 | Move adapter contract and implementation to agent ports and adapters using context facade contracts. | Planned |
| 4 | Ensure config loads and validation in both domains flow through composition facade and domain contracts only. | Planned |
| 5 | Remove legacy adapter and command orchestration ownership from tooling paths in the same phase window. | Planned |

| Exit criterion | Completion |
|----------------|------------|
| Provider and agent command routes are ready for CLI wave cutover with one service call per variant. | Planned |
| Adapter paths use explicit context contracts with no cross domain internal access. | Planned |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Satisfies provider and agent service readiness gates required by CLI route wave sequencing. | Planned |

---

### Phase 8 — Workspace lifecycle status and watch ownership

| Field | Value |
|-------|--------|
| Goal | Move workspace lifecycle status and watch runtime to workspace domain and complete cross domain hook integration. |
| Dependencies | Phase 4, Phase 5, Phase 6, Phase 7 |
| Docs | workspace/workspace_migration_guide.md |
| Completion | Planned |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Add workspace domain root facade and shared types. | Planned |
| 2 | Extract lifecycle service for validate delete restore compact list deleted and ignore flows. | Planned |
| 3 | Extract workspace status service for workspace section assembly. | Planned |
| 4 | Extract watch events runtime and editor bridge ownership into workspace watch modules. | Planned |
| 5 | Route watch queue hooks through context contracts and watch telemetry hooks through telemetry contracts. | Planned |
| 6 | Route status fan in through agent and provider status contracts from domain services. | Planned |
| 7 | Remove legacy workspace watch editor and status ownership from old modules in the same phase window. | Planned |

| Exit criterion | Completion |
|----------------|------------|
| Workspace services satisfy CLI workspace route wave readiness gates. | Planned |
| Watch runtime and status assembly no longer rely on mixed legacy ownership. | Planned |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Satisfies workspace lifecycle status watch and unified status dependencies required by CLI cutover. | Planned |

---

### Phase 9 — CLI route waves and startup execution cutover

| Field | Value |
|-------|--------|
| Goal | Slim CLI to parse route help output and boundary error mapping only. |
| Dependencies | Phase 4, Phase 5, Phase 6, Phase 7, Phase 8 |
| Docs | cli/cli_migration_plan.md |
| Completion | Planned |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Complete CLI foundation modules for parse help route output and presentation ownership. | Planned |
| 2 | Execute route wave one for workspace commands using workspace services from Phase 8. | Planned |
| 3 | Execute route wave two for agent and provider commands using services from Phase 7. | Planned |
| 4 | Execute route wave three for context commands using context facade contracts from Phase 6. | Planned |
| 5 | Execute route wave four for unified status assembly using workspace agent and provider status contracts. | Planned |
| 6 | Execute route wave five for watch and init using workspace watch and config composition contracts. | Planned |
| 7 | Cut over startup and execution policy so CLI uses config composition facade and telemetry services only. | Planned |
| 8 | Remove legacy route tables and orchestration code from old CLI surfaces in the same phase window. | Planned |

| Exit criterion | Completion |
|----------------|------------|
| CLI owns only boundary responsibilities and one route table. | Planned |
| No domain orchestration policy remains in CLI modules. | Planned |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Completes shared shell cutover gates across provider agent config context workspace and telemetry plans. | Planned |

---

### Phase 10 — Legacy removal and boundary seal

| Field | Value |
|-------|--------|
| Goal | Remove temporary migration surfaces and enforce final domain boundaries. |
| Dependencies | Phase 1 to Phase 9 |
| Docs | god_module_detangling_spec.md, src_module_structure_map.md |
| Completion | Planned |

| Order | Task | Completion |
|-------|------|------------|
| 1 | Remove legacy `src/tooling` ownership paths after all route waves are complete. | Planned |
| 2 | Remove legacy context policy ownership from `src/api.rs` and related old helper surfaces. | Planned |
| 3 | Remove legacy `src/progress` ownership once telemetry ownership is complete. | Completed local |
| 4 | Remove stale exports and stale helper code paths that bypass domain contracts. | Planned |
| 5 | Enforce boundary guard tests for no cross domain internal reach through. | Planned |

| Exit criterion | Completion |
|----------------|------------|
| Final module structure matches domain first ownership targets. | Planned |
| No old mixed ownership surfaces remain active. | Planned |

| Dependency closure solved | Completion |
|---------------------------|------------|
| Delivers final durable architecture and prevents dependency regressions. | Planned |

---

## Implementation order summary

| Order | Phase | Summary |
|-------|-------|---------|
| 1 | Phase 1 | Freeze behavior contracts and establish shared gates. |
| 2 | Phase 2 | Establish provider contracts and repository ownership. |
| 3 | Phase 3 | Establish agent contracts and repository ownership. |
| 4 | Phase 4 | Establish config composition facade and path contracts. |
| 5 | Phase 5 | Establish telemetry contracts and policy services. |
| 6 | Phase 6 | Establish context query mutation generation and queue contracts. |
| 7 | Phase 7 | Complete provider and agent workflow ownership and adapter cutover. |
| 8 | Phase 8 | Complete workspace lifecycle status and watch ownership. |
| 9 | Phase 9 | Execute CLI route waves and startup execution cutover. |
| 10 | Phase 10 | Remove legacy surfaces and seal boundaries. |

---

## Dependency resolution map

| Dependency need | Solved in phase |
|-----------------|-----------------|
| Config needs provider and agent contract readiness | Phase 2 and Phase 3 |
| Context generation needs provider services | Phase 2 then consumed in Phase 6 |
| Context and workspace telemetry hooks need telemetry contracts | Phase 5 then consumed in Phase 6 and Phase 8 |
| Agent adapter needs context contracts | Phase 6 then consumed in Phase 7 |
| Workspace watch needs config path context queue and telemetry contracts | Phase 4 and Phase 5 and Phase 6 then consumed in Phase 8 |
| Unified status needs workspace agent and provider status contracts | Phase 7 and Phase 8 then consumed in Phase 9 |
| CLI thin shell requires all domain service contracts | Phase 4 to Phase 8 then executed in Phase 9 |
| Final boundary seal requires all migrations complete | Phase 10 |
