# Context Generate Command Specification

## Overview

This document specifies the implementation details for `merkle context generate`, which creates new context frames for nodes using configured LLM providers.

## CLI Functionality

### Command Structure

The command follows the existing CLI pattern in `src/tooling/cli.rs`:

```rust
#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands ...
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },
}

#[derive(Subcommand)]
pub enum ContextCommands {
    /// Generate context frame for a node
    Generate {
        /// Target node by NodeID (hex string)
        #[arg(long, conflicts_with = "path")]
        node: Option<String>,
        
        /// Target node by workspace-relative or absolute path
        #[arg(long, conflicts_with = "node")]
        path: Option<PathBuf>,
        
        /// Agent to use for generation
        #[arg(long)]
        agent: Option<String>,
        
        /// Frame type (defaults to context-<agent_id>)
        #[arg(long)]
        frame_type: Option<String>,
        
        /// Generate even if head frame exists
        #[arg(long)]
        force: bool,
        
        /// Execute immediately (default)
        #[arg(long, conflicts_with = "async")]
        sync: bool,
        
        /// Enqueue generation with Priority::Urgent
        #[arg(long, conflicts_with = "sync")]
        r#async: bool,
    },
}
```

### Command Execution Flow

1. **Path Resolution** (if `--path` provided):
   - Canonicalize path relative to workspace root using `crate::tree::path::canonicalize_path()`
   - Look up NodeID in `NodeRecordStore` via `api.node_store().find_by_path()`
   - Return `PathNotInTree` error if path not found
   - Provide helpful error message suggesting `merkle scan` or `merkle watch`

2. **NodeID Resolution** (if `--node` provided):
   - Parse hex string to `NodeID` using existing `parse_node_id()` helper
   - Verify node exists in store via `api.node_store().get()`
   - Return `NodeNotFound` error if not found

3. **Agent Resolution**:
   - If `--agent` provided, use that agent ID
   - Otherwise, check if exactly one Writer agent exists in config
   - If multiple Writer agents exist, return error requiring `--agent`
   - If no Writer agents exist, return error

4. **Frame Type Resolution**:
   - Use `--frame-type` if provided
   - Otherwise default to `format!("context-{}", agent_id)`

5. **Validation**:
   - Verify agent exists in registry via `api.get_agent()`
   - Verify agent has provider configured (check `agent.provider.is_some()`)
   - Verify agent has required prompts:
     - `system_prompt` in metadata (required)
     - `user_prompt_file` or `user_prompt_directory` based on node type
   - Use `FrameGenerationQueue::validate_agent_prompts()` helper

6. **Head Frame Check** (if `--force` not set):
   - Check if head frame exists via `api.get_head(node_id, frame_type)`
   - If exists, return no-op result: `"Frame already exists: <frame_id>"`

7. **Generation**:
   - **Sync mode** (default or `--sync`):
     - Call `ContextApiAdapter::generate_frame()` directly (await)
     - Return FrameID as hex string
   - **Async mode** (`--async`):
     - Enqueue via `api.frame_queue().enqueue(node_id, agent_id, frame_type, Priority::Urgent)`
     - Return queue request ID (or confirmation message)

## Required Logical Guards

### 1. Mutually Exclusive Options
- `--node` and `--path` must be mutually exclusive (enforced by clap `conflicts_with`)
- Exactly one must be provided (validate in execution logic)

### 2. Agent Validation
- Agent must exist in registry
- Agent must have `AgentRole::Writer` or `AgentRole::Synthesis`
- Agent must have provider configured (unless generating metadata-only frame)
- Agent must have required prompts in metadata

### 3. Node Validation
- Node must exist in store (for both path and node_id resolution)
- Path must be canonicalized and within workspace root

### 4. Frame Existence Check
- If `--force` not set, check for existing head frame
- Skip generation if head exists (idempotent behavior)

### 5. Provider Configuration
- Provider must be valid and accessible
- Provider authentication must succeed (handled by provider client)

### 6. Prompt Validation
- `system_prompt` must exist in agent metadata
- `user_prompt_file` required for file nodes
- `user_prompt_directory` required for directory nodes

## Tests Required

### CLI Tests (`tests/integration/context_cli.rs`)

1. **Path Resolution Tests**:
   - `test_generate_with_valid_path()` - Success case with path
   - `test_generate_with_invalid_path()` - Path not in tree error
   - `test_generate_with_absolute_path()` - Absolute path resolution
   - `test_generate_with_relative_path()` - Relative path resolution
   - `test_generate_path_error_suggests_scan()` - Error message includes scan suggestion

2. **NodeID Resolution Tests**:
   - `test_generate_with_valid_node_id()` - Success case with node ID
   - `test_generate_with_invalid_node_id()` - Invalid hex string error
   - `test_generate_with_nonexistent_node_id()` - NodeNotFound error

3. **Agent Resolution Tests**:
   - `test_generate_with_single_writer_agent()` - Default agent selection
   - `test_generate_with_multiple_agents_requires_flag()` - Error when multiple agents
   - `test_generate_with_no_agents_error()` - Error when no agents configured
   - `test_generate_with_specified_agent()` - Explicit agent selection

4. **Validation Tests**:
   - `test_generate_validates_provider_configured()` - Error if no provider
   - `test_generate_validates_system_prompt()` - Error if missing system_prompt
   - `test_generate_validates_user_prompt_file()` - Error if missing user_prompt_file for files
   - `test_generate_validates_user_prompt_directory()` - Error if missing user_prompt_directory for dirs

