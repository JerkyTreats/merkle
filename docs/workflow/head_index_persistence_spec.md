# Head Index Persistence Specification

## Overview

This specification defines the implementation of persistent storage for the Head Index, enabling the system to maintain frame head pointers across process restarts and between different CLI invocations.

## Problem Statement

### Current State

The Head Index is currently an in-memory data structure (`HashMap<(NodeID, String), FrameID>`) that provides O(1) lookup of the latest frame for each `(node_id, frame_type)` pair. However, this index is lost when:

1. The watch daemon process exits
2. A CLI command completes
3. The system is restarted

### Impact

- **Frame Loss Visibility**: Frames are stored on disk, but cannot be efficiently queried without the head index
- **Cross-Process Incompatibility**: Watch daemon and CLI commands cannot share frame state
- **No Persistence**: System must rebuild state from scratch on each startup
- **User Confusion**: Users see "No head frames found" even though frames exist on disk

### Example Scenario

1. Watch daemon creates frames and updates in-memory head index
2. Watch daemon exits (or user runs separate CLI command)
3. CLI command starts with empty head index
4. `get-head` command returns "No head frame found" despite frames existing on disk

## Goals

### Primary Goals

1. **Persistence**: Head index survives process restarts
2. **Cross-Process Sharing**: Watch daemon and CLI commands share the same head index
3. **Performance**: Persistence should not significantly impact frame creation/update performance
4. **Consistency**: Head index remains consistent with frame storage
5. **Backward Compatibility**: System works even if persistence file is missing or corrupted

### Secondary Goals

1. **Atomic Updates**: Head index updates are atomic (no partial writes)
2. **Efficient Loading**: Fast startup time when loading head index
3. **Recovery**: Ability to rebuild head index from frame storage if persistence file is corrupted
4. **Observability**: Logging for persistence operations

## Architecture

### Design Approach

**Option 1: File-Based Persistence (Selected)**

Store head index as a serialized file on disk, updated atomically on each head change.

**Advantages:**
- Simple implementation
- Fast reads/writes for small to medium indexes
- Easy to debug (human-readable with JSON, or compact with bincode)
- No external dependencies

**Disadvantages:**
- File I/O on every head update (can be optimized with batching)
- Potential for file corruption (mitigated with atomic writes)

**Option 2: Database-Backed (Alternative)**

Store head index in the same database as node records (e.g., sled).

**Advantages:**
- Transactional consistency with node records
- Better performance for large indexes
- Atomic updates with other operations

**Disadvantages:**
- More complex implementation
- Requires database schema changes
- Tighter coupling with storage layer

### Selected Approach: File-Based Persistence

We will implement file-based persistence with the following characteristics:

1. **Storage Location**: `.merkle/head_index.bin` (or `.merkle/head_index.json` for debugging)
2. **Update Strategy**: Atomic writes using temporary files + rename
3. **Serialization Format**: Bincode (compact, fast) with optional JSON for debugging
4. **Loading Strategy**: Load on startup, fallback to empty index if file missing/corrupted
5. **Update Frequency**: Immediate (on each head update) with optional batching optimization

## Implementation

### Storage Format

#### Binary Format (Bincode)

```rust
// Serialized structure
struct HeadIndexPersistence {
    version: u32,  // Format version for migration
    entries: Vec<HeadIndexEntry>,
}

struct HeadIndexEntry {
    node_id: [u8; 32],
    frame_type: String,
    frame_id: [u8; 32],
}
```

#### File Structure

```
.merkle/
  ├── head_index.bin          # Binary format (production)
  ├── head_index.bin.tmp      # Temporary file for atomic writes
  └── head_index.json         # Optional JSON format (debugging)
```

### API Changes

#### HeadIndex Extensions

