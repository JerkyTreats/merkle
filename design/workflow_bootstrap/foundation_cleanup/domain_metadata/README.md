# Domain Metadata Separation Cleanup

Date: 2026-03-01
Status: active

## Intent

Define the domain metadata cleanup boundary with one canonical doc set.
This folder contains baseline findings and a synthesis execution spec.

## Related Docs

- [Code Path Findings](code_path_findings.md)
- [Synthesis Technical Specification](technical_spec.md)
- [Domain Metadata Separation Spec](separation_spec.md)
- [Boundary Cleanup Foundation Spec](../README.md)

## Scope

- isolate frame metadata node metadata and agent metadata by explicit domain contracts
- remove cross domain dependence on private metadata key names
- establish explicit adapters where metadata crosses domain boundaries
- add characterization and parity coverage for metadata isolation

## Out Of Scope

- prompt artifact storage rollout
- metadata key registry and budget enforcement rollout
- workflow feature behavior expansion

## Entry Criteria

1. domain metadata baseline findings are reviewed
2. frame integrity cleanup design is available for downstream alignment

## Exit Criteria

1. frame node and agent metadata contracts are separated by explicit types
2. context generation no longer reads private agent metadata keys directly
3. frame metadata handling is centralized behind one explicit write contract
4. characterization tests prove node and agent metadata behavior stability
