# Src Module Structure Map

Date: 2026-02-17

## Purpose

Map the current `src` layout to domain fit and target landing paths.

## Fit Legend

- Good: cohesive domain or stable primitive
- Mixed: valid concern with boundary leakage
- Bad: monolithic ownership or wrong boundary for domain first design

## Module Map

| Module | Size lines | Current role | Fit | Recommended landing | Dedicated spec |
| --- | ---: | --- | --- | --- | --- |
| `src/lib.rs` | 25 | Crate module wiring | Mixed | Keep as thin export root only | None |
| `src/bin/merkle.rs` | 94 | CLI binary bootstrap and logging setup | Mixed | Keep as binary entry only and delegate to cli shell | None |
| `src/tooling.rs` | 17 | Legacy tooling module exports | Mixed | Keep only as migration wrapper until `src/cli` and domain exports replace it | [Tooling Diffusion Map](cli/tooling_diffusion_map.md) |
| `src/tooling` | 6134 | Legacy container with cli watch editor adapter and ci concerns | Bad | Diffuse concerns into cli workspace agent context and telemetry. Remove `src/tooling` after migration | [Tooling Diffusion Map](cli/tooling_diffusion_map.md), [God Module Detangling Spec](god_module_detangling_spec.md), [CLI Shell Parse Route Help Spec](cli/cli_shell_parse_route_help_spec.md), [Context Generation Orchestration Spec](context/context_generation_orchestration.md), [Workspace Lifecycle Services Spec](workspace/workspace_lifecycle_services.md), [Workspace Watch Runtime Spec](workspace/workspace_watch_runtime_spec.md), [Agent Provider Config Management Commands Spec](agent/agent_provider_config_management_commands.md), [Agent Context Adapter Boundary Spec](agent/agent_context_adapter_boundary_spec.md), [CLI Presentation Formatting Spec](cli/cli_presentation_formatting.md) |
| `src/api.rs` | 1841 | Context query mutation and lifecycle mixed in one facade | Bad | Keep as compatibility facade. Move core logic to `src/context` subdomains | [Context Query API Spec](api/context_query_api.md), [Context Domain Structure Spec](context/context_domain_structure.md), [God Module Detangling Spec](god_module_detangling_spec.md) |
| `src/provider.rs` | 1721 | Provider entities clients registry and persistence in one file | Bad | Split to `src/provider/domain`, `src/provider/ports`, `src/provider/application`, `src/provider/infra` | [Provider Diagnostics Connectivity Spec](provider/provider_diagnostics_connectivity.md), [God Module Detangling Spec](god_module_detangling_spec.md) |
| `src/agent.rs` | 686 | Agent identity registry and config persistence mixed | Mixed | Split to `src/agent/domain`, `src/agent/ports`, `src/agent/application`, `src/agent/infra` | [God Module Detangling Spec](god_module_detangling_spec.md), [Agent Provider Config Management Commands Spec](agent/agent_provider_config_management_commands.md) |
| `src/progress` | 862 | Event contracts bus ingestor session store | Mixed | Rename and migrate to `src/telemetry` with behavior named subdomains | [Telemetry Event Engine Spec](telemetry/telemetry_event_engine_spec.md) |
| `src/generation` | 523 | Generation plan and executor | Mixed | Move to `src/context/generation` | [Context Domain Structure Spec](context/context_domain_structure.md), [Context Generation Orchestration Spec](context/context_generation_orchestration.md) |
| `src/frame` | 2301 | Frame model storage set and queue runtime | Mixed | Move entire module to `src/context/frame` as submodule of context. Queue runtime to `src/context/queue`. Remove top-level `src/frame`. | [Context Domain Structure Spec](context/context_domain_structure.md) |
| `src/views.rs` | 337 | Context view policy and frame selection | Mixed | Move to `src/context/query/view_policy.rs` | [Context Query API Spec](api/context_query_api.md), [Context Domain Structure Spec](context/context_domain_structure.md) |
| `src/composition.rs` | 675 | Multi frame context composition | Mixed | Move to `src/context/query/composition.rs` | [Context Query API Spec](api/context_query_api.md), [Context Domain Structure Spec](context/context_domain_structure.md) |
| `src/workspace_status.rs` | 455 | Workspace status plus agent and provider status shaping | Mixed | Move workspace section assembly to `src/workspace/status_service.rs` and delegate agent provider status to their domains | [Workspace Lifecycle Services Spec](workspace/workspace_lifecycle_services.md), [God Module Detangling Spec](god_module_detangling_spec.md) |
| `src/ignore.rs` | 397 | Workspace ignore list policy and sync | Good | Keep under workspace domain as primitive or move to `src/workspace/ignore` when workspace package exists | [Workspace Lifecycle Services Spec](workspace/workspace_lifecycle_services.md) |
| `src/tree` | 1026 | Tree walk hash build primitives | Good | Keep as workspace tree primitive | None |
| `src/store` | 909 | Node record storage and persistence | Good | Keep as workspace storage primitive | None |
| `src/heads.rs` | 438 | Head index read write and tombstone helpers | Mixed | Keep as shared primitive now. Later place under context mutation and query contracts | [God Module Detangling Spec](god_module_detangling_spec.md), [Context Query API Spec](api/context_query_api.md) |
| `src/config.rs` | 1310 | Global config types load merge and XDG path logic | Mixed | Promote to `src/config` composition root. Delegate provider and agent config policy to their domains | [Config Composition Root Spec](config/config_composition_root_spec.md), [Provider Diagnostics Connectivity Spec](provider/provider_diagnostics_connectivity.md), [God Module Detangling Spec](god_module_detangling_spec.md), [Agent Provider Config Management Commands Spec](agent/agent_provider_config_management_commands.md) |
| `src/init.rs` | 319 | Default prompt and agent bootstrapping | Mixed | Keep as bootstrap flow in cli boundary or create `src/bootstrap` package | None |
| `src/logging.rs` | 345 | Logging config and tracing setup | Good | Keep as infra logging primitive. Telemetry domain consumes it as needed | None |
| `src/concurrency.rs` | 162 | Node lock manager for mutation safety | Good | Keep as shared primitive or move to context mutation internals later | None |
| `src/error.rs` | 121 | Shared error contracts | Good | Keep as cross domain contract module | None |
| `src/types.rs` | 10 | Core id aliases | Good | Keep as cross domain primitives | None |