```rust
impl HeadIndex {
    /// Create a new empty head index
    pub fn new() -> Self { ... }

    /// Load head index from disk
    ///
    /// Returns an empty index if the file doesn't exist or is corrupted.
    pub fn load_from_disk<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> { ... }

    /// Save head index to disk atomically
    ///
    /// Uses temporary file + rename for atomic writes.
    pub fn save_to_disk<P: AsRef<Path>>(&self, path: P) -> Result<(), StorageError> { ... }

    /// Get the persistence path for a workspace root
    pub fn persistence_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".merkle").join("head_index.bin")
    }
}
```

#### ContextApi Integration

```rust
impl ContextApi {
    /// Update head index with persistence
    ///
    /// Updates the in-memory index and persists to disk atomically.
    fn update_head_with_persistence(
        &self,
        node_id: &NodeID,
        frame_type: &str,
        frame_id: &FrameID,
        workspace_root: Option<&Path>,
    ) -> Result<(), ApiError> {
        // Update in-memory index
        {
            let mut head_index = self.head_index.write();
            head_index.update_head(node_id, frame_type, frame_id)?;
        }

        // Persist to disk if workspace root provided
        if let Some(root) = workspace_root {
            let head_index = self.head_index.read();
            head_index.save_to_disk(HeadIndex::persistence_path(root))?;
        }

        Ok(())
    }
}
```

#### CliContext Changes

```rust
impl CliContext {
    pub fn new(workspace_root: PathBuf, config_path: Option<PathBuf>) -> Result<Self, ApiError> {
        // ... existing initialization ...

        // Load head index from disk, or create empty if not found
        let head_index_path = HeadIndex::persistence_path(&workspace_root);
        let head_index = Arc::new(parking_lot::RwLock::new(
            HeadIndex::load_from_disk(&head_index_path)
                .unwrap_or_else(|e| {
                    warn!("Failed to load head index from disk: {}, starting with empty index", e);
                    HeadIndex::new()
                })
        ));

        // ... rest of initialization ...
    }
}
```

#### WatchDaemon Changes

```rust
impl WatchDaemon {
    pub fn new(api: Arc<ContextApi>, config: WatchConfig) -> Self {
        // Load head index on startup
        let head_index_path = HeadIndex::persistence_path(&config.workspace_root);
        {
            let mut head_index = api.head_index().write();
            if let Ok(loaded) = HeadIndex::load_from_disk(&head_index_path) {
                *head_index = loaded;
                info!("Loaded head index from disk: {} entries", head_index.heads.len());
            } else {
                info!("Starting with empty head index");
            }
        }

        Self { api, config, running: ... }
    }

    // Update put_frame calls to persist after head updates
    // This requires passing workspace_root through the API or storing it in ContextApi
}
```

### Persistence Strategy

#### Update Frequency

**Immediate Persistence (Initial Implementation)**
- Persist on every head update
- Simple and ensures consistency
- May have performance impact for high-frequency updates

**Batched Persistence (Future Optimization)**
- Batch updates and persist periodically (e.g., every N updates or every T seconds)
- Reduces I/O overhead
- Requires careful handling of process crashes

#### Atomic Writes

All persistence operations use atomic write pattern:

```rust
pub fn save_to_disk<P: AsRef<Path>>(&self, path: P) -> Result<(), StorageError> {
    let path = path.as_ref();
    let temp_path = path.with_extension("bin.tmp");

    // Serialize to temporary file
    let serialized = bincode::serialize(&self.heads)?;
    fs::write(&temp_path, &serialized)?;

    // Atomic rename
    fs::rename(&temp_path, path)?;

    Ok(())
}
```

### Error Handling

#### Corruption Recovery

If the head index file is corrupted or unreadable:

1. Log a warning
2. Start with empty head index
3. Optionally: Attempt to rebuild from frame storage (future enhancement)

#### Missing File

If the head index file doesn't exist:

1. Log info message
2. Start with empty head index
3. This is expected on first run

### Migration Strategy

#### Version Field

Include a version field in the persistence format to enable future migrations:

```rust
struct HeadIndexPersistence {
    version: u32,  // Current version: 1
    entries: Vec<HeadIndexEntry>,
}
```

#### Migration Path

