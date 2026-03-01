# Domain Metadata Separation Spec

Date: 2026-03-01

## Intent

Separate frame metadata contracts from node metadata and agent metadata so policy changes stay local to the correct domain.

## Problem

Current code uses generic metadata maps across multiple domains.
This creates coupling that increases blast radius for every metadata contract change.

## Target Boundary Model

Ownership:
- `src/metadata` owns frame metadata key registry, mutability classes, and validation rules
- `src/context` owns frame creation and delegates metadata validation
- `src/store` owns node record metadata with store specific rules
- `src/agent` owns agent profile metadata with agent specific rules

Rules:
- frame metadata policies must not apply to node record metadata by default
- frame metadata policies must not apply to agent profile metadata by default
- cross domain conversion between metadata forms must use explicit adapters

## Required Changes

1. define explicit metadata types per domain
- frame metadata type
- node metadata type
- agent metadata type

2. add explicit conversion adapters only where data crosses domains

3. stop using generic free form map aliases at shared boundaries

4. add contract tests that verify policy isolation

## Done Criteria

- frame metadata keys are validated by frame metadata contracts only
- node metadata writes are unaffected by frame metadata key allow list changes
- agent metadata writes are unaffected by frame metadata key allow list changes

## Verification

- characterization tests for existing node metadata behavior
- characterization tests for existing agent metadata behavior
- new tests for frame metadata policy isolation
