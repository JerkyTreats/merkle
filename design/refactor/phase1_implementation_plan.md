# Phase 1 Implementation Plan

Date: 2026-02-17

## Objective

Lock current behavior contracts before extraction work by adding temporary parity gates, deterministic ordering checks, and one shared dependency checklist.

## Scope

This plan covers Phase 1 execution detail.

- temporary parity suite under `tests/phase1`
- parse and help contract gates
- command output parity gates in text and json
- telemetry summary contract gates
- deterministic ordering gates for status, list deleted, watch, and generation related flows
- one shared dependency checklist for all migration streams

## Out Of Scope

- domain extraction and ownership transfer
- route wave cutover
- behavior redesign

## Lifetime Policy

Phase 1 tests are temporary migration scaffolding.

- home during migration: `tests/phase1`
- removal or fold target: final migration cleanup phase
- durable tests can be moved into long term integration homes when migration risk drops

## Gate Groups

### Parse and help gate

- parse matrix for top level commands and nested command families
- invalid argument matrix for conflict and rejection paths
- help contract checks for top level and selected nested command scopes

### Output parity gate

- provider, agent, context, workspace, and unified status contract checks
- text contracts with focused snapshot tokens
- json contracts with field and key assertions

### Summary contract gate

- command family typed summary event coverage
- `command_summary` payload contract checks
- session event sequence and monotonic checks

### Deterministic ordering gate

- repeated status output checks
- repeated list deleted ordering checks
- watch contract stability checks
- repeated generation related output checks

### Dependency gate checklist

- one checklist artifact used by all migration plans
- each gate includes owner, evidence, command, and status

## Test Artifact Layout

- `tests/phase1_tests.rs`
- `tests/phase1/mod.rs`
- `tests/phase1/support.rs`
- `tests/phase1/parse_help_parity.rs`
- `tests/phase1/output_contracts.rs`
- `tests/phase1/summary_contracts.rs`
- `tests/phase1/deterministic_ordering.rs`
- `tests/fixtures/phase1/help/top_level.tokens`
- `tests/fixtures/phase1/help/context_generate.tokens`

## Gate Commands

```bash
cargo test --test phase1_tests
cargo test --test integration_tests integration::progress_observability::command_families_emit_typed_summaries_with_command_summary
cargo test --test integration_tests integration::unified_status
cargo test --test integration_tests integration::context_cli::test_context_generate_rejects_async_flag
```

## Acceptance Criteria

- Phase 1 test suite is green in local run
- deterministic checks are stable across repeated runs
- dependency checklist is published and linked by all migration streams
- no intentional command surface drift is introduced

## Progress

- Status: Completed local on 2026-02-17
- Gate command runs completed and green in local execution
- CI confirmation pending

## Deferred Integration Structure Design

Integration test reorganization is design only in this phase.

Target structure for later execution:

- `tests/integration/workspace/`
- `tests/integration/agent/`
- `tests/integration/provider/`
- `tests/integration/context/`
- `tests/integration/telemetry/`
- `tests/integration/cli/`
- `tests/integration/cross_domain/`
- `tests/integration/support/`

Rules for later move:

- each test has one canonical home
- cross domain contracts live in `cross_domain`
- shared fixtures and environment helpers live in `support`
- file moves happen in one dedicated follow up phase

## Risks and Mitigation

- risk: output drift in json field contracts
- mitigation: explicit key assertions and focused snapshots

- risk: flaky deterministic checks from volatile values
- mitigation: normalize volatile fields and compare stable projections

- risk: hidden non deterministic ordering in legacy paths
- mitigation: repeated run checks and fail fast gate policy
