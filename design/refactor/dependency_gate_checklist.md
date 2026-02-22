# Dependency Gate Checklist

Date: 2026-02-17

## Purpose

This checklist was the shared Phase 1 gate contract used by every migration stream. The temporary Phase 1 parity suite (`tests/phase1`) was removed in post-Phase 10 cleanup; rely on integration tests for ongoing gates.

## Gate Table

| Gate ID | Gate Name | Owner | Evidence Path | Validation Command | Status |
|--------|-----------|-------|---------------|--------------------|--------|
| P1-G5 | Integration backstop parity | Shared stream owners | `tests/integration/progress_observability.rs`, `tests/integration/unified_status.rs`, `tests/integration/context_cli.rs` | `cargo test --test integration_tests integration::progress_observability`; `cargo test --test integration_tests integration::unified_status`; `cargo test --test integration_tests integration::context_cli` | Completed local |

## Usage

- each migration stream references this checklist in its related docs section
- integration test suite is the ongoing gate for parity and regression

## Phase 2 Local Verification

- `cargo test provider::tests:: -- --nocapture`
- `cargo test --test integration_tests integration::provider_cli`
- `cargo test --test integration_tests integration::model_providers`
- `cargo test --test integration_tests integration::config_integration`
- `cargo test --test integration_tests integration::unified_status`
- `cargo test --test integration_tests`
