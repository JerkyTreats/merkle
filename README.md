# Merkle: Deterministic Filesystem State Management

A Merkle-based filesystem state management system that provides deterministic, hash-based tracking of filesystem state and associated context. The system enables fast, scan-free traversal of nodes and attached context, with append-only and verifiable context frames.

## Overview

This system provides a deterministic foundation for tracking filesystem state and context using Merkle trees and Merkle sets. It enables:

- **Deterministic Identity**: Same filesystem state → same root hash
- **Fast Lookups**: O(1) node lookup, O(1) head resolution, O(log n) set operations
- **Append-Only Context**: Immutable, content-addressed context frames
- **Hash-Based Invalidation**: Changes detected only through hash comparison
- **Bounded Context Views**: Policy-driven, deterministic frame selection
- **Agent Workflows**: Read/write APIs for agent-driven context management
- **Multi-Frame Composition**: Policy-driven composition of related context frames

## Core Principles

### Determinism
All operations are deterministic: same inputs → same outputs (hashes, IDs, frame sets). No random number generation, time-dependent behavior, or external API calls in core paths.

### No Search
No semantic search, full scans, or fuzzy matching. All lookups are hash-based (O(1) or O(log n)). The system never requires scanning frame storage.

### Append-Only
Frames and nodes are immutable. New context creates new frames; existing frames are never modified. History is preserved.

### Hash-Based Invalidation
Changes are detected only through hash comparison. No polling, file watching, or content-based diffing required.

### Bounded Context
Context views have maximum frame counts. Memory usage is bounded per operation.

## Architecture

The system is built in three phases:

### Phase 1: Bootstrap Core Components
Establishes the deterministic, Merkle-addressed foundation for filesystem state and context.

**Components:**
1. **Filesystem Merkle Tree**: Deterministic representation of filesystem structure
2. **NodeRecord Store**: Fast lookup storage for node metadata
3. **Context Frames**: Immutable, append-only context containers
4. **Context Frame Merkle Set**: Deterministic frame set membership
5. **Frame Heads**: Efficient pointers to latest frames
6. **Context Views**: Policy-driven frame selection

**Key Outcomes:**
- Stable workspace root hash
- Stable NodeID and FrameID generation
- Bounded, deterministic context retrieval
- Hash-based invalidation only

📖 **[Phase 1 Documentation](docs/bootstrap/phase1_spec.md)**

### Phase 2: Construct Workflows & Integrations
Enables agent-driven workflows, deterministic context retrieval, and composition flows.

**Components:**
1. **Agent Read/Write Model**: Defines how agents interact with nodes and frames
2. **Context APIs**: Minimal, stateless API surface (GetNode, PutFrame)
3. **Multi-Frame Composition**: Combining multiple frames into composite views
4. **Tooling & Integration Layer**: CLI tools, editor hooks, CI integration

**Key Outcomes:**
- Agents can read and write context frames via stable APIs
- Workflows operate without global rescans or semantic search

📖 **[Phase 2 Documentation](docs/workflow/phase2_spec.md)**

### Phase 3: Prepare for External Use
Stabilizes the system for external consumption with versioning, isolation, observability, and operational readiness.

**Components:**
1. **Public API Contracts & Versioning**: Versioned, stable API surfaces with backward compatibility
2. **Workspace Isolation & Access Control**: Multi-tenant isolation and fine-grained access control
3. **Snapshot Export, Verification, and Replay**: Export/import workflows with integrity verification
4. **Observability & Diagnostics**: Metrics, logging, and determinism diagnostics
5. **Performance Hardening**: Batching, caching, and concurrency controls
6. **Pluggable Backends & Portability**: Swappable storage and compression backends
7. **Documentation & Developer Experience**: Comprehensive docs and reference implementations

**Key Outcomes:**
- Versioned public API and schemas with backward compatibility guarantees
- Workspace isolation and access controls with audit logging
- Exportable/verifiable snapshots and deterministic replay
- Operational observability and performance hardening
- Production-ready system suitable for external adoption

📖 **[Phase 3 Documentation](docs/productionize/phase3_spec.md)**

## Data Flow

```
Filesystem Changes
    ↓
Filesystem Merkle Tree (recompute)
    ↓
NodeID Changes
    ↓
NodeRecord Store (update)
    ↓
Frame Set Invalidation
    ↓
Context Frame Merkle Set (update)
    ↓
Frame Heads (update)
    ↓
Context Views (select frames)
    ↓
Agent Consumption
```

## Core Concepts

### NodeID
Deterministic hash of a filesystem node (file or directory). Computed from:
- Path (canonicalized)
- Content (for files) or children hashes (for directories)
- Metadata (size, type, etc.)

Same content → same NodeID.

### FrameID
Deterministic hash of a context frame. Computed from:
- Basis (NodeID, previous FrameID, or both)
- Content (blob)
- Frame type

Same basis + content → same FrameID.

### Context Frames
Immutable containers for context information associated with nodes. Each frame is:
- **Content-addressed**: FrameID = hash(content + basis)
- **Append-only**: Never modified, only new frames created
- **Basis-driven**: Explicitly declares what it's based on

