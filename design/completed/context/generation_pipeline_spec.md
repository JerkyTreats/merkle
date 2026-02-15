# Generation pipeline architecture

## Direction of travel

The generation pipeline has three layers. Each layer depends on the one below it:

```
Context Generate --submits plan--> Orchestrator --submits items--> Queue --calls--> Provider
```

**Forward flow:** To generate, you must orchestrate. To orchestrate, you must queue. Context Generate builds a plan encoding what to generate and in what order. The Orchestrator executes that plan by feeding items to the Queue level by level. The Queue processes individual items by calling providers, handling retries, and storing frames.

**Return flow up:** The queue returns a per item result to the orchestrator for each submitted request. The orchestrator aggregates these results into a GenerationResult with per node outcomes, per level summaries, and totals. Context Generate consumes the GenerationResult to produce CLI output, error handling, and exit status. The return flow is required even when events are enabled, since events are for observability while return values are for programmatic results.

**Backward requirements:** Each layer's contract defines what the layer above can ask for. The Queue's API defines what the Orchestrator can submit. The Orchestrator's API defines what Context Generate must prepare. Requirements propagate upward: if the Queue cannot deduplicate, the Orchestrator must work around it; if the Orchestrator cannot handle failure policies, Context Generate must encode simpler plans.

This document is the architectural authority for the generation pipeline. The individual specs -- [context_generate_by_path_spec.md](context_generate_by_path_spec.md), [generation_orchestrator_spec.md](generation_orchestrator_spec.md), and the queue implementation in [src/frame/queue.rs](../../src/frame/queue.rs) -- must conform to the contracts defined here. This document does not replace those specs; it defines the boundaries they operate within.

## Layer contracts

Each layer is a self-contained domain. It owns its logic completely, exposes a contract upward, and depends only on the contract of the layer below.

### Queue -- lowest layer

**Domain:** Individual request processing.

**Owns:**
- Active plan first scheduling for orchestrated generation
- Rate limiting per agent
- Retry logic with backoff for transient failures
- Provider calls -- the queue is the only path to LLM providers
- Frame creation and storage after successful provider response
- Completion notification to callers via oneshot channels
- Request deduplication -- rejecting or coalescing duplicate requests for the same `node_id + agent_id + frame_type` key
- Head short circuit -- if a head already exists for request key and force is not set, return existing head without provider call

**Contract it offers upward:**
- `enqueue` -- submit a request, get a request ID, returns immediately
- `enqueue_and_wait` -- submit a request, block until completion, get the resulting frame ID or error
- `enqueue_batch` -- submit multiple requests at once
- `stats` -- get aggregate queue statistics
- `wait_for_completion` -- block until the queue drains

**Knows nothing about:** Trees, subtrees, and level construction logic. Queue executes requests by scheduling policy and request identity rules.

**Key invariant:** Two requests for the same `node_id + agent_id + frame_type` key must not both result in provider calls. `provider_name` is not part of this key. If a matching request is pending or processing, queue coalesces the new request with existing work. If matching work already completed and head exists, queue returns existing head unless force is set.

### Orchestrator -- middle layer

**Domain:** Plan execution and state management.

**Owns:**
- Accepting generation plans (the data structure defined in section 3)
- Executing plans level by level: submit all items in a level to the queue, wait for the level to complete, then advance to the next level
- Tracking per-plan state: which levels are done, which items succeeded or failed, aggregate results
- Enforcing the plan's failure policy (stop on level failure, continue despite failures, fail immediately on first error)
- Managing multiple concurrent plans, each with independent state and lifecycle
- Emitting execution-level events (plan started, level started, level completed, plan completed/failed)

**Contract it offers upward:**
- Accept a GenerationPlan, execute it, return a GenerationResult listing successes and failures per node, per level

**Knows nothing about:** Tree traversal policy, descendant readiness policy, and head existence policy. The orchestrator receives ordered batches and executes them faithfully. It knows how to submit and track queue work from queue ready item fields. It does not decide why a plan has a given order.

