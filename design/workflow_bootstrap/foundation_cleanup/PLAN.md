# Foundation Cleanup Implementation Plan

Date: 2026-03-01
Status: active
Scope: workflow bootstrap foundation cleanup

## Overview

This document defines the phased implementation plan for `workflow_bootstrap/foundation_cleanup`.
The plan follows the same execution style as the completed context plan and maps cleanup work to clear outcomes, dependencies, and verification gates.

Primary objective:
- stabilize metadata and generation boundaries before metadata contracts and turned workflow features

Foundation outcome:
- metadata ownership boundaries are explicit across frame node and agent domains
- frame write validation and policy enforcement run through one shared write boundary
- queue lifecycle is isolated from prompt provider and metadata construction concerns

## CLI Path Default Exception List

Project direction is path first targeting.
Current command surfaces that still include non default path behavior:

- `merkle context generate` accepts `--node` as an alternate selector
- `merkle context get` accepts `--node` as an alternate selector
- `merkle workspace delete` accepts `--node` as an alternate selector
- `merkle workspace restore` accepts `--node` as an alternate selector

This foundation cleanup plan does not expand non default path behavior.

## Development Phases

| Phase | Goal | Dependencies | Status |
|-------|------|--------------|--------|
| 1 | Domain metadata separation | None | active |
| 2 | Frame integrity boundary cleanup | Phase 1 | active |
| 3 | Generation orchestration boundary cleanup | Phase 1 and Phase 2 | active |
| 4 | Integrated parity and readiness gates | Phase 1 through Phase 3 | active |

---

### Phase 1 — Domain metadata separation

**Goal**: Separate frame node and agent metadata contracts with explicit types and adapters.

**Source docs**:
- [Domain Metadata Separation Cleanup](domain_metadata/README.md)
- [Domain Metadata Separation Technical Specification](domain_metadata/technical_spec.md)
- [Domain Metadata Separation Spec](domain_metadata/separation_spec.md)
- [Domain Metadata Code Path Findings](domain_metadata/code_path_findings.md)

| Task | Completion |
|------|------------|
| Normalize store module layout to remove `mod.rs` usage in the targeted seam. | Not started |
| Introduce explicit metadata domain types for frame node and agent domains. | Not started |
| Add explicit prompt contract adapter in agent profile domain. | Not started |
| Centralize frame metadata construction and validation contract usage through one write boundary. | Not started |
| Replace read path raw metadata map lookups with typed accessors and projection policy. | Not started |
| Complete node metadata cutover with direct serialization path and no compatibility wrapper track. | Not started |
| Add isolation misuse and adapter parity coverage across integration suites. | Not started |

**Exit criteria**:
- frame node and agent metadata boundaries are explicit and isolated
- context generation no longer depends on private agent metadata key names
- frame metadata write checks are centralized through one shared contract path
- characterization coverage proves metadata isolation and deterministic misuse failures

**Key files and seams**:
- `src/store.rs`
- `src/metadata/frame_types.rs`
- `src/store/node_metadata.rs`
- `src/agent/profile/metadata_types.rs`
- `src/agent/profile/prompt_contract.rs`
- `src/agent/registry.rs`
- `src/context/frame.rs`
- `src/api.rs`
- `src/context/queue.rs`
- `src/context/query/view_policy.rs`

---

### Phase 2 — Frame integrity boundary cleanup

**Goal**: Enforce deterministic typed frame metadata policy at one write boundary and decouple storage integrity from free form metadata lookup.

**Source docs**:
- [Frame Integrity Boundary Cleanup](frame_integrity/README.md)
- [Frame Integrity Boundary Technical Specification](frame_integrity/technical_spec.md)
- [Frame Integrity Code Path Findings](frame_integrity/code_path_findings.md)

| Task | Completion |
|------|------------|
| Extend shared frame write validator as the single frame write validation service. | Not started |
| Add structural frame identity fields for storage integrity verification. | Not started |
| Remove storage hash dependence on metadata map lookup. | Not started |
| Introduce typed metadata policy error variants for unknown forbidden and budget failures. | Not started |
| Enforce allow list and forbidden key policy at write boundary. | Not started |
| Enforce per key and total metadata size budgets at write boundary. | Not started |
| Add direct and queue parity tests for success and failure behavior. | Not started |

**Exit criteria**:
- all frame writes flow through one shared validator path
- storage hash and integrity checks rely on structural identity fields only
- metadata policy failures are typed deterministic and parity verified across direct and queue write paths

