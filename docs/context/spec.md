# Context Management Specification

## Overview

This specification defines the Context Management system for the Merkle filesystem state management tool. The context system provides a convenient interface for generating, retrieving, and managing context frames associated with filesystem nodes.

The suite is centered around `merkle context` commands and supports both NodeID and path-based targeting.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Command Reference](#command-reference)
- [Related Documentation](#related-documentation)
- [Terminology](#terminology)
- [Architecture](#architecture)

---

## Quick Start

### Generate Context

Create a new context frame for a file:

```bash
merkle context generate --path src/lib.rs --agent docs-writer
```

### Retrieve Context

Get context frames for a node:

```bash
merkle context get --path src/lib.rs --max-frames 3 --combine
```

---

## Command Reference

### 1) Generate Context

Create a new context frame for a node using a configured LLM provider.

**Syntax**

```
merkle context generate --node <node_id> [options]
merkle context generate --path <path> [options]
```

**Core Options**
- `--node <node_id>`: Target node by NodeID (hex string).
- `--path <path>`: Target node by workspace-relative or absolute path.
- `--agent <agent_id>`: Agent to use for generation. Required unless a single Writer agent is configured.
- `--frame-type <type>`: Optional; defaults to `context-<agent_id>`.
- `--force`: Generate even if a head frame already exists (default: skip if exists).

**Execution Mode**
- `--sync`: Execute immediately and return the new FrameID (default).
- `--async`: Enqueue generation with `Priority::Urgent` and return a request ID.

**Behavior**
1. Resolve target (node or path).
2. Validate agent and provider configuration.
3. Validate required prompts (system and user templates).
4. If `--force` is not set and an existing head frame is present, return a no-op result.
5. Generate a frame via the configured provider.
6. Persist the frame and return the new FrameID (sync) or queue ID (async).

**Detailed Specification**: See [context_generate_command.md](context_generate_command.md)

### 2) Retrieve Context

Fetch context frames for a node and render them as text or JSON.

**Syntax**

```
merkle context get --node <node_id> [options]
merkle context get --path <path> [options]
```

**Core Options**
- `--node <node_id>` or `--path <path>`: Target node.
- `--agent <agent_id>`: Filter by agent.
- `--frame-type <type>`: Filter by frame type.
- `--max-frames <n>`: Default 10.
- `--ordering <recency|deterministic>`: Default `recency`.
- `--combine`: Concatenate frame contents with a separator.
- `--separator <text>`: Used with `--combine` (default: `\n\n---\n\n`).
- `--format <text|json>`: Output format.
- `--include-metadata`: Include metadata fields in output (JSON or annotated text).

**Behavior**
1. Resolve target (node or path).
2. Load frames using the requested filters and ordering.
3. Render output per format/combination settings.

**Detailed Specification**: See [context_get_command.md](context_get_command.md)

---

## Related Documentation

### Command Implementation Details

- **[context_generate_command.md](context_generate_command.md)** - Detailed specification for `merkle context generate`
  - CLI functionality and structure
  - Required logical guards
  - Test requirements
  - Implementation patterns

- **[context_get_command.md](context_get_command.md)** - Detailed specification for `merkle context get`
  - CLI functionality and structure
  - Required logical guards
  - Test requirements
  - Implementation patterns

### Core API Documentation

- **[../workflow/phase2_apis.md](../workflow/phase2_apis.md)** - Core Context API specifications
  - `get_node()` API
  - `put_frame()` API
  - ContextView and filtering
  - Error handling

- **[../workflow/context_api_ergonomics_spec.md](../workflow/context_api_ergonomics_spec.md)** - API ergonomics and convenience methods
  - Content access methods
  - Query builder patterns
  - Typed accessors
  - Usage examples

### System Architecture

- **[../workflow/phase2_spec.md](../workflow/phase2_spec.md)** - Phase 2 overall specification
  - System goals and outcomes
  - Component architecture
  - Development phases

- **[../workflow/phase2_architecture.md](../workflow/phase2_architecture.md)** - Architecture overview
  - Component relationships
  - Data flow
  - Design patterns

- **[../workflow/phase2_components.md](../workflow/phase2_components.md)** - Component specifications
  - ContextApi
  - Frame storage
  - Head index
  - Agent registry

### Frame Generation

- **[../workflow/frame_generation_queue_spec.md](../workflow/frame_generation_queue_spec.md)** - Frame generation queue
  - Async generation
  - Priority handling
  - Rate limiting
  - Retry logic

- **[../workflow/phase2_model_providers.md](../workflow/phase2_model_providers.md)** - Model provider integration
  - Provider abstraction
  - LLM integration
  - Configuration

### Persistence and Storage

- **[../workflow/head_index_persistence_spec.md](../workflow/head_index_persistence_spec.md)** - Head index persistence
  - Persistence strategy
  - Cross-process sharing
  - Recovery mechanisms

---

## Terminology

- **Context Frame**: Immutable content associated with a node; stored append-only.
- **NodeID**: Deterministic 32-byte hash identifying a filesystem node.
- **Frame Type**: Logical label for a frame; defaults to `context-<agent_id>`.
- **Agent**: Identity with optional provider configuration used for LLM generation.
- **Head**: The latest frame per node and frame type.

---

## Path Resolution and Validation

Path-based commands MUST:
1. Canonicalize paths relative to the configured workspace root.
2. Look up the NodeID in the NodeRecord store.
3. Fail with `PathNotInTree` if the path is not present.

**Recommended error guidance**:
- Suggest running `merkle scan` or starting `merkle watch` if the tree is outdated.
- If path exists on disk but not in the tree, recommend refreshing the workspace index.

---

## Output and Errors

### Standard Outputs
- **Generate**: FrameID (sync) or queue/request ID (async).
- **Get**: Rendered content or JSON object with frames and metadata.

### Common Errors
- `NodeNotFound`: NodeID not in store.
- `PathNotInTree`: Path not present in current tree.
- `ProviderNotConfigured`: Agent lacks provider configuration.
- `MissingPrompts`: Required prompts not present in agent metadata.
- `Unauthorized`: Provider auth failure.
- `RateLimited`: Provider request throttled.

---

## Configuration Integration

This suite relies on the existing configuration system:
- **Agents**: `agents.<name>` defines `agent_id`, role, and provider config.
- **Providers**: `providers.<name>` defines model and endpoint settings.
- **Prompts**: `agents.<name>.system_prompt` and `agents.<name>.metadata` for user prompt templates.

**Defaults**:
- If exactly one Writer agent exists, it is used by default.
- Otherwise, `--agent` is required for generation.

---

## Examples

### Generate Context

```bash
# Generate context for a file using a specific agent
merkle context generate --path src/lib.rs --agent docs-writer

# Generate with force flag (overwrites existing head)
merkle context generate --node 0xabc... --agent local-dev --force

# Generate asynchronously
merkle context generate --path src/lib.rs --agent docs-writer --async
```

### Retrieve Context

```bash
# Get combined text content
merkle context get --path src/lib.rs --max-frames 3 --combine

# Get JSON output with metadata
merkle context get --node 0xabc... --format json --include-metadata

# Get frames from specific agent
merkle context get --path src/lib.rs --agent docs-writer --max-frames 5
```

---

## Future Extensions

- `merkle context list` (enumerate available frames and heads)
- `merkle context diff` (compare head frames over time)
- `merkle context export` (write frames to files or JSONL)
- `merkle context import` (ingest external context frames)

---

## Architecture

The context management system builds on the core Context API:

1. **Path Resolution**: Converts file paths to NodeIDs using the NodeRecord store
2. **Agent Resolution**: Selects appropriate agent based on configuration and flags
3. **Frame Generation**: Uses LLM providers via the ContextApiAdapter
4. **Frame Retrieval**: Queries frames using ContextView policies

All operations maintain the append-only storage model and preserve historical frames. Context frames are immutable and cannot be deleted, ensuring a complete audit trail.

---

[← Back to Phase 2 Spec](../workflow/phase2_spec.md)

