# Generation orchestrator specification

## 1\. Purpose

The GenerationOrchestrator sits between Context Generate and the frame generation queue. It executes a prebuilt plan and reports results. The queue owns request processing, rate limiting, retries, provider calls, and frame storage.

**Separation of concerns:**

*   **Context Generate** resolves path, computes subtree and levels, checks descendants and heads, builds a GenerationPlan, and invokes the orchestrator.
*   **GenerationOrchestrator** executes the plan in order, submits items to the queue, waits for completion, enforces the failure policy, and aggregates results.
*   **FrameGenerationQueue** processes individual requests, handles ordering and retries, calls providers, and stores frames. It has no knowledge of trees or plan structure.

## 2\. Interface

The orchestrator receives a GenerationPlan and returns a GenerationResult. It blocks until plan execution completes.

**Input:**

*   plan\_id
*   ordered levels with GenerationItem entries
*   priority
*   failure\_policy
*   plan metadata for observability

Each `GenerationItem` is queue ready and includes `node_id`, `path`, `node_type`, `agent_id`, `provider_name`, and `frame_type`.

**Output:**

The orchestrator returns a GenerationResult structure.

### GenerationResult structure

**Identity:**

*   plan_id

**Per node outcomes:**

*   successes: map of node_id to frame_id
*   failures: map of node_id to error detail

**Per level summaries:**

*   level_summaries: ordered list of summaries, each with:
    *   level index
    *   generated count
    *   failed count
    *   total count

**Totals:**

*   total_generated
*   total_failed

## 3\. Single item plans

Single item plans are executed like any other plan. The plan contains one level with one item. The orchestrator submits the item to the queue and waits for completion. Domain checks and plan construction happen in Context Generate.

## 4\. Plan execution flow

**Process levels in order.** For each level in the plan:

*   Submit all items in the level to the queue concurrently.
*   Wait for all items in the level to complete.
*   Record each item outcome as success or failure.
*   Build a level summary from the completed items.
*   If any item fails, enforce the failure policy from the plan.

**Return results.** After all levels complete, return the GenerationResult with per node outcomes and per level summaries.


## 6\. Queue interaction

The orchestrator uses the queue enqueue and wait API. Each plan item is submitted as an individual request with queue ready execution fields. Items in the same level are submitted concurrently. Queue scheduling follows active plan first policy.

**Priority and scheduling:** Plan priority is provided by Context Generate and applied to each item. Queue executes one active orchestrator plan at a time. Other non deduplicated work waits for active plan completion.

**Deduplication:** Queue identity is `node_id + agent_id + frame_type`. `provider_name` is not part of identity. If an item is already pending or processing, queue coalesces request and orchestrator receives shared completion. If item already has a head by execution time and force is not set, queue returns existing head and no provider call runs.

**Shared completion:** The queue preserves completion channels across retries so the orchestrator always receives the final result.

**Concurrency:** Multiple orchestrator instances do not conflict. Each plan tracks only its own completions. Queue handles concurrent submissions through active plan first scheduling and deduplication.

## 7\. Error handling

*   **Plan validation failure:** Invalid plan structure or missing fields. Return error before any generation.
*   **Level failure:** One or more items in a level fail after queue retries exhaust. Enforce the failure policy and return error details if the plan stops.
*   **Queue full:** Queue rejects enqueue. Return error suggesting the user wait and retry.

**Failure policy return behavior:**

*   **StopOnLevelFailure:** return a GenerationResult with partial results up to and including the failed level, and surface an error that includes failed nodes.
*   **Continue:** return a GenerationResult with all successes and failures across all levels.
*   **FailImmediately:** return an error immediately after the first item failure and include any completed items so far in the GenerationResult if they exist.

## 8\. Event emission

The orchestrator receives an optional EventBus. If present, it emits execution events at each step. See [design/observability/observability_spec.md](../observability/observability_spec.md) for the full event catalog.

*   generation\_started when the plan begins
*   level\_started when a new level begins processing
*   node\_generation\_started, node\_generation\_completed, node\_generation\_failed for each item
*   level\_completed when all items in a level finish
*   generation\_completed or generation\_failed when the plan ends

Events are emitted regardless of success or failure. The event bus writes them to a session file, which can be consumed by `merkle tui` for real-time progress monitoring or reviewed after the fact.

If no event bus is provided (e.g. library use without CLI), the orchestrator operates identically but emits no events. Emit calls are cheap no-ops.

**Events and return values:** Events provide observability and progress. The GenerationResult is the programmatic return value. Events and return values are emitted independently and both are required for full fidelity.

## 9\. Implementation location

`src/generation/orchestrator.rs` new module. The orchestrator depends on:

*   `FrameGenerationQueue` for submitting requests
*   `EventBus` optional for emitting execution events

The CLI command (`handle_context_generate`) delegates to the orchestrator after resolving path, agent, provider, and flags.

## 10\. Required tests

### Unit tests with mock queue

**Plan execution:**

*   Single item plan completes and returns one success
*   Items in the same level are submitted concurrently
*   Levels execute in the order provided by the plan

**Failure policy:**

*   Stop on level failure stops after the first failed level
*   Continue executes all levels and reports mixed results
*   Fail immediately stops on the first failed item
*   Invalid plan returns validation error before first enqueue
*   Queue enqueue rejection returns queue full error with no silent drop

**Multi plan concurrency:**

*   Two plans execute concurrently with independent state
*   Completion of one plan does not affect level barriers in another

### Integration tests with real queue

**Multi process concurrency:**

*   Two recursive plans for the same folder run concurrently and both return the same frame id for shared nodes
*   Two processes submit overlapping plans with different priorities and active plan execution remains stable for currently active plan
*   One process submits a plan while another submits direct queue requests and both receive results without duplication
*   Plan B built before Plan A completes reuses Plan A completed heads for shared nodes and does not regenerate
*   Recursive plan for a higher branch submitted while a subbranch plan is active deduplicates shared pending items and reuses completed heads
*   High priority single file request for a file already in active plan queue deduplicates to active request and does not trigger duplicate provider call
*   Direct non overlapping single file request submitted during active plan remains queued until active plan completion

**Failure isolation:**

*   A failure in one process does not block another process from completing its plan

## 11\. Summary

The orchestrator is a thin execution layer. It does not call providers, build payloads, or store frames. It does not resolve paths, collect subtrees, or check heads. Its job is to execute a plan level by level, submit items to the queue, emit execution events, and report results.

## 12\. Related docs

*   [generation\_pipeline\_spec.md](generation_pipeline_spec.md) -- Architectural authority for the generation pipeline. Defines layer contracts, the GenerationPlan data structure, and separation of concerns.
*   [context\_generate\_by\_path\_spec.md](context_generate_by_path_spec.md) -- The CLI command spec that delegates to this orchestrator.
*   [llm\_payload\_spec.md](llm_payload_spec.md) -- What the queue sends to the LLM (file content or child context).
*   [design/observability/observability_spec.md](../observability/observability_spec.md) -- Event types, event bus, and session files that the orchestrator emits to.
*   [design/tui/tui\_spec.md](../tui/tui_spec.md) -- The TUI that consumes orchestrator events for real-time progress display.