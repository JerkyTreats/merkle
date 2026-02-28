# Workflow Foundation Spec

Date: 2026-02-28.

## Problem Statement

The project has context generation and watch driven updates, yet it lacks a generalized mechanism for policy guided actions over new context.

Goal: define a reusable workflow foundation that can:
- react to context and filesystem changes
- decide whether action should run
- execute actions safely with loop resistance
- retain deterministic provenance

## Goals

- Generalize beyond docs writing
- Keep adapters thin and domain contracts explicit
- Preserve deterministic behavior where possible
- Prevent self triggering infinite loops
- Support future merkle cluster style linkage across workflow definitions and runs

## Non Goals

- No immediate implementation details beyond design level contracts
- No provider specific orchestration logic in this spec
- No backward compatibility layer requirements

## Architectural Fit With Current Repository

Current relevant domains:
- `src/context` already owns generation planning queue and frame query
- `src/workspace/watch` already emits change events and updates tree plus node store
- `src/tree` already provides deterministic filesystem merkle identity

Proposed new domain:
- `src/workflow` as domain owner for workflow definition run policy action and audit

Suggested behavior named submodules for future implementation:
- `src/workflow/contract.rs`
- `src/workflow/trigger.rs`
- `src/workflow/policy.rs`
- `src/workflow/orchestration.rs`
- `src/workflow/actions.rs`
- `src/workflow/audit.rs`
- `src/workflow/store.rs`

This naming follows current domain first patterns and avoids layer first folders.

## Conceptual Model

### 1. Trigger
A normalized event produced from context update watch update or explicit user command.

### 2. Policy Decision
A pure decision pass that evaluates trigger plus current context and returns allow deny defer plus reason.

### 3. Workflow Definition
Immutable specification of trigger filter policy profile and ordered actions.

### 4. Workflow Run
One execution instance with run id state timestamps decision record action history and final result.

### 5. Action Invocation
A side effect request such as write file emit telemetry queue context generation or call provider.

### 6. Provenance
Durable lineage record of input node ids frame ids workflow definition hash run id action outputs and actor identity.

## Determinism and Idempotency Contract

Orchestration contract:
- deterministic over same normalized input event and same repository snapshot
- no direct side effects in policy stage

Action contract:
- idempotent per action key
- safe to retry

Run identity:
- deterministic run key from workflow definition hash plus trigger fingerprint plus target node id
- duplicate run key resolves to existing run outcome

## Loop Resistance Strategy

The main risk is write side effects that retrigger watch and context pipelines.

Required guards:
1. Source stamping
- every action write adds metadata marker for workflow run id and workflow definition hash

2. Self event suppression
- trigger normalizer drops events whose last writer marker equals current workflow identity

3. Action scope boundaries
- workflow definition must declare allowed write paths and denied watch paths
- generated output paths should default to ignore list membership

4. Idempotency key
- action key uses workflow definition hash plus run key plus action name plus content hash

5. Hop counter
- each trigger carries activation depth
- depth beyond configured max becomes deny decision

6. Cooldown window
- repeated identical triggers within short window become dedupe event instead of new run

7. Terminal run lock
- complete fail aborted states reject new action append

## Reflection Engine Design

Name recommendation:
- use `policy engine` as primary term
- use `reflection` as optional user facing alias

Responsibilities:
- evaluate trigger and context facts
- return deterministic decision payload
- never mutate external state

Decision output shape:
```text
policy_decision
- decision_id
- workflow_definition_id
- run_key
- outcome allow deny defer
- reasons list
- confidence optional
- evaluated_facts digest
```

Policy inputs:
- node metadata and path
- recent frame summary
- repository change kind
- workflow run history summary

## Merkle Cluster Design Direction

This feature can evolve from single tree to linked trees without changing core terms.

Definition tree:
- canonical workflow definition serialized and hashed as immutable object

Run tree:
- each run record stored as immutable node keyed by content hash
- child edges link action results in execution order

Cluster graph:
- edges connect repository root hash to workflow definition hash to workflow run hash to produced artifact hash
- cluster view is a merkle linked activation graph, not a mutable shared object

Benefits:
- replay and audit by hash lineage
- cheap dedupe via content identity
- natural bridge to multi workflow activation chains

## Example Workflow for Docs Writer

Workflow definition intent:
- trigger on context frame update for a target directory
- allow only if frame type matches docs writer frame type
- action writes markdown to declared output path under `design/generated`

Safety defaults:
- output path added to ignore patterns used by watch events
- action stamping enabled
- idempotency key check before write

Expected outcome:
- one run per unique context content change
- zero recursive activation for same run

## Rollout Plan

Phase A
- define workflow domain contracts and run state model
- define trigger envelope and policy decision shape

Phase B
- add one action adapter for file write
- add loop resistance guards and dedupe

Phase C
- add provenance persistence and run query API
- add linked hash references for cluster view

Phase D
- add policy profiles and richer decision facts
- add additional action adapters

## Risks and Mitigations

Risk: policy nondeterminism from wall clock or external calls
- mitigation: forbid external calls in policy stage

Risk: noisy triggers from file watch storms
- mitigation: batch normalize dedupe and cooldown

Risk: large action fanout
- mitigation: queue based bounded concurrency with per run caps

Risk: hard to debug decisions
- mitigation: structured decision record with reasons and evaluated fact digest

## Open Questions

- Should workflow definitions live in repository files or in context frame storage
- Should action outputs become context frames by default or by opt in
- Should run ids be human readable plus hash or hash only
- Should cross workspace activation be allowed in initial rollout
