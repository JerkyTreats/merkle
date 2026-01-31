# Context & Agent Management Refactor Implementation Plan

## Overview

This document outlines the phased implementation plan for refactoring the context and agent management system. The refactor introduces provider-agent separation, XDG-based configuration storage, and new CLI commands for managing agents, providers, and context operations.

The implementation follows a logical progression: first decoupling providers from agents, then moving to XDG-based storage, and finally implementing the new CLI commands that build on these foundations.

---

## Development Phases

### Phase 1 — Provider-Agent Separation

**Goal**: Decouple provider configuration from agent configuration, enabling runtime provider selection.

**Status**: ✅ **COMPLETED**

| Task | Status |
|------|--------|
| Design ProviderRegistry structure | ✅ Completed |
| Implement ProviderConfig type | ✅ Completed |
| Remove provider_name from AgentIdentity | ✅ Completed |
| Remove completion_options from agent config | ✅ Completed |
| Update AgentIdentity to be provider-agnostic | ✅ Completed |
| Implement ProviderRegistry::load_from_config() | ✅ Completed (XDG loading deferred to Phase 2) |
| Implement ProviderRegistry::create_client() | ✅ Completed |
| Update FrameGenerationQueue to accept provider at runtime | ✅ Completed |
| Update ContextApiAdapter to pass provider to queue | ✅ Completed |
| Add provider metadata to frame generation | ✅ Completed |
| Update completion options resolution (provider defaults + agent preferences) | ✅ Completed |
| Provider-agent separation tests | ✅ Completed (246 tests passing) |

**Exit Criteria:**
- ✅ ProviderRegistry implemented and independent from AgentRegistry
- ✅ AgentIdentity no longer contains provider references
- ✅ Frame generation accepts provider_name as runtime parameter
- ✅ Completion options resolved from provider defaults (not agent config)
- ✅ Frame metadata includes provider information for attribution
- ✅ All tests pass with new architecture

**Key Changes:**
- `AgentIdentity` struct: Remove `provider` field
- `ProviderRegistry`: New registry for provider configurations
- `FrameGenerationQueue`: Accept `provider_name` parameter in `enqueue()` and `enqueue_and_wait()`
- `ContextApiAdapter`: Pass provider_name when generating frames
- Frame metadata: Include `provider`, `model`, `provider_type` fields

**Dependencies:**
- None (foundational change)

**Documentation:**
- [Provider-Agent Separation](provider/provider_agent_separation.md) - Design specification for decoupling providers from agents

**Phase 1 Completion Summary:**
- ✅ ProviderRegistry implemented with `load_from_config()` (config.toml loading; XDG loading deferred to Phase 2)
- ✅ ProviderConfig enhanced with `provider_name` field
- ✅ AgentIdentity made provider-agnostic (removed `provider` field)
- ✅ AgentConfig cleaned (removed `provider_name` and `completion_options`)
- ✅ FrameGenerationQueue updated to accept `provider_name` parameter
- ✅ ContextApiAdapter updated to pass `provider_name` to queue
- ✅ Completion options resolved from provider defaults
- ✅ Frame metadata includes provider, model, and provider_type
- ✅ ContextApi includes ProviderRegistry
- ✅ All initialization code updated
- ✅ All 246 tests passing (137 unit, 104 integration, 4 property, 1 doc)

**Note**: `ProviderRegistry::load_from_xdg()` is deferred to Phase 2. Phase 1 uses `load_from_config()` to load from existing config.toml structure, maintaining backward compatibility while preparing the architecture for XDG-based storage.

---

### Phase 2 — XDG Configuration System

**Goal**: Move agent and provider configurations to XDG directories, supporting markdown-based prompts.

**Status**: ✅ **COMPLETED**

| Task | Status |
|------|--------|
| Implement XDG directory resolution utilities | ✅ Completed |
| Create ProviderRegistry::load_from_xdg() implementation | ✅ Completed |
| Create AgentRegistry::load_from_xdg() implementation | ✅ Completed |
| Implement prompt file path resolution (absolute, tilde, relative) | ✅ Completed |
| Implement markdown prompt file loading | ✅ Completed |
| Update agent config schema (system_prompt_path instead of system_prompt) | ✅ Completed |
| Implement provider config schema (XDG TOML format) | ✅ Completed |
| Implement prompt file validation (exists, readable, UTF-8) | ✅ Completed |
| Implement prompt content caching with modification time checks | ✅ Completed |
| Add configuration validation for agents and providers | ✅ Completed |
| XDG configuration loading tests | ✅ Completed (20 tests, all passing) |

