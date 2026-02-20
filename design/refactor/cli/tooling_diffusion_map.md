# Tooling Diffusion Map

Date: 2026-02-17

## Objective

Capture the staged migration from broad `tooling` ownership into sharp domain homes.

Related specs:
- [CLI Migration Plan](cli_migration_plan.md)
- [CLI Shell Parse Route Help Spec](cli_shell_parse_route_help_spec.md)
- [CLI Presentation Formatting Spec](cli_presentation_formatting.md)
- [Context Generation Orchestration Spec](../context/context_generation_orchestration.md)
- [Agent Provider Config Management Commands Spec](../agent/agent_provider_config_management_commands.md)
- [Workspace Lifecycle Services Spec](../workspace/workspace_lifecycle_services.md)
- [Workspace Watch Runtime Spec](../workspace/workspace_watch_runtime_spec.md)
- [Agent Context Adapter Boundary Spec](../agent/agent_context_adapter_boundary_spec.md)
- [Telemetry Event Engine Spec](../telemetry/telemetry_event_engine_spec.md)

## Rule

`tooling` is not a domain.

- keep only boundary mechanics in `cli`
- move use case policy into domain modules
- keep adapters close to the domain that owns the contract

## Diffusion Table

| Current file | Concern | Target domain | Target modules | Action |
| --- | --- | --- | --- | --- |
| `src/tooling/cli.rs` | Parse route help and output mode | Cli | `src/cli/mod.rs`, `src/cli/parse.rs`, `src/cli/route.rs`, `src/cli/help.rs`, `src/cli/output.rs` | Keep only boundary mechanics |
| `src/tooling/cli.rs` | Command output formatting | Cli | `src/cli/presentation/mod.rs`, `src/cli/presentation/agent.rs`, `src/cli/presentation/provider.rs`, `src/cli/presentation/context.rs`, `src/cli/presentation/init.rs`, `src/cli/presentation/shared.rs` | Move formatter helpers |
| `src/tooling/cli.rs` | Workspace lifecycle orchestration | Workspace | `src/workspace/lifecycle_service.rs` | Move |
| `src/tooling/cli.rs` | Context generation | Context | `src/context/generation/plan.rs`, `src/context/generation/executor.rs`, `src/context/queue/runtime.rs` | Move |
| `src/tooling/cli.rs` | Agent and provider command workflows | Agent and Provider | `src/agent/application/command_service.rs`, `src/provider/application/command_service.rs` | Move |
| `src/tooling/cli.rs` | Session lifecycle and summary emission policy | Telemetry | `src/telemetry/sessions/service.rs`, `src/telemetry/emission/summary_mapper.rs` | Move |
| `src/tooling/watch.rs` | Workspace watch runtime and event batching | Workspace | `src/workspace/watch/mod.rs`, `src/workspace/watch/runtime.rs`, `src/workspace/watch/events.rs` | Move |
| `src/tooling/editor.rs` | Editor watch bridge | Workspace | `src/workspace/watch/editor_bridge.rs` | Move |
| `src/workspace_status.rs` | Workspace status assembly | Workspace | `src/workspace/status_service.rs` | Move |
| `src/tooling/adapter.rs` | Agent to context adapter contract | Agent and Context | `src/agent/ports/context_adapter.rs`, `src/agent/adapters/context_api.rs` | Move |
| `src/tooling/ci.rs` | Sparse CI helpers with no runtime callers | None | None | Delete or rehome later if real CI surface appears |

## Module Definition Snapshot

### Cli

- `src/cli/mod.rs`
- `src/cli/parse.rs`
- `src/cli/route.rs`
- `src/cli/help.rs`
- `src/cli/output.rs`
- `src/cli/presentation/mod.rs`
- `src/cli/presentation/agent.rs`
- `src/cli/presentation/provider.rs`
- `src/cli/presentation/context.rs`
- `src/cli/presentation/init.rs`
- `src/cli/presentation/shared.rs`

### Workspace watch

- `src/workspace/watch/mod.rs`
- `src/workspace/watch/runtime.rs`
- `src/workspace/watch/events.rs`
- `src/workspace/watch/editor_bridge.rs`

### Agent adapter edge

- `src/agent/ports/context_adapter.rs`
- `src/agent/adapters/context_api.rs`

## Sequencing

1. Move CLI shell and presentation specs and paths to `cli`.
2. Extract command workflows from CLI into context agent provider workspace and telemetry services.
3. Move watch and editor runtime to workspace watch modules.
4. Move adapter contract into agent and context owned modules.
5. Delete `src/tooling/ci.rs` after tests and exports are updated.
6. Remove `src/tooling` module root after all callers move.

## Acceptance Criteria

- No refactor specs remain under `design/refactor/tooling`
- CLI owns only parse route help output and formatter dispatch
- Use case policies live in domain modules
- Watch and editor concerns live under workspace
- Adapter contract lives under agent and context ownership
- CI helper surface is removed or moved behind a real domain owner
