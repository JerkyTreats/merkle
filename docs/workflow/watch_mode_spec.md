# Watch Mode Workflow Specification

## Overview

This specification defines the watch mode workflow for the Merkle filesystem state management system. Watch mode enables the binary to run as a long-lived daemon process that monitors the workspace for filesystem changes and automatically updates the Merkle tree and triggers node regeneration when changes are detected.

## Goals

### Primary Goals
- **Continuous Monitoring**: Run as a background process that watches the workspace for changes
- **Automatic Tree Updates**: Update the Merkle tree when files are created, modified, or deleted
- **Incremental Regeneration**: Trigger regeneration of affected nodes when their basis changes
- **Efficient Change Detection**: Minimize unnecessary work through intelligent batching and debouncing
- **Deterministic Behavior**: Maintain all Phase 1 and Phase 2 invariants (determinism, no search, append-only)

### Secondary Goals
- **Low Latency**: Respond to changes quickly while avoiding excessive processing
- **Resource Efficiency**: Minimize CPU, memory, and I/O usage during idle periods
- **Fault Tolerance**: Recover gracefully from errors and continue monitoring
- **Observability**: Provide logging and metrics for monitoring and debugging

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Watch Mode Daemon                         │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────┐  │
│  │ File Watcher │─────▶│ Event Queue  │─────▶│ Batcher  │  │
│  │  (notify)    │      │              │      │          │  │
│  └──────────────┘      └──────────────┘      └──────────┘  │
│                                                              │
│                              │                               │
│                              ▼                               │
│                    ┌──────────────────┐                      │
│                    │ Change Processor │                      │
│                    └──────────────────┘                      │
│                              │                               │
│         ┌────────────────────┼────────────────────┐         │
│         │                    │                    │         │
│         ▼                    ▼                    ▼         │
│  ┌─────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ Tree Builder│    │ Node Updater │    │ Regenerator  │  │
│  └─────────────┘    └──────────────┘    └──────────────┘  │
│         │                    │                    │         │
│         └────────────────────┼────────────────────┘         │
│                              │                               │
│                              ▼                               │
│                    ┌──────────────────┐                      │
│                    │  Context API    │                      │
│                    └──────────────────┘                      │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### Core Components

#### 1. File Watcher
- **Purpose**: Monitor filesystem for changes using OS-level notifications
- **Implementation**: Uses `notify` crate (already in dependencies)
- **Scope**: Recursive watch of workspace root
- **Events**: Create, Modify, Remove, Rename

#### 2. Event Queue
- **Purpose**: Buffer filesystem events for processing
- **Properties**:
  - Bounded size to prevent memory exhaustion
  - Thread-safe for concurrent access
  - Preserves event ordering

#### 3. Event Batcher
- **Purpose**: Group related events to reduce processing overhead
- **Strategy**:
  - Debounce rapid successive changes to same file
  - Batch events within a time window (default: 100ms)
  - Deduplicate events for same path

#### 4. Change Processor
- **Purpose**: Orchestrate tree updates and regeneration
- **Responsibilities**:
  - Map file paths to NodeIDs
  - Determine affected nodes (changed node + ancestors)
  - Coordinate tree rebuild and regeneration

#### 5. Tree Builder
- **Purpose**: Rebuild Merkle tree for changed subtrees
- **Optimization**: Incremental updates where possible (rebuild only affected subtrees)

#### 6. Node Updater
- **Purpose**: Update NodeRecord store with new NodeIDs
- **Operations**: Insert/update node records, maintain parent-child relationships

#### 7. Regenerator
- **Purpose**: Trigger regeneration of frames whose basis changed
- **Scope**: Only regenerate frames for nodes whose basis hash changed

## Workflow

### Initialization

1. **Load Configuration**
   - Read workspace root from CLI args or config
   - Load agent registry and provider configurations
   - Initialize storage backends (NodeRecord store, Frame storage)

2. **Build Initial Tree**
   - Perform full filesystem scan using `TreeBuilder`
   - Populate NodeRecord store with all nodes
   - Compute initial workspace root hash

3. **Initialize Watcher**
   - Create file watcher for workspace root (recursive)
   - Set up event channel for receiving notifications
   - Start watcher thread

4. **Start Event Loop**
   - Begin processing events from watcher
   - Initialize event queue and batcher

