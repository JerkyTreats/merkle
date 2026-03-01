# Generation Orchestration Boundary Cleanup

Date: 2026-03-01
Status: active

## Intent

Define the generation orchestration cleanup boundary with one canonical doc set.
This folder contains the boundary specification, code findings, and synthesis execution spec.

## Related Docs

- [Code Path Findings](code_path_findings.md)
- [Synthesis Technical Specification](technical_spec.md)
- [Boundary Cleanup Foundation Spec](../README.md)

## Problem

Current generation request handling mixes queue lifecycle, prompt and context collection, provider execution, metadata construction, and frame writes in one queue path.
This coupling increases refactor risk for metadata contracts and prompt artifact placement.

## Target Ownership Model

- `src/context/queue` owns dequeue, dedupe, retry policy, rate limiting, and queue telemetry
- `src/context/generation` owns orchestration units and domain contracts between units
- `src/metadata` owns metadata construction and validation rules
- `src/prompt_context` owns prompt and context artifact writes

## Required Boundary Shifts

1. extract prompt and context collection out of queue worker flow
2. extract provider execution out of queue worker flow
3. route metadata construction through explicit metadata contract unit
4. route frame writes through shared frame write validation boundary
5. preserve queue lifecycle behavior and retry behavior through parity tests

## Entry Criteria

1. foundation cleanup scope and order are accepted
2. generation orchestration code findings are reviewed

## Exit Criteria

1. queue worker no longer performs inline prompt assembly or provider calls
2. queue worker no longer performs inline frame metadata map construction
3. generation orchestration units expose explicit input and output contracts
4. parity tests cover generated frame output and retry behavior before and after split