**Key files and seams**:
- `src/metadata/frame_write_contract.rs`
- `src/context/frame.rs`
- `src/context/frame/id.rs`
- `src/context/frame/storage.rs`
- `src/error.rs`
- `src/api.rs`
- `src/context/queue.rs`

---

### Phase 3 — Generation orchestration boundary cleanup

**Goal**: Split generation orchestration responsibilities from queue lifecycle while preserving generation and retry behavior.

**Source docs**:
- [Generation Orchestration Boundary Cleanup](generation_orchestration/README.md)
- [Generation Orchestration Synthesis Technical Specification](generation_orchestration/technical_spec.md)
- [Generation Orchestration Code Path Findings](generation_orchestration/code_path_findings.md)

| Task | Completion |
|------|------------|
| Extract prompt and context collection logic from queue worker into generation units. | Not started |
| Extract provider execution from queue worker into a dedicated generation unit contract. | Not started |
| Extract frame metadata construction from queue worker and route through metadata contract boundary. | Not started |
| Constrain queue worker to lifecycle dedupe retry ordering and telemetry concerns. | Not started |
| Preserve generate run ownership and level policy seams in generation domain. | Not started |
| Add characterization baseline capture and post split parity suites for generation output and retries. | Not started |

**Exit criteria**:
- queue worker no longer performs inline prompt assembly provider calls or metadata map construction
- generation contracts are explicit and domain scoped
- queue lifecycle behavior remains stable for ordering retry and dedupe
- parity suites confirm no contract drift for targeted scenarios

**Key files and seams**:
- `src/context/queue.rs`
- `src/context/generation/run.rs`
- `src/context/generation/executor.rs`
- `src/context/generation/` new unit modules
- `src/metadata/frame_write_contract.rs`
- `tests/fixtures/generation_parity/`

---

### Phase 4 — Integrated parity and readiness gates

**Goal**: Run cross phase verification gates and confirm cleanup readiness for downstream metadata contracts and turned workflow features.

| Task | Completion |
|------|------------|
| Run full integration suite for context queue store config and cli surfaces impacted by cleanup. | Not started |
| Run generation parity gates P1 P2 and P3 with committed baseline artifacts. | Not started |
| Verify frame write policy parity for direct and queue paths under identical invalid inputs. | Not started |
| Verify storage integrity checks remain deterministic after metadata policy hardening. | Not started |
| Verify no new non default path behavior appears in CLI docs or specs. | Not started |
| Publish final cleanup completion notes in foundation cleanup readme and workflow bootstrap roadmap. | Not started |

**Exit criteria**:
- all phase gates pass with no unresolved behavioral drift
- cleanup outputs are accepted by downstream metadata contracts and turn manager tracks

---

## Implementation Order Summary

1. Complete Phase 1 domain metadata separation
2. Complete Phase 2 frame integrity boundary cleanup
3. Complete Phase 3 generation orchestration split
4. Complete Phase 4 integrated readiness gates

## Verification Strategy

Isolation gates:
- frame metadata policy edits do not alter node metadata behavior
- frame metadata policy edits do not alter agent profile metadata behavior

Write boundary gates:
- direct and queue writes share one validation and policy path
- unknown forbidden and oversize metadata fail deterministically

Storage integrity gates:
- frame integrity checks use structural identity fields only
- metadata map key mutations do not bypass integrity checks

Generation parity gates:
- baseline artifacts exist and are committed before split validation
- post split artifacts match baseline for targeted success scenarios
- retry count backoff class and terminal error class match baseline

CLI direction gates:
- no new non default path command behavior is introduced
- exception list in this plan remains accurate as command surfaces evolve

## Success Criteria

Foundation cleanup is complete when:

1. domain metadata boundaries are explicit and isolated by contract types
2. frame integrity policy and validation are centralized typed and deterministic
3. generation orchestration is split by ownership with queue lifecycle isolation
4. parity and characterization coverage pass for metadata integrity and generation behavior
5. cleanup outputs are ready inputs for metadata contracts and turned docs workflow phases

## Related Documentation

- [Boundary Cleanup Foundation Spec](README.md)
- [Domain Metadata Separation Cleanup](domain_metadata/README.md)
- [Frame Integrity Boundary Cleanup](frame_integrity/README.md)
- [Generation Orchestration Boundary Cleanup](generation_orchestration/README.md)
- [Workflow Bootstrap Roadmap](../README.md)
- [Workflow Metadata Contracts Spec](../metadata_contracts/README.md)
- [Turn Manager Generalized Spec](../turn_manager/README.md)
- [Docs Writer Thread Turn Configuration Spec](../docs_writer/README.md)