## Proposed Good Targets

| Proposed area | Target path | Ported from current good item | Why this is good | Source spec |
| --- | --- | --- | --- | --- |
| Context domain | `src/context` | New domain target | One owner for query mutation generation queue and frame model and storage. Frame at `src/context/frame`; generation at `src/context/generation`. | [Context Domain Structure Spec](context/context_domain_structure.md), [Context Query API Spec](api/context_query_api.md), [Context Generation Orchestration Spec](context/context_generation_orchestration.md) |
| Provider domain | `src/provider` | New domain target | Domain meaning contracts and implementations live together behind explicit boundaries | [Provider Diagnostics Connectivity Spec](provider/provider_diagnostics_connectivity.md) |
| Agent domain | `src/agent` | New domain target | Agent identity authorization config policy and command workflows are owned by one domain | [God Module Detangling Spec](god_module_detangling_spec.md), [Agent Provider Config Management Commands Spec](agent/agent_provider_config_management_commands.md) |
| Config domain | `src/config` | New domain target | Composition root for source loading precedence merge and domain config delegation | [Config Composition Root Spec](config/config_composition_root_spec.md) |
| Telemetry domain | `src/telemetry` | New domain target | Event emission has clear subdomains for contracts sessions emission routing and sinks | [Telemetry Event Engine Spec](telemetry/telemetry_event_engine_spec.md) |
| Workspace domain | `src/workspace` | New domain target | Lifecycle orchestration has a single owner and CLI remains a thin route adapter | [Workspace Lifecycle Services Spec](workspace/workspace_lifecycle_services.md) |
| Workspace status service | `src/workspace/status_service.rs` | `src/workspace_status.rs` | Workspace status assembly has one domain owner under workspace | [Workspace Lifecycle Services Spec](workspace/workspace_lifecycle_services.md) |
| Workspace watch runtime | `src/workspace/watch` | `src/tooling/watch.rs` and `src/tooling/editor.rs` | Watch runtime and editor bridge stay with workspace behavior ownership | [Workspace Lifecycle Services Spec](workspace/workspace_lifecycle_services.md), [Workspace Watch Runtime Spec](workspace/workspace_watch_runtime_spec.md), [Tooling Diffusion Map](cli/tooling_diffusion_map.md) |
| Agent context adapter edge | `src/agent/ports/context_adapter.rs` and `src/agent/adapters/context_api.rs` | `src/tooling/adapter.rs` | Adapter contract and implementation are owned by agent domain boundaries | [Agent Context Adapter Boundary Spec](agent/agent_context_adapter_boundary_spec.md), [Tooling Diffusion Map](cli/tooling_diffusion_map.md) |
| Cli boundary root | `src/cli` | New domain target | Sharp boundary for parse route help output and formatting dispatch | [CLI Shell Parse Route Help Spec](cli/cli_shell_parse_route_help_spec.md), [CLI Presentation Formatting Spec](cli/cli_presentation_formatting.md), [Tooling Diffusion Map](cli/tooling_diffusion_map.md) |
| Workspace ignore primitive | `src/workspace/ignore` | `src/ignore.rs` | Ignore policy stays cohesive within workspace behavior | [Workspace Lifecycle Services Spec](workspace/workspace_lifecycle_services.md) |
| Workspace tree primitive | `src/tree` | `src/tree` | Tree build and hash behavior is cohesive and reusable | None |
| Workspace store primitive | `src/store` | `src/store` | Node record persistence stays cohesive and reusable | None |
| Shared logging primitive | `src/logging.rs` | `src/logging.rs` | Shared infra concern with clear ownership | None |
| Shared concurrency primitive | `src/concurrency.rs` | `src/concurrency.rs` | Locking primitive is focused and reusable | None |
| Shared error contracts | `src/error.rs` | `src/error.rs` | One contract surface for cross domain error translation | None |
| Shared core ids | `src/types.rs` | `src/types.rs` | Shared identifiers keep cross domain contracts stable | None |
