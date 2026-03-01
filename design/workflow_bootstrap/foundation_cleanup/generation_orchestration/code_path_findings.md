# Generation Orchestration Code Path Findings

Date: 2026-03-01
Scope: current code path findings for foundation cleanup generation orchestration split

## Intent

Capture current generation execution seams and isolate concrete cleanup work for orchestration split.
This document grounds split planning in observed code paths.

## Source Specs

1. [Boundary Cleanup Foundation Spec](../README.md)
2. [Generation Orchestration Boundary Cleanup](README.md)

## Current Execution Path Snapshot

- `src/context/generation/run.rs` resolves inputs, builds level plan, starts queue workers, and runs executor
- `src/context/generation/executor.rs` executes level order and failure policy, then submits items to queue
- `src/context/queue.rs` owns queue lifecycle plus full generation request processing

## Findings

### G1 Queue worker owns mixed orchestration concerns

Observed state:
- `FrameGenerationQueue::process_request` performs agent lookup, provider config lookup, node lookup, prompt validation, prompt generation, provider execution, frame metadata build, and frame write
- these operations are executed in one queue method chain

Primary seams:
- `src/context/queue.rs`

Impact:
- contract changes for prompt, metadata, or write behavior require queue edits
- retry and scheduling logic remain coupled to generation content logic

### G2 Prompt and context collection remain embedded in queue domain

Observed state:
- queue code collects directory child frame context
- queue code collects scoped node frame context
- queue code reads file bytes for prompt grounding and truncation
- queue code renders user prompt templates from agent metadata

Primary seams:
- `src/context/queue.rs`

Impact:
- prompt context behavior cannot evolve independently from queue lifecycle behavior
- focused orchestration ownership is not yet achieved

### G3 Frame metadata construction remains inline and includes raw prompt text

Observed state:
- queue request processing builds metadata map directly
- metadata includes `provider`, `model`, `provider_type`, and `prompt`
- frame write is executed directly from queue processing through `api.put_frame`

Primary seams:
- `src/context/queue.rs`
- `src/api.rs`

Impact:
- metadata contract rollout requires queue internal edits
- raw prompt material remains in frame metadata

### G4 Provider execution is not isolated behind a dedicated generation unit

Observed state:
- queue request processing creates provider client and calls completion directly
- queue path also handles provider model lookup fallback on failure
- generation executor module currently delegates queue submission and does not own provider execution

Primary seams:
- `src/context/queue.rs`
- `src/context/generation/executor.rs`

Impact:
- provider behavior and queue retry behavior are hard to test in isolation
- provider orchestration changes have broad queue blast radius

### G5 Queue module size and responsibility breadth indicate split work is incomplete

Observed state:
- `src/context/queue.rs` contains lifecycle, scheduling, dedupe, retry, prompt templating, context gathering, provider calls, metadata writes, and telemetry emission

Primary seams:
- `src/context/queue.rs`

Impact:
- local reasoning cost is high
- targeted refactor safety is lower without extraction

### G6 Characterization coverage for generation content parity is missing

Observed state:
- queue integration tests focus queue behavior and event emission
- queue integration tests note that full generation content parity is not covered
- generation executor tests use a mocked queue submitter and validate policy flow

Primary seams:
- `tests/integration/frame_queue.rs`
- `src/context/generation/executor.rs`

Impact:
- split work risks behavior drift in prompt assembly and frame metadata output
- parity verification needs dedicated tests before large extraction

## Existing Positive Baseline

### B1 Planning and run entrypoint already exist in generation domain

Observed state:
- generate command flow uses `src/context/generation/run.rs` for input resolution and plan build
- level policy execution is in `src/context/generation/executor.rs`

Primary seams:
- `src/context/generation/run.rs`
- `src/context/generation/executor.rs`

### B2 Queue lifecycle capabilities are already mature

Observed state:
- priority ordering, dedupe, retry, queue sizing, worker lifecycle, and telemetry emission are implemented and test covered

Primary seams:
- `src/context/queue.rs`
- `tests/integration/frame_queue.rs`

## Cleanup Focus From Findings

1. extract prompt and context collection into focused units under `src/context/generation/`
2. extract provider execution into a dedicated unit with explicit request and response contract
3. extract frame metadata build into explicit contract unit and remove inline map assembly from queue worker
4. keep queue worker focused on dequeue, retry, rate limiting, dedupe, and telemetry
5. add characterization and parity tests for generated frame content and metadata before and after extraction

## Exclusions

This document records baseline findings only.
It does not define metadata contracts rollout details or prompt artifact placement rollout details.
