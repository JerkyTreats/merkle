# Frame Integrity Boundary Cleanup

Date: 2026-03-01
Status: active

## Intent

Define the frame integrity cleanup boundary with one canonical doc set.
This folder contains the baseline findings and the synthesis execution spec.

## Related Docs

- [Code Path Findings](code_path_findings.md)
- [Synthesis Technical Specification](technical_spec.md)
- [Boundary Cleanup Foundation Spec](../README.md)

## Scope

- one shared validation boundary for all frame writes
- storage integrity checks that do not depend on free form metadata lookup
- deterministic typed failures for metadata key policy and size budget enforcement
- parity verification for direct write and queue write paths

## Out Of Scope

- workflow behavior expansion
- provider capability expansion
- prompt artifact rollout

## Entry Criteria

1. domain metadata separation cleanup is complete
2. frame integrity baseline findings are reviewed

## Exit Criteria

1. write boundary enforcement is centralized behind `ContextApi::put_frame`
2. storage integrity checks use structural identity inputs only
3. invalid metadata writes fail with typed deterministic errors
4. direct and queue paths have parity tests for failure and success cases
