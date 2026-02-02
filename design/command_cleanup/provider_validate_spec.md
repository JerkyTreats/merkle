# Provider Validate Command Specification

## Overview

This document specifies `merkle provider validate`, the canonical command for validating a single provider's configuration and optionally testing connectivity and model availability. It lives in command_cleanup; for provider form and registry behavior, see design/context/provider/provider_cli_spec.md as reference only. Top-level `validate-providers` is removed; use this command instead.

## Command structure

**Syntax**

```
merkle provider validate <provider_name> [--test-connectivity] [--check-model] [--verbose]
```

**Options**

- `--test-connectivity`: Run a lightweight connectivity check against the provider API.
- `--check-model`: Verify the configured model is available from the provider.
- `--verbose`: Show detailed validation results (each check listed).

## Purpose

Validate provider configuration (required fields, endpoint format) and optionally test connectivity and model availability. Scripts and users run this to confirm a provider is correctly configured before context generation.

## Behavior

1. Load provider configuration from registry (XDG providers directory).
2. Validate required fields (name, type, model, endpoint format).
3. Optionally test API connectivity when `--test-connectivity` is set.
4. Optionally verify model availability when `--check-model` is set.
5. Report pass or fail and list any errors; with `--verbose`, list each check.

## Output

### Text format

- Heading or first line: "Validating provider: <provider_name>".
- Per-check lines (when --verbose): check name and result (e.g. pass/fail).
- Final line: "Validation passed" or "Validation failed: N errors found" with optional summary.

**Example (passed)**

```
Validating provider: openai-gpt4

✓ Provider name matches filename
✓ Provider type is valid (openai)
✓ Model is not empty
✓ Endpoint URL is valid
✓ API connectivity: OK
✓ Model 'gpt-4' is available

Validation passed: 6/6 checks
```

**Example (failed)**

```
Validating provider: invalid-provider

✗ Provider name doesn't match filename
✗ Endpoint URL invalid: not-a-url
✗ API connectivity failed: Connection refused

Validation failed: 3 errors found
```

### JSON format (optional)

When `--format json` is supported:

- Object with `provider_name`, `valid` (bool), `checks` (array of { name, passed }), `errors` (array of strings), `total_checks`, `passed_checks`.

## Guards

- **Missing provider:** If <provider_name> is not found in the registry, error with clear message (e.g. "Provider not found: <name>").
- **Invalid config:** Report all validation errors; exit with non-zero when validation fails if desired by exit code policy.

## Implementation notes

- CLI variant under the `Provider` subcommand: `Validate { provider_name, test_connectivity, check_model, verbose }`.
- Reuse existing validation and connectivity logic from the codebase where possible.
- Reference design/context for registry and config loading only.

## Tests required

- Unit tests: validation logic for required fields and endpoint format; mock provider config.
- Integration tests: `merkle provider validate <name>` with one provider (pass and fail cases); with and without `--test-connectivity` and `--check-model`; missing provider returns clear error; `--verbose` output shape.