**Dependencies:** Queue (for submitting items), EventBus (optional, for emitting events). The orchestrator does NOT depend on NodeRecordStore, HeadIndex, or any tree/store layer.

### Context Generate -- highest layer

**Domain:** Generation domain logic and plan construction.

**Owns:**
- Path resolution: canonicalize user input, resolve to a node via NodeRecordStore
- Subtree collection: given a node_id, recursively collect all descendants, guard against cycles
- Level grouping: group subtree nodes by depth and order levels deepest first so execution runs leaf to trunk
- Descendant checks in single node directory mode: verify descendants have heads for target frame_type unless `--force` is set
- Head-existence filtering: for each node in the subtree, check whether a head already exists for the frame_type, and exclude nodes that do not need regeneration (unless --force)
- Building the GenerationPlan data structure from the above analysis
- Emitting domain-level events (nodes skipped due to existing heads, descendant check results)
- CLI surface: flags, argument parsing, error messages, output formatting

**Contract it requires from Orchestrator:** "Accept my plan, execute it in level order, tell me per-node and per-level results, respect the failure policy I specified."

**Knows nothing about:** Queue internals, rate limiting, retries, provider calls, how items are processed. Context Generate hands off a plan and waits for results.

## The generation plan data structure

The GenerationPlan is the contract between Context Generate and the Orchestrator. It encodes everything the orchestrator needs to execute a generation run without any domain knowledge.

**Plan identity:**
- plan_id -- unique identifier for this plan (e.g. UUID or session-scoped counter)
- source -- string describing what created the plan (e.g. "context generate ./src")
- session_id -- the observability session this plan belongs to, if any

**Ordered levels:**
- A sequence of levels, where each level is a set of items that can be processed concurrently
- Levels are ordered: the orchestrator processes level 0 first, then level 1, and so on
- Context Generate is responsible for ordering levels correctly (e.g. deepest-first for tree generation)
- Represented as a list of lists of GenerationItem

**GenerationItem:**
- node_id -- the node to generate a frame for
- path -- canonical path for event reporting and payload context
- node_type -- file or directory
- agent_id -- the agent to use
- provider_name -- the provider to call
- frame_type -- the frame type to generate

**Plan-level configuration:**
- priority -- the Priority to use when submitting items to the queue (e.g. Urgent for user-initiated, Normal for background)
- failure_policy -- what to do when an item or level fails:
  - StopOnLevelFailure: if any item in a level fails permanently, do not proceed to the next level. This is the correct policy for tree generation where parents depend on children.
  - Continue: attempt all levels regardless of failures. Useful for best-effort batch generation.
  - FailImmediately: stop the entire plan on the first item failure.

**Plan metadata for observability:**
- target_path -- the path the user specified (for event emission and reporting)
- total_nodes -- total items across all levels
- total_levels -- number of levels

The orchestrator treats this structure as ordered execution input. It does not reorder items. Context Generate is responsible for constructing correct levels and queue ready item fields.

## Requirements flowing backward

### Context Generate requires from Orchestrator

- Accept a GenerationPlan and execute it
- Process levels in the order given (level 0 before level 1 before level 2, etc.)
- Within each level, submit all items concurrently to the queue
- Wait for all items in a level to complete (success or permanent failure) before advancing
- Respect the failure_policy: stop, continue, or fail immediately as specified
- Return a GenerationResult with per-node outcomes (frame_id on success, error on failure) and per-level summaries
- Emit execution events to the EventBus if one is provided
- Support multiple submitted plans while preserving independent state per plan

### Orchestrator requires from Queue

- `enqueue_and_wait` must reliably return final result, success or permanent failure, for every submitted item. Completion channels remain preserved across retries so caller receives final outcome.
- Deduplication: if orchestrator submits an item already pending or processing from another source, queue does not make duplicate provider call. Queue returns completion channel that resolves when existing request completes.
- Completed overlap reuse: if orchestrator submits an item whose head already exists by execution time and force is not set, queue returns existing head immediately and does not regenerate.
- Active plan scheduling: one orchestrator plan is active at a time for queue execution. Active plan items run before non active plan items. New requests from other sources remain queued unless they deduplicate to active work.
- Queue-full rejection: if the queue cannot accept an item, it returns an error immediately so the orchestrator can report it.

