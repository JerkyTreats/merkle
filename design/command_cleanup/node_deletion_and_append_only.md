# Node Deletion and Append-Only

## Current state

- **Nodes are not deletable today.** The `NodeRecordStore` trait has only `get`, `put`, and `find_by_path` — no `delete`. Sled (and the path→node_id mapping in `put`) only add or overwrite; nothing removes node records or path keys.
- **Scan does not prune.** On `merkle scan` (or `--force`), we build a new tree from the filesystem and call `populate_store_from_tree`, which **puts** every node in the new tree. We never remove nodes that are no longer in the tree. So if the user deletes a directory on disk and rescans, the old node records (and their path mappings) remain in the store — stale entries accumulate.

## What “append-only” actually applies to

From the Phase 1 spec and README:

- **Frames**: Immutable once created; no deletion or modification of existing frames. History is preserved. (“Frame Deletion: No deletion of frames (append-only).”)
- **Nodes**: “Nodes are immutable (new state = new NodeID)” — meaning a given **NodeID** is an immutable identity for a content/structure snapshot; you don’t mutate a node in place, you get a new NodeID when the filesystem state changes.

The **node store** is an **index** over “what is in the current tree.” It is not the same as “node content identity.” Removing a node **record** from this index when a path is removed from the tree is **updating the index to match the current tree**, not mutating node content or deleting frames.

So:

- **Append-only** = frames are never deleted or mutated; node **identities** (NodeIDs) are immutable for a given content snapshot.
- **Node index deletion** = removing entries from the node store (and path→node_id map) when a subtree is no longer in the tree. That does **not** violate append-only.

## Allowing node deletion

**Conclusion: Deleting nodes from the node store (with cascade to descendants) is OK. Frame blob deletion on node delete is allowed as a policy choice to avoid orphan bloat.**

- **Frames**: Two policies. **Default (delete frames):** When a node is deleted, we delete the frame blobs that were heads for that node (avoids orphan bloat). **Option (--keep-frames):** We do not delete frame blobs; only indices are updated (strict append-only).
- **Nodes**: We are only removing **index entries** for nodes that are no longer in the current tree. We are not mutating any node’s content or identity.

### Cascade semantics

- **Delete node** = remove that node’s record (and its path→node_id mapping) from the node store, and **recursively remove all descendants** (so the whole subtree disappears from the index).
- **Head index**: Remove head index entries for every removed node ID.
- **Basis index**: Remove entries for frame IDs that were heads for deleted nodes. Required when deleting frame blobs.
- **Frame storage**: **Default:** Delete the frame blobs that were heads for the deleted node IDs. **Option (--keep-frames):** No deletion; frames remain in storage (orphaned with respect to current tree).

### When deletion happens

Two ways to get “nodes deleted”:

1. **Explicit delete** (future): e.g. `merkle node delete --path <path>` (or by node ID). Removes that node and all descendants from the node store (and path map), and clears their head index entries. Use case: user wants to drop a subtree from the index without rescanning (e.g. “stop tracking node_modules”).
2. **Rescan with replace** (alternative): On `merkle scan --force`, optionally **clear the node store** (and path mappings, and head index entries for nodes no longer in the new tree) then populate from the new tree. That effectively deletes any node not in the new tree. Requires a clear/replace or “replace store from tree” API.

Either way, cascade = “this node + all descendants” so the index never has a child without a parent.

## Summary

| Layer           | Append-only? | Deletion allowed? |
|----------------|--------------|--------------------|
| **Frames**     | Optional     | **Default:** Yes — delete frame blobs for deleted nodes (avoids orphan bloat). **Option:** --keep-frames preserves blobs (append-only). |
| **Node store** | No (index)   | Yes — remove node + descendants; update index to current tree |
| **Head index** | N/A          | Yes — remove entries for deleted node IDs so heads match the tree |

Deleting nodes (with cascade to descendants) is allowed. Frame blob deletion on node delete is the default to keep storage bounded; use --keep-frames to preserve history (strict append-only).