### Frame Sets
Deterministic set of frames for each node using a Merkle set structure. Enables:
- Efficient membership verification
- Set comparison through hash-based operations
- Deterministic ordering (same frames → same set root)

### Frame Heads
O(1) access to the "latest" frame for a given node and frame type. Enables fast access without scanning frame sets.

### Context Views
Selects and orders a bounded set of frames based on policies (recency, type, agent). Ensures deterministic, bounded context retrieval.

## Technology Stack

- **Language**: Rust (1.70+)
- **Hash Algorithm**: BLAKE3 (primary) or SHA-256 (fallback)
- **Storage**: Pluggable backends (SQLite, RocksDB, Badger, etc.)
- **Serialization**: Bincode (binary) or JSON (debugging)

## API Surface

### Core Operations

#### GetNode
Retrieve node context using policy-driven view.

```rust
async fn get_node(
    node_id: NodeID,
    view: ContextView,
) -> Result<NodeContext, ApiError>;
```

#### PutFrame
Append new frame to node's frame set.

```rust
async fn put_frame(
    node_id: NodeID,
    frame: Frame,
    agent_id: String,
) -> Result<FrameID, ApiError>;
```

📖 **[Phase 2 API Documentation](docs/workflow/phase2_apis.md)**
📖 **[Phase 3 API Documentation](docs/productionize/phase3_api.md)**

## Performance Targets

### Core Operations
- **NodeID computation**: < 1ms per node
- **NodeRecord lookup**: < 1ms (O(1) hash table)
- **Frame creation**: < 1ms (hash computation)
- **Frame set update**: < 10ms (O(log n) Merkle set)
- **Head resolution**: < 1ms (O(1) hash table)
- **Context view construction**: < 10ms for 100 frames

### API Operations
- **GetNode**: < 10ms p50, < 50ms p99 (with bounded view)
- **PutFrame**: < 5ms p50, < 20ms p99

## Documentation Structure

### Phase 1: Bootstrap
- **[Specification](docs/bootstrap/phase1_spec.md)**: Goals, components, APIs, constraints
- **[Architecture](docs/bootstrap/phase1_architecture.md)**: Component relationships and system properties
- **[Components](docs/bootstrap/phase1_components.md)**: Detailed component specifications
- **[Merkle Implementation](docs/bootstrap/merkle_implementation.md)**: Algorithm research and design decisions
- **[Implementation](docs/bootstrap/phase1_implementation.md)**: Rust-specific implementation details
- **[Phases](docs/bootstrap/phase1_phases.md)**: Development task breakdown

### Phase 2: Workflow
- **[Specification](docs/workflow/phase2_spec.md)**: Goals, components, APIs, constraints
- **[Architecture](docs/workflow/phase2_architecture.md)**: Component relationships and data flow
- **[Components](docs/workflow/phase2_components.md)**: Detailed component specifications
- **[APIs](docs/workflow/phase2_apis.md)**: API signatures and examples
- **[Phases](docs/workflow/phase2_phases.md)**: Development task breakdown

### Phase 3: Productionize
- **[Specification](docs/productionize/phase3_spec.md)**: Goals, components, APIs, constraints
- **[Components](docs/productionize/phase3_components.md)**: Detailed component specifications
- **[APIs](docs/productionize/phase3_api.md)**: Public API surface and error handling
- **[Phases](docs/productionize/phase3_phases.md)**: Development task breakdown

## Development Status

All phases are currently in planning/specification stage. Implementation tasks are tracked in the phase-specific documentation.

### Phase 1 Exit Criteria
- ✅ Deterministic ingestion: Same filesystem → same root hash
- ✅ Stable NodeID / FrameID: Same content → same IDs
- ✅ Zero-scan context retrieval: O(1) or O(log n) access, no full scans
- ✅ Hash-based invalidation: Changes detected only through hash comparison
- ✅ Bounded context views: Context retrieval is bounded and deterministic
- ✅ All components operational: All six components implemented and tested

### Phase 2 Exit Criteria
- ✅ Agents can reliably read and write context
- ✅ Workflows compose without search or mutation

### Phase 3 Exit Criteria
- ✅ Public API and schemas are versioned and stable
- ✅ Workspaces are isolated with enforced access controls
- ✅ Snapshots can be exported, verified, and replayed deterministically
- ✅ System is observable and debuggable under real workloads
- ✅ Performance targets are met
- ✅ Backends are swappable without semantic drift
- ✅ Documentation enables external adoption

## Constraints & Non-Goals

### What This System Does
- ✅ Deterministic, hash-based filesystem state tracking
- ✅ Append-only context frames with immutable history
- ✅ Fast, O(1) or O(log n) lookups without scanning
- ✅ Agent-driven workflows with read/write APIs
- ✅ Multi-tenant workspace isolation

### What This System Does NOT Do
- ❌ Semantic search or fuzzy matching
- ❌ Frame deletion or modification
- ❌ Global queries across entire workspace
- ❌ Real-time collaboration or live sync
- ❌ Version control integration (Git/SVN)
- ❌ Distributed storage (single deployment per workspace)
- ❌ Frame encryption (storage-level encryption OK)
- ❌ Advanced access control (basic ACLs only)

## License

[License information to be added]

## Contributing

[Contributing guidelines to be added]