5. **Force Flag Tests**:
   - `test_generate_skips_if_head_exists()` - No-op when head exists without --force
   - `test_generate_force_overwrites_head()` - Generates new frame with --force

6. **Sync/Async Mode Tests**:
   - `test_generate_sync_returns_frame_id()` - Sync mode returns FrameID
   - `test_generate_async_returns_queue_id()` - Async mode returns queue confirmation
   - `test_generate_default_is_sync()` - Default behavior is sync

7. **Frame Type Tests**:
   - `test_generate_uses_custom_frame_type()` - Custom frame type used when provided
   - `test_generate_defaults_frame_type()` - Defaults to context-<agent_id>

### Functional Tests (`tests/integration/context_api.rs`)

1. **Generation Logic Tests**:
   - `test_generate_frame_creates_valid_frame()` - Frame structure is correct
   - `test_generate_frame_updates_head_index()` - Head index updated correctly
   - `test_generate_frame_persists_to_storage()` - Frame stored on disk
   - `test_generate_frame_metadata_includes_agent()` - Metadata includes agent_id

2. **Provider Integration Tests**:
   - `test_generate_with_openai_provider()` - OpenAI provider works
   - `test_generate_with_anthropic_provider()` - Anthropic provider works
   - `test_generate_with_ollama_provider()` - Ollama provider works
   - `test_generate_handles_provider_errors()` - Provider errors handled gracefully
   - `test_generate_handles_rate_limiting()` - Rate limiting handled correctly

3. **Prompt Generation Tests**:
   - `test_generate_replaces_path_placeholder()` - {path} placeholder replaced
   - `test_generate_replaces_node_type_placeholder()` - {node_type} placeholder replaced
   - `test_generate_replaces_file_size_placeholder()` - {file_size} replaced for files

4. **Concurrency Tests**:
   - `test_generate_concurrent_requests()` - Multiple concurrent generations work
   - `test_generate_node_lock_prevents_conflicts()` - Node locks prevent conflicts

### Unit Tests (`src/tooling/cli.rs` - module tests)

1. **Helper Function Tests**:
   - `test_parse_node_id_valid()` - Valid hex string parsing
   - `test_parse_node_id_invalid()` - Invalid hex string error
   - `test_resolve_path_to_node_id()` - Path to NodeID resolution
   - `test_resolve_default_agent()` - Default agent selection logic

## Patterns to Follow

### CLI Structure

1. **Command Location**: Add to `src/tooling/cli.rs` in the `Commands` enum
2. **Subcommand Pattern**: Use nested `ContextCommands` enum for `merkle context` subcommands
3. **Option Parsing**: Use clap with `#[arg(long)]` for all options
4. **Mutual Exclusivity**: Use `conflicts_with` for mutually exclusive options
5. **Error Handling**: Return `Result<String, ApiError>` from `CliContext::execute()`

### Logic Location

1. **Path Resolution**: Create helper function `resolve_path_to_node_id()` in `src/tooling/cli.rs`
2. **Agent Resolution**: Create helper function `resolve_agent_id()` in `src/tooling/cli.rs`
3. **Validation**: Reuse `FrameGenerationQueue::validate_agent_prompts()` from `src/frame/queue.rs`
4. **Generation**: Use `ContextApiAdapter::generate_frame()` from `src/tooling/adapter.rs` for sync
5. **Queue Enqueue**: Use `FrameGenerationQueue::enqueue()` from `src/frame/queue.rs` for async

### Components to Use

1. **ContextApi**: Main API for node operations (`src/api.rs`)
   - `get_node()` - Get node context
   - `get_head()` - Check for existing head frame
   - `node_store()` - Access to node record store

2. **ContextApiAdapter**: LLM generation adapter (`src/tooling/adapter.rs`)
   - `generate_frame()` - Generate frame using LLM provider

3. **FrameGenerationQueue**: Async queue for generation (`src/frame/queue.rs`)
   - `enqueue()` - Enqueue generation request
   - `validate_agent_prompts()` - Validate agent prompts

4. **AgentRegistry**: Agent management (`src/agent.rs`)
   - `get()` - Get agent by ID
   - `get_all_writer_agents()` - Get all Writer agents

5. **NodeRecordStore**: Node storage (`src/store/mod.rs`)
   - `get()` - Get node by NodeID
   - `find_by_path()` - Find node by path (needs implementation)

6. **Path Utilities**: Path canonicalization (`src/tree/path.rs`)
   - `canonicalize_path()` - Canonicalize and normalize path

7. **Error Types**: Use existing `ApiError` variants (`src/error.rs`)
   - `NodeNotFound`
   - `PathNotInTree` (may need to add)
   - `ProviderNotConfigured`
   - `ConfigError` (for missing prompts)

### Error Messages

Follow existing error message patterns:
- Include context (node_id, agent_id, path)
- Suggest remediation (e.g., "Run `merkle scan` to update tree")
- Use consistent formatting

### Output Format

- **Success (sync)**: `"Frame generated: <frame_id_hex>"`
- **Success (async)**: `"Generation enqueued: <request_id>"` or `"Generation queued"`
- **No-op (head exists)**: `"Frame already exists: <frame_id_hex>"`
- **Errors**: Use `ApiError` formatting

### Logging

Use tracing macros:
- `info!()` for successful operations
- `warn!()` for skipped operations (head exists)
- `error!()` for failures
- `debug!()` for detailed flow information

Include relevant context in log fields:
- `node_id`, `agent_id`, `frame_type`, `path`, etc.

