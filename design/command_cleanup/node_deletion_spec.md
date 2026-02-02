# Node Deletion Specification

## Overview

This spec defines **node deletion**: removing a node and all its descendants from the node store (and related indices) so they are no longer part of the "current tree." Deletion is **cascade-by-default**: deleting a node removes that node and every descendant. **Frame blobs** for deleted nodes are **deleted by default** to avoid orphan bloat; an option preserves them for history.

**Rationale:** Users need to drop subtrees from the index (e.g. after adding `node_modules` to ignore, or to prune stale nodes without a full rescan). When the underlying file is removed and the node is removed, keeping frames can be **disruptive**: for large or long-lived trees with many iterations, orphan frame bloat is unnecessary. Allowing frame blob deletion on node delete keeps storage bounded. See [node_deletion_and_append_only.md](node_deletion_and_append_only.md) for append-only policy.

---

## Goals

1. **Explicit delete**: Remove a node and all descendants from the node store and head index by path or node ID.
2. **Cascade**: One delete operation removes the entire subtree; no orphan children.
3. **Consistency**: Head index and (optionally) basis index no longer reference deleted node IDs.
4. **Frame blobs**: **Delete by default** the frame blobs that were heads for the deleted nodes (and optionally all frames whose basis is a deleted node). Option **keep-frames** preserves blobs for history.

---

## Scope

### What is deleted (removed from indices and storage)

| Component | Action |
|-----------|--------|
| **Node store** | Remove node record (key = `node_id`) and path→node_id mapping (key = `path:<path>`) for the node and every descendant. |
| **Head index** | Remove every entry `(node_id, frame_type)` for the node and every descendant. |
| **Basis index** | Remove entries for frame IDs that were heads for deleted nodes (and optionally for any frame whose basis is a deleted node). |
| **Frame storage** | **Default:** Delete the frame blobs that are heads for the deleted node IDs (one blob per (node_id, frame_type) in head index before removal). **Optional:** Also delete any other frames whose basis is a deleted node (e.g. historical frames for that node). With `--keep-frames`, do **not** delete frame blobs; only indices are updated (legacy append-only behavior). |

### What is not deleted (when using default frame deletion)

| Component | Action |
|-----------|--------|
| **Frames for other nodes** | Only frames that reference a deleted node (as head or basis) are candidates for deletion; all other frame blobs remain. |
| **Node content** | No mutation of node content or NodeID semantics; we only remove index entries and the chosen frame blobs. |

---

## Cascade Semantics

- **Target**: One node, identified by path (positional) or `--node <node_id_hex>`. Path may be to a file or directory.
- **Scope**: That node plus **all descendants** — the entire branch. For a file, the branch is just that node. For a directory, the branch is the directory and every node in its subtree (children, grandchildren, down to leaves).
- **Order**: Walk to bottom-leaf: collect all descendant node IDs (e.g. depth-first from children), then delete every node in the branch including the specified one. Delete descendants first (e.g. depth-first post-order) so we never leave a child in the store whose parent was already removed; or collect the full set of node IDs and delete in any order (path and head index are keyed by node_id).
- **Root**: Deleting the workspace root node is equivalent to "clear the tree"; allowed.

---

## API Design

### 1. NodeRecordStore

Add to the trait and implementations:

```text
/// Remove a node record and its path→node_id mapping.
/// Does not remove descendants; caller is responsible for cascade.
fn delete(&self, node_id: &NodeID) -> Result<(), StorageError>;
```

**SledNodeRecordStore:**  
- `db.remove(node_id.as_slice())` for the node record.  
- Look up `record.path` (need to get the record first, or store path in a way we can recover). So: `get(node_id)` → if `Some(record)`, delete `record.node_id` key and `path:{}` key for `record.path`.  
- If we don't have the record (e.g. already deleted), we cannot remove the path key by node_id alone. So either: (a) require that callers pass path when deleting, or (b) maintain a reverse map node_id→path, or (c) accept that path keys for deleted nodes may remain until overwritten. Spec option (a): **delete(node_id)** — implement by get(node_id), then remove node_id key and path key; if get returns None, delete is a no-op for that node (path key may still exist; optional cleanup).

**Path key:** Use the same format as put: `path:{}` with path as string. So we must have the record to know the path; delete(node_id) = get(node_id), then remove both keys.

### 2. HeadIndex

Add:

```text
/// Remove all head entries for a node (all frame types).
pub fn remove_heads_for_node(&mut self, node_id: &NodeID) {
    self.heads.retain(|(nid, _), _| *nid != *node_id);
}
```

(Or iterate and remove; same effect.) No return value needed; persist after batch of deletions.

### 3. BasisIndex

Optional for v1:

```text
/// Remove all entries for frames that are heads for the given node.
/// Requires head index to know which frame IDs to remove.
pub fn remove_frames_for_node(&mut self, node_id: &NodeID, head_index: &HeadIndex) { ... }
```

Or: for each (node_id, frame_type) in head_index, get frame_id, then basis_index.remove_frame(&frame_id). Defer if not needed for correctness.

### 4. FrameStorage

Add (or use existing) capability to delete a frame blob by FrameID:

```text
/// Remove a frame blob from storage.
/// Idempotent: no error if frame_id is not present.
fn delete(&self, frame_id: &FrameID) -> Result<(), StorageError>;
```

(If frame storage is content-addressed and shared, ensure we only delete when no other reference exists; for head-only frames of deleted nodes, safe to delete.)

### 5. ContextApi (or equivalent)

Add:

```text
/// Delete a node and all descendants from the node store, head index, and (by default) frame storage.
/// With delete_frames == false, only indices are updated; frame blobs are preserved (append-only).
pub fn delete_node(
    &self,
    node_id: NodeID,
    cascade: bool,       // true = delete subtree; false = delete only this node (not recommended for directories)
    delete_frames: bool, // true = delete frame blobs for deleted nodes (default); false = keep blobs
) -> Result<DeleteNodeResult, ApiError>;
```

**DeleteNodeResult:** e.g. `{ nodes_removed: u64, head_entries_removed: u64, frames_deleted: u64 }`.

**Algorithm (cascade = true):**

1. Get node record; if missing, return error (e.g. NodeNotFound).
2. Collect all descendant node IDs: BFS/DFS from `record.children`, traversing via node_store.get and record.children. Build set `to_remove = { node_id } ∪ descendants`.
3. For each node_id in to_remove:
   - **Before** removing from head index: if `delete_frames`, collect all head frame IDs for this node_id from head index.
   - Head index: remove all heads for this node_id.
   - Basis index: remove entries for those frame IDs.
   - If `delete_frames`: for each collected frame_id, call frame_storage.delete(frame_id).
   - Node store: delete(node_id) (removes record + path key).
4. Persist head index and basis index.
5. Return counts (including frames_deleted).

**Concurrency:** Use existing node lock for the target node so two deletes don’t run for overlapping subtrees; or document that delete is not safe to run concurrently with scan/other structural changes.

---

## CLI

### Command

Placement under **workspace** (tree lifecycle):

```text
merkle workspace delete <path>
merkle workspace delete --node <node_id_hex>
```

**Primary form:** `merkle workspace delete <path>` — path is a **positional argument**: workspace-relative or absolute path to a **file or directory**. The command resolves the path to the corresponding node, walks the branch to the bottom (all descendants, leaf-first), and deletes every node in that branch including the one specified. For a file, the branch is just that node; for a directory, the branch is the directory and its entire subtree.

**Alternate form:** `merkle workspace delete --node <node_id_hex>` — when the node is identified by NodeID (hex) instead of path. Same cascade: delete that node and all descendants.

**Options:**

| Option | Description |
|--------|-------------|
| `<path>` | Positional: workspace-relative or absolute path to a file or directory. Mutually exclusive with `--node`. |
| `--node <id>` | Node ID (hex). Mutually exclusive with path. |
| `--delete-frames` | Default: true. Delete frame blobs for the removed nodes (avoids orphan bloat). |
| `--keep-frames` | Do not delete frame blobs; only remove node store and head/basis index entries (append-only preservation; may leave orphan frames). Mutually exclusive with `--delete-frames`. |
| `--dry-run` | Report how many nodes, head entries, and frames would be removed, without performing deletion. |
| `--no-ignore` | Do not add the deleted path to the workspace ignore list. By default, the path is appended to the ignore list so the next scan skips it. See [ignore_list_spec.md](ignore_list_spec.md). |

Cascade is always on: the target node and all descendants are removed. There is no option to delete only the single node and leave descendants (that would leave orphan children).

**Behavior:**

1. Resolve path → node_id via node_store.find_by_path (or, if `--node` given, parse node_id hex). Path must refer to a file or directory that exists in the current tree.
2. If node not found, error: "Node not found" / "Path not in tree."
3. Walk the branch: collect the target node and all descendants (bottom-leaf order or any order; implementation collects the set then deletes). This is the full branch: the specified node plus every node in its subtree.
4. If --dry-run: compute subtree size and (if delete_frames) frame count, output "Would remove N nodes, M head entries, F frames," exit.
5. Call api.delete_node(node_id, cascade: true, !keep_frames). Implementation deletes all nodes in the branch (target + descendants).
6. **Unless `--no-ignore`:** Append the deleted path to the workspace ignore list. Path is normalized to workspace-relative form; file is `$XDG_DATA_HOME/merkle/<workspace_path>/ignore_list`. Create parent dir and file if needed. See [ignore_list_spec.md](ignore_list_spec.md).
7. Output: "Removed N nodes, M head entries[, F frames]." Optionally: "Added \<path\> to ignore list."

