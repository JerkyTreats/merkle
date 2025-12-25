# Phase 1C: NodeRecord Store — Explanation

## Overview

The **NodeRecord Store** is a fast lookup index that provides O(1) access to node metadata and relationships without requiring tree traversal. It acts as a bridge between the Filesystem Merkle Tree (Phase 1B) and the Context Frame system (Phase 1D+).

---

## Why Do We Need It?

### Problem: Tree Traversal is Slow

After Phase 1B, we can build a Merkle tree and compute root hashes. However, to access node information, we'd need to:

1. **Traverse the entire tree** to find a node by path
2. **Recompute relationships** every time we need parent/child info
3. **Scan all nodes** to find nodes matching criteria

This is inefficient for frequent lookups.

### Solution: Indexed Lookup

The NodeRecord Store provides:
- **O(1) lookup by NodeID**: Direct hash-based access
- **Pre-computed relationships**: Parent and children stored with each node
- **Metadata caching**: File size, content hash, path stored for quick access
- **Frame set pointers**: Links to associated context frames (for Phase 1D+)

---

## Relationship to Other Components

```
Filesystem Merkle Tree (Phase 1B)
    ↓ (generates NodeIDs and tree structure)
NodeRecord Store (Phase 1C) ← YOU ARE HERE
    ↓ (provides fast lookups)
Context Frames (Phase 1D)
    ↓ (uses NodeRecord for basis)
Frame Sets & Heads (Phase 1E)
    ↓ (indexed by NodeID from NodeRecord)
Context Views (Phase 1F)
```

**Key Insight**: The NodeRecord Store is populated from the Filesystem Merkle Tree, but provides the fast access patterns needed by all subsequent phases.

---

## What Gets Stored?

### NodeRecord Structure

Each `NodeRecord` contains:

```rust
pub struct NodeRecord {
    pub node_id: NodeID,              // [u8; 32] - Unique identifier
    pub path: PathBuf,                 // Filesystem path
    pub node_type: NodeType,           // File or Directory
    pub children: Vec<NodeID>,         // Child node IDs (for directories)
    pub parent: Option<NodeID>,        // Parent node ID (None for root)
    pub frame_set_root: Option<Hash>,  // Root hash of associated frame set (Phase 1E)
    pub metadata: HashMap<String, String>, // Additional metadata
}
```

### NodeType Enum

```rust
pub enum NodeType {
    File {
        size: u64,              // File size in bytes
        content_hash: [u8; 32]  // BLAKE3 hash of file content
    },
    Directory,                   // No additional data for directories
}
```

---

## Core Requirements

### 1. O(1) Lookup by NodeID

**Requirement**: Constant-time access to node records.

**Implementation**: Use a key-value store where:
- **Key**: `NodeID` (`[u8; 32]`)
- **Value**: Serialized `NodeRecord`

**Options**:
- `sled` (embedded database) - Recommended for Phase 1
- `rocksdb` (embedded database) - Alternative
- `HashMap` (in-memory) - For testing only

### 2. Stores Structural Relationships

**Requirement**: Each record must include:
- **Children**: List of child NodeIDs (for directories)
- **Parent**: Parent NodeID (for all nodes except root)

**Why**: Enables tree navigation without rebuilding the tree.

**Example**:
```rust
// Directory node
NodeRecord {
    node_id: [0x12, ...],
    path: "/workspace/src",
    node_type: Directory,
    children: [
        [0x34, ...],  // child1.rs
        [0x56, ...],  // child2.rs
    ],
    parent: Some([0x78, ...]),  // /workspace
    ...
}

// File node
NodeRecord {
    node_id: [0x34, ...],
    path: "/workspace/src/child1.rs",
    node_type: File { size: 1024, content_hash: [...] },
    children: [],  // Files have no children
    parent: Some([0x12, ...]),  // /workspace/src
    ...
}
```

### 3. Frame Set Pointers

**Requirement**: Each record includes `frame_set_root: Option<Hash>`.

**Purpose**: Points to the root hash of the Merkle set of context frames associated with this node (implemented in Phase 1E).

**Initial State**: `None` for all nodes (no frames yet).

**Future Use**: When frames are added in Phase 1D+, this will be updated to point to the frame set root.

### 4. No Embedded Frame Content

**Requirement**: Frames are stored separately; only references (hashes) are kept in NodeRecord.

**Why**:
- Frames can be large (blobs of context)
- Frames are append-only and may accumulate
- Separation of concerns: NodeRecord = index, Frame storage = content

---

## API Design

### Core Interface

```rust
pub trait NodeRecordStore {
    /// Get a node record by NodeID
    fn get(&self, node_id: &NodeID) -> Result<Option<NodeRecord>, StorageError>;

    /// Store or update a node record
    fn put(&self, record: &NodeRecord) -> Result<(), StorageError>;

    /// Batch operations (optional, for efficiency)
    fn put_batch(&self, records: &[NodeRecord]) -> Result<(), StorageError>;

    /// Check if a node exists
    fn contains(&self, node_id: &NodeID) -> Result<bool, StorageError>;
}
```

### Usage Example

```rust
// After building tree (Phase 1B)
let tree = builder.build()?;

// Populate NodeRecord Store
let store = SledNodeRecordStore::new("store.db")?;
for (node_id, node) in tree.nodes {
    let record = NodeRecord::from_merkle_node(node_id, node, &tree)?;
    store.put(&record)?;
}

// Later: Fast lookup
let node_id = [0x12, ...];
let record = store.get(&node_id)?;
if let Some(record) = record {
    println!("Path: {:?}", record.path);
    println!("Children: {:?}", record.children);
}
```