- **Version 1**: Initial implementation
- **Future versions**: Can add compression, different serialization, etc.

## Configuration

### File Location

- **Default**: `.merkle/head_index.bin` (relative to workspace root)
- **Configurable**: Via `MERKLE_HEAD_INDEX_PATH` environment variable (future)

### Format Selection

- **Production**: Binary (bincode) - compact and fast
- **Debug**: JSON - human-readable for debugging
- **Selection**: Via `MERKLE_HEAD_INDEX_FORMAT=json` environment variable (future)

## Performance Considerations

### Write Performance

- **Immediate persistence**: ~1-5ms per head update (depends on disk speed)
- **Batched persistence**: Amortized cost over batch size
- **Impact**: Acceptable for typical frame creation rates (< 1000 frames/second)

### Read Performance

- **Load time**: ~10-50ms for typical workspace (100-1000 entries)
- **Memory**: ~100 bytes per entry (32 + 32 + string overhead)
- **Impact**: Negligible on startup

### Optimization Strategies

1. **Lazy Loading**: Load head index on first access (not on startup)
2. **Batched Writes**: Batch multiple head updates before persisting
3. **Async Writes**: Use background thread for persistence (adds complexity)
4. **Compression**: Compress persistence file for large indexes (future)

## Testing Strategy

### Unit Tests

1. **Persistence Operations**
   - Test `save_to_disk` and `load_from_disk`
   - Test atomic write behavior
   - Test corruption recovery
   - Test missing file handling

2. **Integration with API**
   - Test head updates persist correctly
   - Test cross-process sharing (watch daemon → CLI)
   - Test concurrent access (if applicable)

### Integration Tests

1. **End-to-End Workflow**
   - Create frames in watch daemon
   - Stop watch daemon
   - Query frames from CLI
   - Verify frames are found

2. **Recovery Scenarios**
   - Corrupt head index file
   - Delete head index file
   - Verify system continues to work

### Performance Tests

1. **Load Time**: Measure head index load time for various sizes
2. **Write Time**: Measure persistence overhead per update
3. **Throughput**: Measure frames/second with persistence enabled

## Security Considerations

### File Permissions

- Head index file should have same permissions as other `.merkle/` files
- Default: User read/write only (600 on Unix)

### Validation

- Validate deserialized data structure
- Check for reasonable entry counts (prevent DoS via large files)
- Verify frame IDs are valid (32 bytes)

## Observability

### Logging

- **INFO**: Head index loaded successfully (with entry count)
- **WARN**: Head index file missing or corrupted (falling back to empty)
- **DEBUG**: Head index persistence operations (save/load)
- **ERROR**: Persistence failures that prevent operation

### Metrics (Future)

- Head index size (entries)
- Persistence operation duration
- Persistence failures count

## Future Enhancements

### Phase 1: Basic Persistence (Current)

- File-based persistence with immediate writes
- Atomic updates
- Corruption recovery

### Phase 2: Performance Optimization

- Batched writes
- Async persistence
- Compression

### Phase 3: Advanced Features

- Rebuild from frame storage
- Incremental updates
- Distributed head index (for multi-workspace scenarios)

## Implementation Plan

### Phase 1: Core Implementation

1. Add `save_to_disk` and `load_from_disk` to `HeadIndex`
2. Update `ContextApi::put_frame` to persist after head updates
3. Update `CliContext::new` to load head index on startup
4. Update `WatchDaemon::new` to load head index on startup
5. Add unit tests for persistence operations

### Phase 2: Integration

1. Test cross-process sharing (watch daemon → CLI)
2. Add error handling and logging
3. Add integration tests
4. Performance testing and optimization

### Phase 3: Polish

1. Add configuration options
2. Add metrics and observability
3. Documentation updates
4. Migration tools (if needed)

## References

- [Watch Mode Specification](watch_mode_spec.md) - Context for watch daemon usage
- [Phase 2 Architecture](../bootstrap/phase2_architecture.md) - Head index design
- [Frame Storage](../bootstrap/phase1_components.md) - Frame storage structure