**Exit Criteria:**
- ✅ Agents load from `$XDG_CONFIG_HOME/merkle/agents/*.toml`
- ✅ Providers load from `$XDG_CONFIG_HOME/merkle/providers/*.toml`
- ✅ Agent configs reference markdown prompt files via `system_prompt_path`
- ✅ Prompt files can be anywhere (absolute, tilde, or relative paths)
- ✅ Prompt files loaded and validated on agent load
- ✅ Clear error messages for missing/invalid configs

**Phase 2 Completion Summary:**
- ✅ XDG config directory utilities implemented (`config_home()`, `agents_dir()`, `providers_dir()`)
- ✅ Prompt file path resolution supporting absolute, tilde, relative, and base-relative paths
- ✅ Prompt file loading and caching with modification time tracking
- ✅ AgentConfig schema updated with `system_prompt_path` field (backward compatible with `system_prompt`)
- ✅ `ProviderRegistry::load_from_xdg()` implemented with error handling
- ✅ `AgentRegistry::load_from_xdg()` implemented with prompt file resolution and loading
- ✅ CLI initialization updated to load from both config.toml and XDG (XDG overrides)
- ✅ Comprehensive validation for XDG-loaded configs and prompt files
- ✅ 20 integration tests covering all functionality (all passing, non-flaky)
- ✅ All 124 integration tests passing

**Key Changes:**
- New directory structure: `$XDG_CONFIG_HOME/merkle/agents/` and `$XDG_CONFIG_HOME/merkle/providers/`
- Agent config format: `system_prompt_path` field instead of inline `system_prompt`
- Provider config format: Separate TOML files per provider
- Path resolution: Support absolute, tilde (`~/`), and relative paths

**Dependencies:**
- Phase 1 (Provider-Agent Separation) - Registry structures must support XDG loading

**Documentation:**
- [Agent Management Requirements](agents/agent_management_requirements.md) - Agent configuration and XDG storage requirements
- [Provider Management Requirements](provider/provider_management_requirements.md) - Provider configuration and XDG storage requirements

---

### Phase 3 — Agent Management CLI Commands

**Goal**: Implement CLI commands for managing agents stored in XDG directories.

**Status**: ✅ **COMPLETED**

| Task | Status |
|------|--------|
| Implement `merkle agent list` command | ✅ Completed |
| Implement `merkle agent show <agent_id>` command | ✅ Completed |
| Implement `merkle agent validate <agent_id>` command | ✅ Completed |
| Implement `merkle agent create <agent_id>` command (interactive) | ✅ Completed |
| Implement `merkle agent edit <agent_id>` command | ✅ Completed |
| Implement `merkle agent remove <agent_id>` command | ✅ Completed |
| Add agent filtering (by role, by source) | ✅ Completed |
| Add output formatting (text, JSON) | ✅ Completed |
| Implement prompt file content display (--include-prompt) | ✅ Completed |
| Implement agent validation logic (config + prompt file checks) | ✅ Completed |
| Add editor integration for `agent edit` | ✅ Completed |
| Agent CLI tests | ✅ Completed (16 integration tests, 3 unit tests, all passing) |

**Exit Criteria:**
- ✅ `merkle agent list` shows all agents from XDG directory
- ✅ `merkle agent show` displays agent details with optional prompt content
- ✅ `merkle agent validate` checks config and prompt file validity
- ✅ `merkle agent create` creates new agent configs interactively
- ✅ `merkle agent edit` allows editing agent configs
- ✅ `merkle agent remove` removes XDG agents (with confirmation)
- ✅ All commands support text and JSON output formats
- ✅ Clear error messages for missing/invalid agents

**Key Commands:**
- `merkle agent list [--format text|json] [--role Reader|Writer|Synthesis]`
- `merkle agent show <agent_id> [--format text|json] [--include-prompt]`
- `merkle agent validate <agent_id> [--verbose]`
- `merkle agent create <agent_id> [--role <role>] [--prompt-path <path>] [--interactive|--non-interactive]`
- `merkle agent edit <agent_id> [--prompt-path <path>] [--role <role>] [--editor <editor>]`
- `merkle agent remove <agent_id> [--force]`