### Change Detection Workflow

```
File System Event
    │
    ▼
Event Queue (buffering)
    │
    ▼
Event Batcher (debouncing, deduplication)
    │
    ▼
Change Processor
    │
    ├─▶ Map path to NodeID (if exists)
    │
    ├─▶ Determine affected nodes:
    │   - Changed file/directory node
    │   - All ancestor nodes (up to root)
    │
    ├─▶ Rebuild affected subtree:
    │   - Read current filesystem state
    │   - Compute new NodeIDs for changed nodes
    │   - Compute new NodeIDs for ancestors
    │
    ├─▶ Update NodeRecord Store:
    │   - Insert/update changed nodes
    │   - Update parent-child relationships
    │   - Update root hash if root changed
    │
    └─▶ Trigger Regeneration:
        - For each changed node:
          - Detect basis changes (compare stored vs current basis hash)
          - Regenerate frames with changed basis
          - Update basis index
          - Update head pointers
```

### Event Processing Details

#### Event Types and Handling

1. **File Created** (`EventKind::Create`)
   - Add new file node to tree
   - Update parent directory node
   - Regenerate parent directory frames if they depend on children

2. **File Modified** (`EventKind::Modify`)
   - Recompute file NodeID (content hash changed)
   - Update file node in store
   - Update parent directory node (child hash changed)
   - Regenerate frames based on this node
   - Regenerate parent frames if they depend on children

3. **File Removed** (`EventKind::Remove`)
   - Remove node from tree
   - Update parent directory node (child removed)
   - Regenerate parent directory frames
   - Note: Frames for removed node are preserved (append-only)

4. **File Renamed** (`EventKind::Rename`)
   - Treated as Remove + Create
   - Path-based NodeID changes, so node is effectively new
   - Old frames remain associated with old NodeID

#### Batching Strategy

**Debouncing Window**: 100ms (configurable)
- Events for the same path within the window are collapsed
- Only the latest event is processed
- Prevents excessive processing during rapid edits

**Batch Processing**: 50ms (configurable)
- Events within a batch are processed together
- Enables optimization (e.g., rebuild directory once for multiple child changes)
- Reduces lock contention

**Deduplication**:
- Same path + same event type within debounce window → single event
- Prevents redundant processing

#### Incremental Tree Updates

**Strategy**: Rebuild only affected subtrees

1. **Identify Changed Paths**: From batched events, collect all changed paths
2. **Find Affected Subtrees**:
   - For each changed path, find its parent directory
   - Collect all directories that need recomputation (ancestors up to root)
3. **Rebuild Subtrees**:
   - Process files first (no dependencies)
   - Process directories bottom-up (children before parents)
   - Only rebuild nodes whose children changed or whose content changed

**Optimization**:
- If only file content changed (not structure), only recompute that file's NodeID
- If directory structure changed, recompute directory NodeID and ancestors
- Avoid full tree rebuild when possible

### Regeneration Workflow

#### Basis Change Detection

For each node that changed:

1. **Get Current Head Frames**: Query head index for all frame types
2. **For Each Frame Type**:
   - Get stored basis hash from basis index
   - Compute current basis hash:
     - For file nodes: `hash(node_content + metadata)`
     - For synthesized frames: `hash(child_frame_ids + synthesis_policy)`
   - Compare stored vs current
   - If different → mark for regeneration

#### Regeneration Execution

For each frame marked for regeneration:

1. **Acquire Node Lock**: Prevent concurrent modifications
2. **Retrieve Current Basis**:
   - For file nodes: Read file content
   - For synthesized frames: Collect child frames
3. **Regenerate Frame**:
   - Use same synthesis policy as original frame
   - Compute new frame content
   - Generate new FrameID
4. **Store New Frame**: Append to frame storage (append-only)
5. **Update Indices**:
   - Add to basis index with new basis hash
   - Update head pointer to new frame
   - Old frame remains in storage (history preserved)
6. **Propagate to Parents** (if recursive):
   - If parent frames depend on this frame, mark for regeneration
   - Limit propagation depth (configurable, default: 3 levels)

#### Regeneration Scope

**Default**: Only regenerate frames for directly changed nodes
**Recursive Option**: Regenerate frames for changed nodes and their ancestors (up to N levels)

