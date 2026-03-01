# Domain Metadata Separation Technical Specification

Date: 2026-03-01
Status: active

## Intent

Provide one synthesis execution spec for domain metadata separation cleanup.
This spec maps each code change set to a concrete outcome and verification gate.

## Source Synthesis

This specification synthesizes:

- [Domain Metadata Separation Cleanup](README.md)
- [Code Path Findings](code_path_findings.md)
- [Domain Metadata Separation Spec](separation_spec.md)

## Boundary

Start condition:
- metadata domains use shared free form map shapes with string key semantics in multiple seams

End condition:
- frame node and agent metadata contracts are separated by explicit domain types
- cross domain metadata transfer uses explicit adapters only
- frame metadata write behavior is enforced through one shared contract path
- characterization and parity coverage proves domain isolation

## Change To Outcome Map

### C0 Normalize store module layout before metadata type extraction

Code changes:
- replace `src/store/mod.rs` with `src/store.rs`
- keep store domain ownership and move child module declarations to `src/store.rs`
- preserve existing store behavior during file layout move

Outcome:
- cleanup path aligns with project module layout rule
- metadata separation work can proceed without adding new `mod.rs` dependencies

Verification:
- compile passes after layout move with no store behavior drift
- existing store integration tests remain green before domain metadata extraction

### C1 Introduce explicit metadata domain types

Code changes:
- add frame metadata contract types in `src/metadata/frame_types.rs`
- add node metadata contract type in `src/store/node_metadata.rs`
- add agent metadata contract type in `src/agent/profile/metadata_types.rs`
- update `src/context/frame.rs` `src/store.rs` `src/agent/identity.rs` and `src/agent/profile/config.rs` to use explicit types

Outcome:
- compile time ownership boundaries are explicit for frame node and agent metadata
- key contract drift across domains is reduced

Verification:
- compile checks confirm cross assignment between metadata domains fails without explicit adapters
- integration coverage still passes for valid frame writes node store writes and agent profile loads

### C2 Add explicit adapter for agent prompt contract

Code changes:
- add adapter contract in `src/agent/profile/prompt_contract.rs` that exports required prompt fields for generation
- update `src/agent/registry.rs` to resolve prompt contract values through agent domain logic
- update `src/context/queue.rs` and `src/context/generation/run.rs` to consume prompt contract instead of direct metadata key lookups

Outcome:
- context domain no longer reaches into agent private metadata key names
- prompt input requirements become explicit and typed

Verification:
- queue and generate flows pass with valid prompt contract inputs
- missing prompt contract fields fail with deterministic typed configuration errors

### C3 Centralize frame metadata construction and validation contract

Code changes:
- add shared frame metadata write contract in `src/metadata/frame_write_contract.rs`
- update `src/api.rs` so `ContextApi::put_frame` routes metadata checks through this shared contract
- update `src/context/queue.rs` to stop inline frame metadata key assembly and delegate to shared contract

Outcome:
- frame metadata key policy exists at one shared boundary
- direct write and queue write paths use identical metadata contract behavior

Verification:
- parity tests prove direct and queue writes emit identical success and failure behavior for matching inputs
- invalid frame metadata key use fails at shared boundary before storage write

### C4 Remove read path dependence on raw frame metadata map lookups

Code changes:
- update `src/context/query/view_policy.rs` and `src/context/query/composition.rs` to use typed frame metadata accessors
- update `src/cli/presentation/context.rs` to use explicit metadata projection policy from metadata domain contracts

Outcome:
- query and presentation surfaces stop depending on ad hoc string key lookups
- read behavior aligns with explicit metadata contract boundaries

Verification:
- context text and json output tests pass with expected metadata visibility behavior
- agent filter and ordering behavior remains stable

### C5 Complete node metadata type cutover without compatibility wrappers

Code changes:
- update `src/store/persistence.rs` to use node metadata contract serialization directly
- remove intermediate compatibility conversion layers from this cleanup scope

Outcome:
- node metadata behavior remains unaffected by frame metadata contract evolution
- ownership and persistence paths stay direct and explicit

Verification:
- characterization tests prove node record read and write behavior remains stable
- persisted node records used by current tests continue to load and store without behavior drift

### C6 Add isolation and misuse coverage

Code changes:
- add domain isolation characterization tests in `tests/integration/store_integration.rs` and `tests/integration/config_integration.rs`
- add frame write misuse tests in `tests/integration/context_api.rs` and `tests/integration/frame_queue.rs`
- add read surface parity checks in `tests/integration/context_cli.rs`

Outcome:
- cleanup has measurable evidence that domain boundaries are enforced
- regressions from metadata contract edits are detected early

Verification:
- frame policy edits do not regress node metadata behavior
- frame policy edits do not regress agent profile metadata behavior
- cross domain misuse cases fail deterministically

## File Level Execution Order

1. `src/store.rs`
2. `src/metadata/frame_types.rs`
3. `src/store/node_metadata.rs`
4. `src/agent/profile/metadata_types.rs`
5. `src/agent/profile/prompt_contract.rs`
6. `src/agent/registry.rs`
7. `src/context/frame.rs`
8. `src/api.rs`
9. `src/context/queue.rs`
10. `src/context/generation/run.rs`
11. `src/context/query/view_policy.rs`
12. `src/context/query/composition.rs`
13. `src/cli/presentation/context.rs`
14. `src/store/persistence.rs`
15. `tests/integration/context_api.rs`
16. `tests/integration/frame_queue.rs`
17. `tests/integration/store_integration.rs`
18. `tests/integration/config_integration.rs`
19. `tests/integration/context_cli.rs`

## Verification Matrix

Isolation gates:
- frame metadata contract changes do not alter node metadata write behavior
- frame metadata contract changes do not alter agent profile metadata behavior

Write boundary gates:
- direct and queue writes share one frame metadata contract boundary
- cross domain key misuse fails at write time with deterministic errors

Read boundary gates:
- query and presentation code paths use typed metadata accessors
- metadata output behavior is stable across text and json surfaces

## Completion Criteria

1. all change sets C0 through C6 are implemented
2. verification matrix gates pass
3. domain metadata separation cleanup is ready for downstream frame integrity and metadata contracts work