**Phase 3 Completion Summary:**
- ✅ CLI command structure implemented with `Agent` subcommand and 6 subcommands
- ✅ `AgentRegistry` extended with management methods: `list_by_role()`, `get_agent_config_path()`, `save_agent_config()`, `delete_agent_config()`, `validate_agent()`
- ✅ `ValidationResult` type implemented with comprehensive validation checks
- ✅ Text and JSON output formatters for all commands
- ✅ `merkle agent list` command with role filtering and format options
- ✅ `merkle agent show` command with optional prompt content display
- ✅ `merkle agent validate` command with verbose output option
- ✅ `merkle agent create` command with interactive and non-interactive modes
- ✅ `merkle agent edit` command with flag-based and editor-based editing
- ✅ `merkle agent remove` command with confirmation prompt
- ✅ Helpful error messages with suggestions for common issues
- ✅ 16 integration tests covering all commands and scenarios (all passing)
- ✅ 3 unit tests for filtering, validation, and config path resolution (all passing)
- ✅ `dialoguer` dependency added for interactive prompts

**Key Changes:**
- New CLI subcommand: `merkle agent` with 6 subcommands (list, show, validate, create, edit, remove)
- `AgentRegistry` extended with management operations
- Validation system with structured `ValidationResult` type
- Interactive agent creation using `dialoguer` crate
- Editor integration for config editing
- Comprehensive error handling with actionable suggestions

**Dependencies:**
- Phase 2 (XDG Configuration System) - Agents must load from XDG directories

**Documentation:**
- [Agent CLI Specification](agents/agent_cli_spec.md) - Complete CLI command specification
- [Agent Management Requirements](agents/agent_management_requirements.md) - Agent configuration requirements

---

### Phase 3.5 — Initialization Command and Default Agents

**Goal**: Implement `merkle init` command to initialize default required agents with their prompts.

**Status**: ⏳ **PENDING**

| Task | Status |
|------|--------|
| Design default agents requirements and specifications | Pending |
| Determine prompt storage mechanism (binary embedding vs. external files) | Pending |
| Implement `merkle init` command structure | Pending |
| Create default agent configurations | Pending |
| Implement prompt file creation/copying logic | Pending |
| Add initialization validation and idempotency checks | Pending |
| Create initialization tests | Pending |

**Exit Criteria:**
- ✅ `merkle init` command creates default required agents
- ✅ Default agent prompts are stored and accessible (binary or external)
- ✅ Agents are initialized to correct XDG location
- ✅ Command is idempotent (safe to run multiple times)
- ✅ Clear feedback on what was created/updated
- ✅ Validation that agents were created successfully

**Key Commands:**
- `merkle init` - Initialize default agents and prompts
- `merkle init --force` - Force re-initialization (overwrite existing)
- `merkle init --list` - List what would be initialized without creating

**Key Considerations:**
- **Prompt Storage**: Decide between embedding prompts in binary vs. storing as external files
- **Default Agents**: Define required default agents and their roles (Reader, Writer, Synthesis)
- **Idempotency**: Ensure safe re-running without duplicating agents
- **User Customization**: Allow users to customize default agents after initialization
- **Location**: Initialize to `$XDG_CONFIG_HOME/merkle/agents/` directory

**Dependencies:**
- Phase 3 (Agent Management CLI) - Agent creation and management infrastructure must exist

**Documentation:**
- [Default Agents Requirements](agents/default_agents_requirements.md) - Specification for default required agents and their prompts
- [Initialization Command Specification](init_command_spec.md) - Complete `merkle init` command specification

---

### Phase 4 — Provider Management CLI Commands

**Goal**: Implement CLI commands for managing providers stored in XDG directories.