**Limits**:
- Maximum propagation depth: 3 levels (configurable)
- Maximum frames per batch: 100 (configurable)
- Timeout per regeneration: 5 seconds (configurable)

## Configuration

### Watch Mode Configuration

```toml
[watch]
# Enable watch mode
enabled = true

# Workspace root directory (default: current directory)
workspace_root = "."

# Event batching configuration
[watch.batching]
# Debounce window in milliseconds
debounce_ms = 100

# Batch processing window in milliseconds
batch_window_ms = 50

# Maximum events per batch
max_batch_size = 100

# Regeneration configuration
[watch.regeneration]
# Enable automatic regeneration
enabled = true

# Recursive regeneration (regenerate parent frames)
recursive = false

# Maximum propagation depth for recursive regeneration
max_depth = 3

# Maximum frames to regenerate per batch
max_frames_per_batch = 100

# Timeout per regeneration operation (seconds)
timeout_seconds = 5

# Agent ID for automatic regeneration
agent_id = "watch-daemon"

# File watching configuration
[watch.filesystem]
# Ignore patterns (glob patterns)
ignore_patterns = [
    "**/.git/**",
    "**/.merkle/**",
    "**/target/**",
    "**/node_modules/**",
    "**/.DS_Store",
    "**/*.swp",
    "**/*.tmp"
]

# Watch only specific file extensions (empty = all files)
# watch_extensions = [".rs", ".toml", ".md"]

# Performance configuration
[watch.performance]
# Maximum event queue size
max_queue_size = 10000

# Number of worker threads for processing
worker_threads = 4

# Logging configuration
[watch.logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Log file path (empty = stdout)
log_file = ""

# Enable structured logging (JSON)
structured = false
```

## CLI Interface

### Watch Command

```bash
# Start watch mode daemon
merkle watch [OPTIONS]

Options:
  --workspace <PATH>        Workspace root directory (default: current directory)
  --config <PATH>           Configuration file path
  --debounce-ms <MS>        Debounce window in milliseconds (default: 100)
  --batch-window-ms <MS>    Batch window in milliseconds (default: 50)
  --recursive               Enable recursive regeneration
  --max-depth <N>           Maximum regeneration depth (default: 3)
  --agent-id <ID>           Agent ID for regeneration (default: "watch-daemon")
  --ignore <PATTERN>        Ignore pattern (can be specified multiple times)
  --log-level <LEVEL>       Log level: trace, debug, info, warn, error
  --log-file <PATH>         Log file path (default: stdout)
  --foreground              Run in foreground (default: background daemon)
  --pid-file <PATH>         PID file path (default: .merkle/watch.pid)
```

### Example Usage

```bash
# Start watch mode with default settings
merkle watch

# Start watch mode with custom configuration
merkle watch --workspace /path/to/workspace \
            --debounce-ms 200 \
            --recursive \
            --max-depth 5 \
            --log-level debug

# Start in foreground for debugging
merkle watch --foreground --log-level trace

# Stop watch mode daemon
merkle watch --stop

# Check watch mode status
merkle watch --status
```

## Error Handling

### Error Categories

1. **Filesystem Errors**
   - File read failures
   - Permission denied
   - Path not found
   - **Recovery**: Log error, skip file, continue watching

2. **Tree Build Errors**
   - Invalid file content
   - Circular symlinks
   - **Recovery**: Log error, mark node as invalid, continue

3. **Storage Errors**
   - Store write failures
   - Lock acquisition timeouts
   - **Recovery**: Retry with exponential backoff, log error

4. **Regeneration Errors**
   - Frame synthesis failures
   - Basis computation errors
   - **Recovery**: Log error, skip frame, continue with other frames

5. **Watcher Errors**
   - Watcher initialization failures
   - Event channel errors
   - **Recovery**: Reinitialize watcher, log error

### Error Recovery Strategies

**Retry Logic**:
- Transient errors: Retry with exponential backoff (max 3 retries)
- Permanent errors: Log and skip, continue processing

**Graceful Degradation**:
- If tree rebuild fails: Log error, continue watching (will retry on next change)
- If regeneration fails: Log error, continue with other frames
- If watcher fails: Attempt to reinitialize, log error

**Error Logging**:
- All errors logged with context (path, node_id, error type)
- Structured logging for debugging
- Error metrics for monitoring

## Performance Considerations

### Optimization Strategies

