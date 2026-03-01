# Frame Integrity Boundary Technical Specification

Date: 2026-03-01
Status: active

## Intent

Provide one synthesis execution spec for frame integrity cleanup.
This spec maps each code change set to a concrete outcome and verification gate.

## Source Synthesis

This specification synthesizes:

- [Frame Integrity Boundary Cleanup](README.md)
- [Code Path Findings](code_path_findings.md)

## Boundary

Start condition:
- frame integrity cleanup work has not yet modified write or storage seams

End condition:
- all frame writes pass through one shared validator
- storage hash checks are metadata structure independent
- metadata key policy and size budget failures are typed and deterministic

## Change To Outcome Map

### C1 Add shared frame write validator

Code changes:
- extend `src/metadata/frame_write_contract.rs` as the single validator service
- update `src/context/frame.rs` to expose structural identity fields needed by validator and storage checks
- update `src/api.rs` so `ContextApi::put_frame` delegates frame checks to shared metadata validator

Outcome:
- direct adapter writes and queue writes enforce identical validation rules
- basis alignment and agent identity checks run in one place

Verification:
- characterization tests for valid direct writes still pass
- new tests prove identical error class for direct and queue invalid writes

### C2 Remove storage dependence on metadata lookup

Code changes:
- add structural identity field on `Frame` for agent identity in `src/context/frame.rs`
- update `src/context/frame/id.rs` and `src/context/frame/storage.rs` to recompute ids from structural fields only
- keep metadata map as policy payload only

Outcome:
- storage integrity hash verification no longer depends on free form key lookup
- storage path safety checks and hash checks remain deterministic

Verification:
- new storage tests cover hash verification with metadata key mutations that do not alter structural identity fields
- corruption tests still fail with hash mismatch errors

### C3 Add typed metadata policy failures

Code changes:
- extend `src/error.rs` with typed variants for unknown key forbidden key and budget overflow
- update shared metadata validator service to emit typed variants instead of string only frame failures
- update call sites in `src/api.rs` and `src/context/queue.rs` to preserve typed errors

Outcome:
- invalid metadata writes fail with stable machine actionable error classes
- callers can branch on error type without string parsing

Verification:
- direct write tests assert typed error variants
- queue write tests assert matching typed variants

### C4 Enforce metadata key allow list and forbidden keys

Code changes:
- define allowed key policy for frame writes in shared metadata validator service
- reject forbidden payload keys such as raw prompt at write boundary
- remove inline metadata trust in queue write assembly in `src/context/queue.rs`

Outcome:
- boundary blocks unsafe metadata payload writes before storage
- queue write path and direct write path respect the same policy

Verification:
- tests fail on unknown key writes
- tests fail on forbidden key writes
- tests pass for allowed key writes within policy

### C5 Enforce metadata size budgets at write boundary

Code changes:
- add per key and total metadata byte accounting in shared metadata validator service
- route overflow to typed budget error variants in `src/error.rs`

Outcome:
- oversized metadata cannot reach storage
- budget behavior is deterministic across direct and queue writes

Verification:
- tests cover per key overflow
- tests cover total metadata overflow
- tests cover exact limit success boundary

## File Level Execution Order

1. `src/metadata/frame_write_contract.rs`
2. `src/context/frame.rs`
3. `src/error.rs`
4. `src/api.rs`
5. `src/context/frame/id.rs`
6. `src/context/frame/storage.rs`
7. `src/context/queue.rs`
8. `tests/integration/context_api.rs`
9. `tests/integration/frame_queue.rs`
10. `src/context/frame/storage.rs` test module

## Verification Matrix

Write boundary gates:
- direct write and queue write return same typed failures for same invalid metadata
- valid writes retain existing head update behavior

Storage integrity gates:
- hash recomputation uses structural fields only
- metadata map lookup is not required for integrity checks

Policy gates:
- unknown and forbidden keys fail deterministically
- budget overflows fail deterministically

## Completion Criteria

1. all change sets C1 through C5 are implemented
2. verification matrix gates pass in integration and unit coverage
3. frame integrity boundary is ready for metadata contracts phase execution