| Task | Status |
|------|--------|
| Implement `merkle provider list` command | Pending |
| Implement `merkle provider show <provider_name>` command | Pending |
| Implement `merkle provider validate <provider_name>` command | Pending |
| Implement `merkle provider test <provider_name>` command | Pending |
| Implement `merkle provider create <provider_name>` command (interactive) | Pending |
| Implement `merkle provider edit <provider_name>` command | Pending |
| Implement `merkle provider remove <provider_name>` command | Pending |
| Add provider filtering (by type, by source) | Pending |
| Add output formatting (text, JSON) | Pending |
| Implement API key status display (without exposing keys) | Pending |
| Implement provider validation logic (config + connectivity checks) | Pending |
| Implement provider connectivity testing | Pending |
| Add editor integration for `provider edit` | Pending |
| Provider CLI tests | Pending |

**Exit Criteria:**
- ✅ `merkle provider list` shows all providers from XDG directory
- ✅ `merkle provider show` displays provider details with API key status
- ✅ `merkle provider validate` checks config validity and optionally tests connectivity
- ✅ `merkle provider test` tests provider connectivity and model availability
- ✅ `merkle provider create` creates new provider configs interactively
- ✅ `merkle provider edit` allows editing provider configs
- ✅ `merkle provider remove` removes XDG providers (with confirmation)
- ✅ All commands support text and JSON output formats
- ✅ Clear error messages for missing/invalid providers

**Key Commands:**
- `merkle provider list [--format text|json] [--type openai|anthropic|ollama|local]`
- `merkle provider show <provider_name> [--format text|json] [--include-credentials]`
- `merkle provider validate <provider_name> [--test-connectivity] [--check-model] [--verbose]`
- `merkle provider test <provider_name> [--model <model>] [--timeout <seconds>]`
- `merkle provider create <provider_name> [--type <type>] [--model <model>] [--endpoint <url>] [--api-key <key>] [--interactive|--non-interactive]`
- `merkle provider edit <provider_name> [--model <model>] [--endpoint <url>] [--api-key <key>] [--editor <editor>]`
- `merkle provider remove <provider_name> [--force]`

**Dependencies:**
- Phase 2 (XDG Configuration System) - Providers must load from XDG directories
- Phase 1 (Provider-Agent Separation) - ProviderRegistry must be implemented

**Documentation:**
- [Provider CLI Specification](provider/provider_cli_spec.md) - Complete CLI command specification
- [Provider Management Requirements](provider/provider_management_requirements.md) - Provider configuration requirements

---

### Phase 5 — Context Commands with New Architecture

**Goal**: Implement and update context commands to use the new provider-agent separation and XDG configuration.

| Task | Status |
|------|--------|
| Implement `merkle context generate` command | Pending |
| Implement `merkle context get` command | Pending |
| Add `--provider` flag to context generate | Pending |
| Update path resolution (canonicalize, lookup NodeID) | Pending |
| Update agent resolution (default to single Writer agent or require --agent) | Pending |
| Update frame type resolution (default to context-<agent_id>) | Pending |
| Implement head frame existence check (--force flag) | Pending |
| Implement sync/async mode (--sync, --async flags) | Pending |
| Add frame filtering (--agent, --frame-type) to context get | Pending |
| Add output formatting (--format text|json, --combine, --separator) | Pending |
| Add metadata display (--include-metadata) | Pending |
| Add deleted frame handling (--include-deleted) | Pending |
| Update error messages with helpful suggestions | Pending |
| Context CLI tests | Pending |

**Exit Criteria:**
- ✅ `merkle context generate` creates frames using agent + provider (runtime binding)
- ✅ `merkle context generate` supports `--provider` flag for runtime provider selection
- ✅ `merkle context get` retrieves and displays frames with filtering and formatting
- ✅ Path resolution works correctly (canonicalize, NodeID lookup)
- ✅ Agent resolution works (default or explicit via --agent)
- ✅ Frame type defaults to `context-<agent_id>` when not specified
- ✅ Head frame checks prevent duplicate generation (unless --force)
- ✅ Sync and async modes work correctly
- ✅ All filtering, formatting, and output options work
- ✅ Clear error messages with remediation suggestions

**Key Commands:**
- `merkle context generate --path <path>|--node <node_id> [--agent <agent_id>] [--provider <provider_name>] [--frame-type <type>] [--force] [--sync|--async]`
- `merkle context get --path <path>|--node <node_id> [--agent <agent_id>] [--frame-type <type>] [--max-frames <n>] [--ordering recency|deterministic] [--combine] [--separator <text>] [--format text|json] [--include-metadata] [--include-deleted]`