---

## Edge Cases

| Case | Behavior |
|------|----------|
| **Node not in store** | Return error (NodeNotFound or PathNotInTree). |
| **Delete root** | Allowed; effectively "clear tree." |
| **Delete file** | Cascade removes only that node (no children). |
| **Delete directory** | Cascade removes directory and entire subtree. |
| **Path vs node_id** | Same outcome; path is resolved to node_id once. |
| **Concurrent scan** | Document: avoid running delete and scan concurrently; or use a single writer lock for "structure" operations. |
| **Head index persistence** | After batch removal, call head_index.save_to_disk (or equivalent) once. |
| **Path key missing** | If get(node_id) is None, we cannot remove path key; skip path key removal for that node. |

---

## Tests required

**Unit tests**

- NodeRecordStore delete: single node removed; path key for that node removed.
- HeadIndex remove_heads_for_node: all head entries for the node removed; no orphan entries.
- FrameStorage delete: blob removed by FrameID; idempotent when frame_id not present.
- Path-to-workspace-relative normalization for ignore list append.
- Ignore list append: create file and parent dir when missing; append one line when file exists.

**Integration tests (CLI)**

- Delete by path: file (single node removed); directory (cascade: node and all descendants removed); path not in tree / node not found (error, no mutation).
- Delete by `--node <id>`: same outcomes as path (file, directory, invalid/missing node ID).
- Root delete: tree cleared; no confirmation.
- Large subtree: delete proceeds; no confirmation.
- --dry-run: no store, head index, basis index, or frame storage changes; output "Would remove N nodes..." (or equivalent).
- --keep-frames: head index and node store updated; frame blobs not deleted; basis index updated as specified.
- Default (delete frames): head index, node store, basis index, and frame blobs updated; frame count in output.
- --no-ignore: deleted path not appended to ignore list; next scan can re-add path if tree is rescanned.
- Default (add to ignore): path appended to ignore_list; next scan excludes that path.

**Consistency / invariants**

- After delete: no head index entries for deleted node IDs; no node store records for deleted node IDs; path keys for deleted nodes removed (or documented if not); no orphan head references.
- Optional: basis index and frame storage consistency (no references to deleted frame blobs when delete_frames is true).

**Edge cases**

- Delete root; delete only node in tree; path key missing for a node (get returns None); concurrent scan (document or test single-writer behavior).

---

## Implementation Checklist

- [ ] **NodeRecordStore**: Add `delete(&self, node_id: &NodeID)`. In Sled: get(node_id), then remove node_id key and `path:{path}` key.
- [ ] **HeadIndex**: Add `remove_heads_for_node(&mut self, node_id: &NodeID)`; collect head frame IDs before removal when deleting frames.
- [ ] **FrameStorage**: Add `delete(&self, frame_id: &FrameID)` for blob removal.
- [ ] **BasisIndex**: Remove entries for deleted frame IDs (required when deleting frame blobs).
- [ ] **ContextApi**: Add `delete_node(node_id, cascade, delete_frames)` with subtree collection, head/basis removal, and frame blob deletion when delete_frames is true; persist indices after.
- [ ] **CLI**: Add `merkle workspace delete <path>` (positional path) and `merkle workspace delete --node <id>`; options --delete-frames (default), --keep-frames, --dry-run, --no-ignore; path resolution; cascade always on; unless --no-ignore, append path to workspace ignore list (see ignore_list_spec.md).
- [ ] **Tests**: See Tests required section above.
- [ ] **Docs**: Update design/command_cleanup/command_list if node delete is a top-level or workspace subcommand.

---

## Summary

| Item | Design |
|------|--------|
| **Scope** | Node store + path map + head index + basis index for the node and all descendants; **frame blobs** for those nodes when delete_frames is true. |
| **Cascade** | Always on: delete the target node and all descendants (the entire branch). No option to delete only the single node. |
| **Frames** | **Default:** Delete frame blobs that were heads for deleted nodes. **Option:** --keep-frames preserves blobs (append-only). |
| **API** | NodeRecordStore.delete(node_id); HeadIndex.remove_heads_for_node(node_id); FrameStorage.delete(frame_id); ContextApi.delete_node(node_id, cascade, delete_frames). |
| **CLI** | `merkle workspace delete <path>` (positional path to file or directory) or `merkle workspace delete --node <id>`; cascade always on; options --delete-frames (default), --keep-frames, --dry-run, --no-ignore (default: add path to ignore list). See [ignore_list_spec.md](ignore_list_spec.md). |
| **Order** | Collect subtree IDs; for each node collect head frame IDs (if delete_frames); remove head index and basis index entries; delete frame blobs; delete node store entries; persist indices. |
