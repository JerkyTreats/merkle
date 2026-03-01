# Frame Integrity Boundary Code Findings

Date: 2026-03-01
Scope: foundation cleanup baseline findings for frame integrity boundary

## Intent

Capture current code path reality for frame integrity before cleanup implementation.
This document defines concrete gaps and the seams that must change.

## Code Seam Map

Write boundary seams:
- `src/api.rs`
- `src/agent/context_access/context_api.rs`
- `src/context/queue.rs`

Frame model and storage seams:
- `src/context/frame.rs`
- `src/context/frame/id.rs`
- `src/context/frame/storage.rs`

Error and verification seams:
- `src/error.rs`
- `tests/integration/context_api.rs`
- `tests/integration/frame_queue.rs`
- `src/context/frame/storage.rs` test module

## Baseline Findings

### F1 One shared write entry exists for production writes

Observed state:
- adapter write path delegates to `ContextApi::put_frame` at `src/agent/context_access/context_api.rs:56`
- queue generation path delegates to `ContextApi::put_frame` at `src/context/queue.rs:1248`

Implication:
- cleanup can introduce one validation service behind `put_frame` without route churn

### F2 Storage integrity hash depends on free form metadata lookup

Observed state:
- `FrameStorage::store` reads `agent_id` from `frame.metadata` at `src/context/frame/storage.rs:67`
- recomputation of frame id uses that metadata derived value at `src/context/frame/storage.rs:71`

Gap:
- storage integrity remains coupled to free form metadata structure

### F3 Frame write validation is partial and key policy is absent

Observed state:
- `ContextApi::put_frame` validates basis alignment at `src/api.rs:214`
- `ContextApi::put_frame` validates only `agent_id` metadata equality at `src/api.rs:237`
- no key allow list no forbidden key rejection and no metadata budget checks in this boundary

Gap:
- unknown and oversized metadata pass through the shared write boundary

### F4 Queue path writes raw prompt into frame metadata

Observed state:
- queue generation inserts `prompt` into metadata at `src/context/queue.rs:1237`

Gap:
- boundary currently allows payload values that should be blocked by frame integrity and metadata policy cleanup

### F5 Error model is string based for frame validation failures

Observed state:
- frame validation failures return `ApiError::InvalidFrame(String)` at `src/error.rs:54`
- storage missing `agent_id` currently returns `StorageError::InvalidPath(String)` at `src/context/frame/storage.rs:68`

Gap:
- deterministic typed failures for key policy and budget classes do not exist

### F6 Read time corruption detection is incomplete

Observed state:
- `FrameStorage::get` verifies requested id equals serialized `frame.frame_id` at `src/context/frame/storage.rs:161`
- read path does not recompute structural hash from basis agent identity frame type and content

Gap:
- on disk payload corruption that preserves `frame.frame_id` can pass read validation

### F7 Cleanup verification coverage is missing for required boundary behavior

Observed state:
- direct `put_frame` tests cover auth and basis failures at `tests/integration/context_api.rs:345`
- queue integration tests focus on scheduling and retries in `tests/integration/frame_queue.rs`
- storage tests cover mismatched `frame_id` path at `src/context/frame/storage.rs:339`

Gap:
- no characterization tests for queue and direct parity on invalid metadata
- no tests for unknown key rejection or metadata budget failures
- no test proving storage integrity checks are metadata structure independent

## Cleanup Targets For This Document

1. add one shared validation service behind `ContextApi::put_frame`
2. remove storage dependence on metadata map lookup for structural identity inputs
3. add deterministic typed errors for key policy failures and metadata size failures
4. block forbidden keys at write boundary for direct and queue paths
5. expand tests for direct write queue write and storage integrity independence