**Key Changes:**
- Context generate: Requires `--provider` flag (or uses default from config)
- Context generate: Agent and provider bound at runtime, not configuration time
- Context get: Rich filtering and formatting options
- Error messages: Include suggestions (e.g., "Run `merkle scan` to update tree")

**Dependencies:**
- Phase 1 (Provider-Agent Separation) - Runtime provider selection required
- Phase 2 (XDG Configuration System) - Agents and providers loaded from XDG
- Phase 3 (Agent Management CLI) - Agent discovery and validation
- Phase 4 (Provider Management CLI) - Provider discovery and validation

**Documentation:**
- [Context Generate Command](context_generate_command.md) - Context generation command specification
- [Context Get Command](context_get_command.md) - Context retrieval command specification

---

## Implementation Order Summary

1. **Phase 1: Provider-Agent Separation** (Foundation) ✅ **COMPLETED**
   - Decouples providers from agents
   - Enables runtime provider selection
   - No external dependencies
   - **Status**: All tasks completed, 246 tests passing

2. **Phase 2: XDG Configuration System** (Storage) ✅ **COMPLETED**
   - Moves configs to XDG directories
   - Enables markdown prompts
   - Depends on Phase 1 (registry structures)
   - **Status**: All tasks completed, 20 XDG config tests passing, all 124 integration tests passing

3. **Phase 3: Agent Management CLI** (Agent Tooling) ✅ **COMPLETED**
   - CLI for managing agents
   - Depends on Phase 2 (XDG loading)
   - **Status**: All tasks completed, 16 integration tests and 3 unit tests passing

4. **Phase 4: Provider Management CLI** (Provider Tooling)
   - CLI for managing providers
   - Depends on Phase 2 (XDG loading) and Phase 1 (ProviderRegistry)

5. **Phase 5: Context Commands** (User-Facing Commands)
   - Main user-facing commands
   - Depends on all previous phases

---

## Testing Strategy

### Unit Tests
- Registry loading and validation
- Path resolution (absolute, tilde, relative)
- Prompt file loading and caching
- Configuration validation

### Integration Tests
- End-to-end CLI command execution
- XDG directory structure creation and loading
- Provider-agent runtime binding
- Frame generation with new architecture

### CLI Tests
- All command variations and flags
- Error handling and error messages
- Output formatting (text and JSON)
- Interactive command flows

---

## Success Criteria

The refactor is complete when:

1. ✅ Providers and agents are completely separated **(Phase 1 - COMPLETED)**
2. ✅ Agents and providers stored in XDG directories **(Phase 2 - COMPLETED)**
3. ✅ Agents use markdown prompt files **(Phase 2 - COMPLETED)**
4. ⏳ All CLI commands implemented and tested (Phases 3-5) - **Phase 3 COMPLETED**
5. ⏳ Clear error messages and user guidance (Phases 3-5) - **Phase 3 COMPLETED**
6. ⏳ Documentation updated (Ongoing)
7. ✅ All existing tests pass **(Phase 1 - COMPLETED: 246 tests passing; Phase 2 - COMPLETED: 124 integration tests passing; Phase 3 - COMPLETED: 16 integration + 3 unit tests passing)**
8. ✅ New tests cover all functionality **(Phase 1 - COMPLETED; Phase 2 - COMPLETED: 20 XDG config tests; Phase 3 - COMPLETED: 16 agent CLI tests)**

---

## Related Documentation

- **[README.md](README.md)** - Context management overview
- **[provider/provider_agent_separation.md](provider/provider_agent_separation.md)** - Separation design
- **[provider/provider_management_requirements.md](provider/provider_management_requirements.md)** - Provider requirements
- **[agents/agent_management_requirements.md](agents/agent_management_requirements.md)** - Agent requirements
- **[context_generate_command.md](context_generate_command.md)** - Context generate spec
- **[context_get_command.md](context_get_command.md)** - Context get spec
- **[agents/agent_cli_spec.md](agents/agent_cli_spec.md)** - Agent CLI spec
- **[provider/provider_cli_spec.md](provider/provider_cli_spec.md)** - Provider CLI spec

---

[← Back to Context Management](README.md)