1. **Incremental Updates**
   - Only rebuild affected subtrees
   - Cache directory contents where possible
   - Avoid full tree scans

2. **Batching and Debouncing**
   - Group related events
   - Reduce lock contention
   - Minimize I/O operations

3. **Parallel Processing**
   - Process independent subtrees in parallel
   - Regenerate frames concurrently (with proper locking)
   - Use worker thread pool

4. **Caching**
   - Cache file content hashes (invalidate on change)
   - Cache directory structures (invalidate on change)
   - Cache basis hashes for unchanged nodes

5. **Lazy Evaluation**
   - Only regenerate frames when explicitly needed
   - Defer expensive operations until necessary

### Performance Targets

- **Event Processing Latency**: < 50ms p50, < 200ms p99
- **Tree Update Latency**: < 100ms for single file change, < 500ms for directory change
- **Regeneration Latency**: < 200ms per frame p50, < 1s p99
- **Memory Usage**: < 100MB idle, < 500MB under load
- **CPU Usage**: < 5% idle, < 50% under load

## Observability

### Logging

**Log Levels**:
- `TRACE`: All events, detailed processing steps
- `DEBUG`: Event processing, tree updates, regeneration
- `INFO`: Significant events (file changes, tree updates, regeneration)
- `WARN`: Recoverable errors, performance issues
- `ERROR`: Unrecoverable errors, system failures

**Structured Logging**:
```json
{
  "timestamp": "2024-01-01T12:00:00Z",
  "level": "info",
  "event": "file_changed",
  "path": "/workspace/src/main.rs",
  "node_id": "abc123...",
  "action": "regenerated",
  "frames_regenerated": 2,
  "duration_ms": 45
}
```

### Metrics

**Event Metrics**:
- Events received per second
- Events processed per second
- Event queue size
- Event processing latency

**Tree Metrics**:
- Tree updates per second
- Nodes updated per second
- Tree rebuild duration

**Regeneration Metrics**:
- Frames regenerated per second
- Regeneration duration
- Regeneration failures

**System Metrics**:
- Memory usage
- CPU usage
- Storage I/O operations
- Lock contention

## Implementation Phases

### Phase 1: Basic Watch Mode
- [ ] Implement file watcher integration
- [ ] Implement event queue and basic processing
- [ ] Implement tree updates on file changes
- [ ] Basic error handling and logging
- [ ] CLI command for watch mode

### Phase 2: Event Batching and Optimization
- [ ] Implement event debouncing
- [ ] Implement event batching
- [ ] Implement incremental tree updates
- [ ] Performance optimization

### Phase 3: Regeneration Integration
- [ ] Integrate regeneration on basis changes
- [ ] Implement recursive regeneration
- [ ] Regeneration error handling
- [ ] Regeneration metrics

### Phase 4: Production Readiness
- [ ] Comprehensive error recovery
- [ ] Observability (metrics, structured logging)
- [ ] Performance tuning
- [ ] Documentation and examples

## Testing Strategy

### Unit Tests
- Event batching and debouncing logic
- Tree update algorithms
- Regeneration triggers
- Error recovery

### Integration Tests
- End-to-end watch mode workflow
- Multiple file changes
- Concurrent modifications
- Error scenarios

### Performance Tests
- Event processing throughput
- Tree update latency
- Regeneration performance
- Resource usage under load

## Security Considerations

### File Access
- Respect filesystem permissions
- Don't watch files outside workspace root
- Validate paths before processing

### Resource Limits
- Limit event queue size
- Limit concurrent operations
- Timeout long-running operations

### Isolation
- Watch mode runs with same permissions as user
- No elevation of privileges
- Sandboxed to workspace root

## Future Enhancements

### Potential Improvements
1. **Smart Ignore Patterns**: Learn from user behavior
2. **Change Prediction**: Pre-compute likely changes
3. **Distributed Watching**: Watch multiple workspaces
4. **Webhook Integration**: Notify external systems of changes
5. **Change History**: Track change history over time
6. **Selective Watching**: Watch only specific subtrees

## References

- [Phase 1 Specification](../bootstrap/phase1_spec.md)
- [Phase 2 Specification](phase2_spec.md)
- [Phase 2 Components](phase2_components.md)
- [Phase 2 APIs](phase2_apis.md)
- [notify crate documentation](https://docs.rs/notify/)