### Queue requires from Provider

- A `complete()` method that takes messages and completion options, returns a response or error
- Errors must be classifiable as retryable (rate limit, transient network failure) or permanent (config error, model not found, missing prompts)
- This contract already exists and is unchanged

## Multi-source concurrency

Multiple sources can submit to the pipeline concurrently. The architecture must handle all combinations without data corruption, duplicate work, or deadlocks.

### Scenarios

**Two CLI invocations generating overlapping subtrees:**
User runs `merkle context generate ./src` in one terminal and `merkle context generate ./src/utils` in another. Both create orchestrator plans. Both submit items to the same queue. Some nodes (everything under `./src/utils`) appear in both plans.

**Expected behavior:** Queue-level deduplication ensures each node is generated at most once while pending or processing. When one plan completes a shared node before the other submits it, queue returns existing head for second plan without provider call. Each plan tracks its own level barriers independently -- plan A level completion does not advance plan B level completion.

**Higher branch after partial subbranch completion:**
Plan A runs recursive generation for subbranch and reaches partial completion. Plan B starts recursive generation for a higher branch that includes all of A targets.

**Expected behavior:** Plan B can be built before A completes and still execute safely. For overlapping nodes, queue deduplicates pending work and short circuits nodes that already have newly written heads. Already completed A work is treated as valid progress for B. No duplicate provider calls are made for shared key.

**CLI generation plus watch mode:**
User runs `merkle context generate ./src` while watch mode is running. Watch mode detects a file change in `./src/foo/bar.rs` and enqueues a High-priority generation. The orchestrator also includes `bar.rs` in its plan at Urgent priority.

**Expected behavior:** If watch mode enqueues first and item key matches active plan key, orchestrator attaches by deduplication. If active plan enqueues first and watch submits same key, watch attaches by deduplication. For non matching keys while a plan is active, watch request remains queued until active plan completes.

**CLI generation plus scanner:**
Scanner enqueues Low-priority items for initial coverage. User runs a generate command, creating Urgent-priority items for a subtree. Some overlap.

**Expected behavior:** Overlapping items are deduplicated to shared work. Non overlapping scanner items remain queued while active plan executes. Scanner processing resumes after active plan completion.

**Multiple orchestrator plans in flight:**
Two plans submitted simultaneously, non-overlapping subtrees. Each has its own levels, its own failure policy, its own result tracking.

**Expected behavior:** Plans keep independent state and level barriers. Queue executes one active plan at a time. Non active plans wait for active plan completion unless requests deduplicate to already active work.

### Design principles

- **Deduplication lives in the queue.** The queue is the single authority on "is this generation already happening?" Higher layers do not need to coordinate with each other.
- **Deduplication key ignores provider.** Identity is `node_id + agent_id + frame_type`. Provider selection does not create a separate work identity.
- **Completed work is reusable.** New head frames satisfy later overlapping requests for same key unless force is set.
- **Level barriers are per-plan.** The orchestrator tracks each plan's levels independently. Plans do not block each other.
- **Active plan has execution priority.** Queue executes one active orchestrator plan at a time. Other work waits unless it deduplicates to active requests.
- **Non-orchestrated requests are first-class.** Watch mode and scanner submit directly to the queue without an orchestrator. The queue handles them identically to orchestrated requests. Deduplication works regardless of the submission source.



## Related docs

- [context_generate_by_path_spec.md](context_generate_by_path_spec.md) -- CLI command spec, domain logic, plan construction
- [generation_orchestrator_spec.md](generation_orchestrator_spec.md) -- Plan execution engine
- [design/observability/observability_spec.md](../observability/observability_spec.md) -- Event types, EventBus, session files
- [llm_payload_spec.md](llm_payload_spec.md) -- What the queue sends to the LLM (file content or child context)
- [src/frame/queue.rs](../../src/frame/queue.rs) -- Current queue implementation
- [frame_generation_queue_spec.md](../completed/workflow/frame_generation_queue_spec.md) -- Original queue design spec
