# Generation Orchestration Synthesis Technical Specification

Date: 2026-03-01
Status: active

## Intent

Define one execution specification for generation orchestration cleanup.
This spec maps concrete code changes to measurable outcomes.

## Source Synthesis

This specification synthesizes:

- [Generation Orchestration Boundary Cleanup](README.md)
- [Generation Orchestration Code Path Findings](code_path_findings.md)

## Phase Boundary

Start condition:
- foundation cleanup scope is active
- baseline findings are accepted

End condition:
- queue lifecycle is isolated from generation content concerns
- orchestration units are split with explicit contracts
- parity gates pass for generation output and retry behavior

## Current Baseline Summary

Current seams in production flow:
- queue worker in `src/context/queue.rs` performs prompt collection, provider calls, metadata build, and frame write
- execution plan and level policy are already in `src/context/generation/run.rs` and `src/context/generation/executor.rs`
- integration coverage for queue behavior exists, while generation output parity coverage is limited

## Target Architecture

Ownership:
- `src/context/queue.rs` owns queue lifecycle only
- `src/context/generation/` owns orchestration units and unit contracts
- `src/metadata/` owns frame metadata construction and validation contracts
- `src/api.rs` remains the shared frame write boundary

## Change Set To Outcome Mapping

### C1 Extract prompt and context collection from queue worker

Code changes:
- move prompt and context helper behavior out of `src/context/queue.rs`
- create focused generation units under `src/context/generation/` for prompt template rendering and context collection
- queue worker calls generation unit contract instead of inline helper chain

Outcome:
- prompt and context behavior can change without queue lifecycle edits
- queue worker no longer contains prompt assembly logic

Verification:
- unit tests for generation prompt and context units
- parity test that generated prompt input shape matches pre split behavior

### C2 Extract provider execution from queue worker

Code changes:
- move provider client creation and completion call out of `FrameGenerationQueue::process_request`
- add dedicated provider execution unit under `src/context/generation/` with explicit request and response contract
- queue worker receives provider result from orchestration contract

Outcome:
- provider behavior is isolated from queue retry and scheduling behavior
- provider fallback behavior can be tested without queue internals

Verification:
- unit tests for provider executor success and failure paths
- parity test for retryable and non retryable provider errors

### C3 Extract frame metadata build from queue worker

Code changes:
- remove inline metadata map construction from `src/context/queue.rs`
- route metadata construction through explicit metadata contract boundary
- queue worker receives typed metadata output from generation unit before frame write

Outcome:
- metadata contract changes do not require queue code edits
- frame metadata write input is produced by a single contract path

Verification:
- tests that queue path and direct path use the same metadata contract surface
- tests that forbidden metadata payload values are rejected by boundary policy

### C4 Constrain queue worker to lifecycle concerns

Code changes:
- reduce `FrameGenerationQueue::process_request` to orchestration coordination, dedupe lifecycle, retry decisions, and telemetry
- keep enqueue, batch, ordering, rate limit, and worker lifecycle behavior in `src/context/queue.rs`

Outcome:
- queue module complexity and blast radius are reduced
- queue behavior remains stable for ordering and retries

Verification:
- existing queue integration tests continue to pass
- characterization tests confirm dedupe and retry semantics unchanged

### C5 Preserve generation plan and level policy seams

Code changes:
- keep `run_generate` ownership in `src/context/generation/run.rs`
- keep level policy ownership in `src/context/generation/executor.rs`
- update executor submit path to use new orchestration contract inputs where needed

Outcome:
- existing top level generate flow remains stable
- split work is localized under generation domain boundaries

Verification:
- executor tests for failure policy behavior remain green
- generate command integration tests cover end to end success and partial failure outputs

### C6 Add characterization and parity coverage before and after split

Code changes:
- add characterization tests for current generated frame content and metadata output shape
- capture baseline artifacts in `tests/fixtures/generation_parity/` before extraction
- add parity tests that compare post split behavior against baseline artifacts for successful generation and retry outcomes
- expand integration coverage beyond queue structure tests in `tests/integration/frame_queue.rs`

Outcome:
- split refactor is protected against behavior drift
- cleanup acceptance becomes deterministic

Verification:
- baseline characterization suite passes before extraction work
- baseline artifact capture completes for all parity scenarios
- same suite passes after extraction work
- parity suite passes for file and directory generation paths

## Generation Parity Gate Model

Gate set:
1. `P1` Baseline capture gate
2. `P2` Post split parity gate
3. `P3` Retry semantics parity gate

Scenario matrix:
- file node generation success
- directory node generation success
- provider retryable failure path
- provider non retryable failure path

Artifact contract:
- each scenario writes one baseline artifact under `tests/fixtures/generation_parity/`
- artifact shape includes prompt request payload provider request payload frame content and frame metadata keys
- artifacts exclude timestamps and other non deterministic fields

Pass criteria:
- `P1` baseline artifacts are generated and committed before extraction work
- `P2` post split run matches baseline artifacts with no contract drift
- `P3` retry attempt count backoff class and terminal error class match baseline behavior

## Milestone Gates

Behavior gates:
- queue ordering and retry behavior remain stable
- generation level policy behavior remains stable

Boundary gates:
- queue worker no longer performs inline provider call or inline prompt assembly
- metadata construction is delegated to one contract path
- frame writes still pass through shared `ContextApi::put_frame` boundary

Test gates:
- baseline capture and parity suites pass for generation output
- unit suites pass for extracted generation units

## Completion Criteria

Generation orchestration cleanup is complete when all criteria below are true:

1. queue worker lifecycle logic is isolated from prompt, provider, and metadata construction logic
2. extracted generation units own prompt collection, provider execution, and metadata build contracts
3. frame writes continue through shared validation boundary only
4. parity tests confirm no output drift for targeted generation scenarios
5. downstream metadata contract work can proceed without queue domain redesign