---

## Implementation Tasks

### Task 1: Schema Definition ✅ (Already Done)

**Status**: The `NodeRecord` and `NodeType` structures are already defined in `src/store/mod.rs`.

**What's There**:
- `NodeRecord` struct with all required fields
- `NodeType` enum (File/Directory)
- `NodeRecordStore` trait interface

### Task 2: Persistence Layer

**What to Implement**: Concrete implementation of `NodeRecordStore` using `sled`.

**File**: `src/store/persistence.rs`

**Requirements**:
- Use `sled::Db` for storage
- Serialize `NodeRecord` using `bincode`
- Implement `get()` and `put()` methods
- Handle errors gracefully

**Example Structure**:
```rust
pub struct SledNodeRecordStore {
    db: sled::Db,
}

impl SledNodeRecordStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }
}

impl NodeRecordStore for SledNodeRecordStore {
    fn get(&self, node_id: &NodeID) -> Result<Option<NodeRecord>, StorageError> {
        // 1. Look up key in sled
        // 2. Deserialize if found
        // 3. Return Option<NodeRecord>
    }

    fn put(&self, record: &NodeRecord) -> Result<(), StorageError> {
        // 1. Serialize NodeRecord
        // 2. Store in sled with node_id as key
    }
}
```

### Task 3: Fast Lookup API

**What to Implement**: Additional convenience methods for common operations.

**Examples**:
- `get_by_path()` - Lookup by path (requires path index or scan)
- `get_children()` - Get all children of a node
- `get_parent()` - Get parent node
- `get_descendants()` - Get all descendants (recursive)

**Note**: Some of these may require additional indexes or scans. Start with the core `get()` and `put()` methods.

---

## Integration with Phase 1B

### Populating the Store

After building a tree in Phase 1B, we need to convert `MerkleNode` objects into `NodeRecord` objects and store them.

**Conversion Logic**:
```rust
impl NodeRecord {
    pub fn from_merkle_node(
        node_id: NodeID,
        node: MerkleNode,
        tree: &Tree,  // For parent/child lookups
    ) -> Result<Self, StorageError> {
        match node {
            MerkleNode::File(file) => {
                Ok(NodeRecord {
                    node_id,
                    path: file.path,
                    node_type: NodeType::File {
                        size: file.size,
                        content_hash: file.content_hash,
                    },
                    children: vec![],  // Files have no children
                    parent: tree.find_parent(&node_id)?,
                    frame_set_root: None,
                    metadata: file.metadata,
                })
            }
            MerkleNode::Directory(dir) => {
                let children: Vec<NodeID> = dir.children
                    .iter()
                    .map(|(_, node_id)| *node_id)
                    .collect();

                Ok(NodeRecord {
                    node_id,
                    path: dir.path,
                    node_type: NodeType::Directory,
                    children,
                    parent: tree.find_parent(&node_id)?,
                    frame_set_root: None,
                    metadata: dir.metadata,
                })
            }
        }
    }
}
```

**Note**: This requires adding a `find_parent()` method to the `Tree` structure, or computing parent relationships during tree building.

---

## Testing Strategy

### Unit Tests

1. **Store/Retrieve**: Store a record, retrieve it, verify it matches
2. **Serialization**: Verify bincode serialization/deserialization works
3. **Missing Keys**: `get()` returns `None` for non-existent NodeIDs
4. **Updates**: `put()` with same NodeID updates existing record

### Integration Tests

1. **Tree to Store**: Build tree, populate store, verify all nodes present
2. **Parent/Child Links**: Verify parent and children relationships are correct
3. **Performance**: Measure lookup time (should be < 1ms per lookup)

### Property-Based Tests

1. **Round-trip**: Store → Retrieve → Verify identity
2. **Batch Operations**: Batch put matches individual puts

---

## Performance Targets

- **Lookup Time**: < 1ms per `get()` operation
- **Store Time**: < 5ms per `put()` operation
- **Batch Operations**: < 10ms per 100 records
- **Storage Size**: Reasonable (bincode is compact)

---

## Dependencies

All required dependencies are already in `Cargo.toml`:
- ✅ `sled = "0.34"` - Embedded database
- ✅ `serde = { version = "1.0", features = ["derive"] }` - Serialization
- ✅ `bincode = "1.3"` - Binary serialization

---

## Success Criteria

Phase 1C is complete when:

1. ✅ **Schema defined**: `NodeRecord` structure exists (already done)
2. ✅ **Persistence implemented**: `SledNodeRecordStore` with `get()` and `put()`
3. ✅ **Fast lookup works**: O(1) access by NodeID
4. ✅ **Tree integration**: Can populate store from Phase 1B tree
5. ✅ **Tests pass**: Unit, integration, and property tests

---

## Next Steps After Phase 1C

Once Phase 1C is complete, you'll have:
- Fast node lookups by NodeID
- Pre-computed parent/child relationships
- Foundation for Phase 1D (Context Frames) which will use NodeRecords as basis

---

## Key Takeaways

1. **NodeRecord Store is an index**: It doesn't store the tree structure itself, but provides fast access to node metadata.

2. **Populated from tree**: The store is populated from the Filesystem Merkle Tree built in Phase 1B.

3. **Enables fast access**: Without the store, every lookup would require tree traversal.

4. **Foundation for frames**: The `frame_set_root` field will be used in Phase 1E to link nodes to their context frames.

5. **Separation of concerns**: NodeRecord = metadata/index, Frame storage = content (separate in Phase 1D).

---

[← Back to Phases](phase1_phases.md) | [Next: Implementation Guide →](phase1_implementation.md)
