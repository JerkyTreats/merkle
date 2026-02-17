# Dependency Gate Checklist

Date: 2026-02-17

## Purpose

This checklist is the shared Phase 1 gate contract used by every migration stream.

## Gate Table

| Gate ID | Gate Name | Owner | Evidence Path | Validation Command | Status |
|--------|-----------|-------|---------------|--------------------|--------|
| P1-G1 | Parse and help parity | CLI stream | `tests/phase1/parse_help_parity.rs` | `cargo test --test phase1_tests phase1::parse_help_parity` | Completed local |
| P1-G2 | Command output parity | CLI and domain streams | `tests/phase1/output_contracts.rs` | `cargo test --test phase1_tests phase1::output_contracts` | Completed local |
| P1-G3 | Summary and telemetry parity | Telemetry stream and CLI stream | `tests/phase1/summary_contracts.rs` | `cargo test --test phase1_tests phase1::summary_contracts` | Completed local |
| P1-G4 | Deterministic ordering parity | Context stream and workspace stream | `tests/phase1/deterministic_ordering.rs` | `cargo test --test phase1_tests phase1::deterministic_ordering` | Completed local |
| P1-G5 | Integration backstop parity | Shared stream owners | `tests/integration/progress_observability.rs`, `tests/integration/unified_status.rs`, and `tests/integration/context_cli.rs` | `cargo test --test integration_tests integration::progress_observability::command_families_emit_typed_summaries_with_command_summary`, `cargo test --test integration_tests integration::unified_status`, and `cargo test --test integration_tests integration::context_cli::test_context_generate_rejects_async_flag` | Completed local |

## Usage

- each migration stream references this checklist in its related docs section
- each stream updates status when a gate is complete
- no stream moves into extraction work before all Phase 1 gates are green
