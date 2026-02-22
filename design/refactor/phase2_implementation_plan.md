# Phase 2 Implementation Plan

Date: 2026-02-17

## Objective

Move provider schema validation repository diagnostics command mutation flows and generation contracts into provider domain ownership.

## Scope

- provider profile schema and validation ownership under `src/provider/profile`
- provider repository contract and XDG adapter under `src/provider/repository`
- provider diagnostics and command services under `src/provider/diagnostics` and `src/provider/commands`
- provider client resolver and generation contracts under `src/provider/clients` and `src/provider/generation`
- CLI provider handlers delegated to provider services for validation connectivity and persistence policy

## Completed Work

- added `src/provider/profile/mod.rs`
- added `src/provider/profile/config.rs`
- added `src/provider/profile/validation.rs`
- added `src/provider/repository/mod.rs`
- added `src/provider/repository/contract.rs`
- added `src/provider/repository/xdg.rs`
- added `src/provider/clients/mod.rs`
- added `src/provider/clients/resolver.rs`
- added `src/provider/diagnostics/mod.rs`
- added `src/provider/commands/mod.rs`
- added `src/provider/generation/mod.rs`
- removed category named provider modules in `src/provider/application`, `src/provider/domain`, `src/provider/ports`, and `src/provider/infra`
- moved provider schema ownership from `src/config.rs` to provider domain with config re exports kept in `src/config.rs`
- routed provider registry persistence through repository port and XDG adapter
- removed static provider persistence entry points from `ProviderRegistry`
- delegated provider CLI validation connectivity type defaults and persistence policy to provider behavior services

## Verification

```bash
cargo test provider::tests:: -- --nocapture
cargo test --test integration_tests integration::provider_cli
cargo test --test integration_tests integration::model_providers
cargo test --test integration_tests integration::config_integration
cargo test --test integration_tests integration::unified_status
cargo test --test integration_tests
```

## Progress

- Status: Completed local on 2026-02-17
- CI: Pending
