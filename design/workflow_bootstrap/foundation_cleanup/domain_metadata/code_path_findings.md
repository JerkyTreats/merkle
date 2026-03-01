# Domain Metadata Code Path Findings

Date: 2026-03-01
Scope: current code baseline findings for domain metadata separation cleanup

## Intent

Capture concrete code findings for domain metadata separation before cleanup execution.
This document defines current seams and the remaining cleanup work.

## Source Spec

- [Domain Metadata Separation Spec](separation_spec.md)

## Baseline Findings

### D1 Distinct metadata containers exist yet type isolation is not enforced

Current state:
- frame metadata uses `HashMap<String, String>` in `src/context/frame.rs`
- node metadata uses `HashMap<String, String>` in `src/store/mod.rs`
- agent metadata uses `HashMap<String, String>` in `src/agent/identity.rs` and `src/agent/profile/config.rs`

Impact:
- domain boundaries exist by struct ownership only
- compile time separation of metadata key sets is not present

### D2 Frame metadata key semantics are distributed across multiple seams

Current state:
- `Frame::new` injects `agent_id` in `src/context/frame.rs`
- `ContextApi::put_frame` validates `agent_id` through frame metadata lookup in `src/api.rs`
- `FrameStorage::store` recomputes identity hash using `agent_id` from frame metadata in `src/context/frame/storage.rs`
- query filters and ordering use `agent_id` metadata lookup in `src/context/query/view_policy.rs` and `src/context/query/composition.rs`

Impact:
- frame metadata contract logic is not centralized in one boundary
- key semantics are duplicated across write storage and query paths

### D3 Agent metadata key semantics are hard coded and consumed across domains

Current state:
- `system_prompt` is materialized into agent metadata in `src/agent/registry.rs`
- `user_prompt_file` and `user_prompt_directory` are created in `src/init.rs` and `src/agent/commands.rs`
- queue and generate flows read these keys directly in `src/context/queue.rs` and `src/context/generation/run.rs`

Impact:
- context generation logic reaches into agent private metadata key details
- explicit adapter contract between agent metadata and context generation is missing

### D4 Queue generation writes free form frame metadata keys

Current state:
- queue generation writes `provider`, `model`, `provider_type`, and `prompt` into frame metadata in `src/context/queue.rs`

Impact:
- frame metadata policy is writer local and not governed by a shared contract
- raw prompt text is currently written into frame metadata

### D5 Node metadata is largely isolated from frame metadata semantics

Current state:
- node metadata is stored on `NodeRecord` and sourced from tree metadata conversion in `src/store/mod.rs`
- frame key checks do not run on node metadata code paths in current implementation

Impact:
- this is a useful baseline to preserve with characterization tests during cleanup

### D6 Read presentation exposes full frame metadata map when metadata output is enabled

Current state:
- text presentation performs ad hoc key filtering in `src/cli/presentation/context.rs`
- json presentation emits full `frame.metadata` in `src/cli/presentation/context.rs`

Impact:
- output visibility behavior is not enforced through explicit metadata domain policy
- read surface behavior can drift from future contract rules

## Test Coverage Findings

### T1 Domain isolation characterization coverage is missing

Needed coverage:
- frame metadata policy changes do not change node metadata write behavior
- frame metadata policy changes do not change agent profile metadata behavior

### T2 Cross domain adapter parity coverage is missing

Needed coverage:
- agent prompt contract adapter for file and directory generation inputs
- frame metadata builder output parity before and after separation cleanup

### T3 Negative coverage for metadata key misuse is missing

Needed coverage:
- reject non frame keys at shared frame write boundary
- reject leakage of agent prompt keys into frame metadata writes

## Cleanup Order For Domain Metadata Separation

1. introduce explicit metadata types for frame node and agent domains
2. add explicit adapters where metadata crosses domains
3. route frame metadata contract checks through one shared write boundary
4. isolate agent prompt configuration into typed agent domain contracts
5. add characterization and parity coverage for domain isolation

## Exit Signals

- frame metadata policy edits require changes in frame metadata domain only
- node metadata behavior remains stable under frame metadata policy edits
- agent metadata behavior remains stable under frame metadata policy edits
- cross domain metadata transfer paths use explicit adapters only
