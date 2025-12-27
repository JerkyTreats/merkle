# Phase 2 Spec — Construct Workflows & Integrations

## Overview

This specification documents Phase 2 of the Merkle-based filesystem state management system. Phase 2 builds upon the Phase 1 foundation to enable agent-driven workflows, deterministic context synthesis, and incremental regeneration while preserving all Phase 1 invariants.

## Table of Contents

- [Goals + Outcomes](#goals--outcomes)
- [Dependencies & Assumptions](#dependencies--assumptions)
- [Major Components](phase2_components.md)
- [Architecture Overview](phase2_architecture.md)
- [API Specifications](phase2_apis.md)
- [Error Handling](#error-handling)
- [Performance Considerations](#performance-considerations)
- [Constraints & Non-Goals](#constraints--non-goals)
- [Development Phases](phase2_phases.md)
- [Phase Exit Criteria](#phase-exit-criteria)

---

## Goals + Outcomes

### Goals
- Enable agent-driven workflows on top of the Phase 1 substrate
- Support deterministic context synthesis and regeneration
- Integrate context engine with internal tools and agents
- Preserve all Phase 1 invariants (determinism, no search, no mutation)

### Outcomes
- Agents can read and write context frames via stable APIs
- Branch- and directory-level context is synthesized incrementally
- Context regeneration is localized and basis-driven
- Workflows operate without global rescans or semantic search

---

## Dependencies & Assumptions

### Phase 1 Prerequisites
Phase 2 assumes the following Phase 1 components are complete and operational:

- **Filesystem Merkle Tree**: Stable NodeID generation and root hash computation
- **NodeRecord Store**: O(1) node lookup by NodeID
- **Context Frames**: Immutable, append-only frame storage with deterministic FrameIDs
- **Context Frame Merkle Set**: Deterministic frame set membership tracking
- **Frame Heads**: O(1) head resolution by (NodeID, type)
- **Context Views**: Policy-driven, bounded frame selection

### System Invariants (Preserved from Phase 1)
- **Determinism**: Same inputs → same outputs (hashes, IDs, frame sets)
- **No Search**: No semantic search, full scans, or fuzzy matching
- **No Mutation**: Append-only operations; frames and nodes are immutable
- **Hash-Based Invalidation**: Changes detected only through hash comparison
- **Bounded Context**: Context views are bounded (max frame count)

### Assumptions
- Agents have stable identities (agent_id: String)
- Frame types are explicitly declared (frame_type: String)
- Workspace root is stable during workflow execution
- Concurrent access is handled via appropriate locking mechanisms
- LLM providers use OpenAI-compatible API format (for local and cloud providers)
- Provider responses may be non-deterministic (acceptable; FrameID based on inputs, not outputs)

---

## Major Components

Phase 2 consists of eight core components that enable agent-driven workflows:

1. **Agent Read / Write Model**: Defines how agents interact with nodes and context frames
2. **Context APIs (Core Workflows)**: Minimal, stateless API surface for agent interaction
3. **Branch Context Synthesis**: Directory-level aggregation of child node context
4. **Incremental Regeneration**: Rebuilds derived context frames when bases change
5. **Multi-Frame Composition**: Combining multiple context frames into composite views
6. **Model Provider Abstraction**: Unified interface for multiple LLM providers (OpenAI, Anthropic, local)
7. **Configuration System**: Runtime-driven configuration for agents, providers, and system settings
8. **Tooling & Integration Layer**: CLI tools, editor hooks, CI integration, and agent adapters

For detailed component specifications, see **[Component Specifications](phase2_components.md)**.

---

## Component Relationships

For detailed architecture and component relationships, see **[Architecture Overview](phase2_architecture.md)**.

---

## API Specifications

For detailed API specifications with examples, see **[API Specifications](phase2_apis.md)**.

---

## Error Handling

### Error Types

#### ApiError
```rust
#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("Node not found: {0:?}")]
    NodeNotFound(NodeID),

    #[error("Frame not found: {0:?}")]
    FrameNotFound(FrameID),

    #[error("Agent unauthorized: {0}")]
    Unauthorized(String),

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("Regeneration failed: {0}")]
    RegenerationFailed(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Provider not configured: {0}")]
    ProviderNotConfigured(String),

    #[error("Provider request failed: {0}")]
    ProviderRequestFailed(String),

    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
}
```

### Error Handling Principles
- **Deterministic**: Same error conditions → same error responses
- **No Panics**: All errors returned as Result types
- **Clear Messages**: Error messages are actionable
- **Error Propagation**: Errors bubble up with context
- **Graceful Degradation**: Missing frames/components handled gracefully

### Error Scenarios

#### Node Not Found
- **Cause**: NodeID doesn't exist in NodeRecord Store
- **Response**: Return `NodeNotFound` error
- **Recovery**: Agent should verify NodeID or trigger scan

#### Frame Not Found
- **Cause**: FrameID doesn't exist in frame storage
- **Response**: Return `FrameNotFound` error
- **Recovery**: Agent should use different FrameID or regenerate

#### Unauthorized Agent
- **Cause**: Agent lacks required permissions (e.g., reader trying to write)
- **Response**: Return `Unauthorized` error
- **Recovery**: Agent should use appropriate role or request access

#### Synthesis Failure
- **Cause**: Missing child frames, invalid policy, or computation error
- **Response**: Return `SynthesisFailed` with reason
- **Recovery**: Agent should ensure prerequisites met or use different policy

---

## Performance Considerations

### Performance Targets

#### API Operations
- **GetNode**: < 10ms for bounded view (100 frames)
- **PutFrame**: < 5ms (frame creation + head update)
- **SynthesizeBranch**: < 50ms for directory with 100 children
- **Regenerate**: < 100ms per node (incremental, only changed frames)

#### Storage Operations
- **NodeID lookup**: < 1ms (O(1) hash table)
- **Frame retrieval**: < 5ms per frame (content-addressed storage)
- **Head resolution**: < 1ms (O(1) hash table)
- **Frame set operations**: < 10ms (O(log n) Merkle set)

### Optimization Strategies

#### Caching
- **Head Index**: In-memory cache for frequently accessed heads
- **NodeRecord Cache**: LRU cache for recently accessed nodes
- **Frame Content Cache**: Optional cache for frequently read frames

#### Batch Operations
- **Batch Frame Retrieval**: Retrieve multiple frames in single operation
- **Batch Synthesis**: Synthesize multiple directories in parallel
- **Batch Regeneration**: Regenerate multiple nodes efficiently

#### Lazy Evaluation
- **Frame Content**: Load frame content only when needed
- **Composition**: Compute composition on-demand, not pre-computed
- **Synthesis**: Only synthesize when explicitly requested

### Scalability Considerations
- **Frame Count**: System handles millions of frames (bounded views keep queries fast)
- **Node Count**: System handles large filesystems (O(1) node lookup)
- **Concurrent Access**: Multiple agents can operate simultaneously (proper locking)
- **Storage Growth**: Append-only storage grows linearly (old frames can be archived)

---

## Constraints & Non-Goals

### Constraints

#### Determinism Requirement
- All operations must be deterministic
- No random number generation in core paths
- No time-dependent behavior (except metadata)
- No external API calls that could vary

#### No Search Constraint
- No semantic search or fuzzy matching
- No full scans of frame storage
- No content-based queries (only hash-based)
- No machine learning or AI in core engine

#### Append-Only Constraint
- Frames are immutable once created
- Nodes are immutable (new state = new NodeID)
- No deletion or modification of existing data
- History is preserved (can archive old data)

#### Bounded Context Constraint
- Context views have maximum frame count
- Composition results are bounded
- No unbounded frame retrieval
- Memory usage is bounded per operation

### Non-Goals (Out of Scope for Phase 2)

#### Not Included
- **Semantic Search**: No content-based search or similarity matching
- **Frame Deletion**: No deletion of frames (append-only)
- **Frame Modification**: No mutation of existing frames
- **Global Queries**: No queries across entire workspace
- **Real-time Collaboration**: No live sync or conflict resolution
- **Version Control Integration**: No direct Git/SVN integration
- **Distributed Storage**: No multi-machine storage (single workspace)
- **Frame Encryption**: No encryption of frame content
- **Access Control**: Basic agent roles only, no fine-grained permissions
- **Performance Monitoring**: No built-in metrics or profiling

#### Future Phases
- **Phase 3**: May add semantic search, advanced queries
- **Phase 4**: May add distributed storage, replication
- **Phase 5**: May add encryption, advanced access control

---

## Development Phases

For detailed task breakdown and exit criteria, see **[Development Phases](phase2_phases.md)**.

---

## Quick Links

- **[Component Specifications](phase2_components.md)** - Detailed specifications for each component
- **[Architecture Overview](phase2_architecture.md)** - System architecture and component relationships
- **[API Specifications](phase2_apis.md)** - Detailed API signatures with examples
- **[Model Provider Specification](phase2_model_providers.md)** - Model provider abstraction and integration
- **[Configuration Specification](phase2_configuration.md)** - Runtime configuration system for agents, providers, and system settings
- **[Watch Mode Workflow](watch_mode_spec.md)** - Watch mode daemon for automatic tree updates and regeneration
- **[Development Phases](phase2_phases.md)** - Task breakdown and exit criteria

---

## Phase Exit Criteria

Phase 2 is complete when:
- Agents can reliably read and write context
- Branch context is synthesized incrementally
- Regeneration is minimal and deterministic
- Workflows compose without search or mutation
