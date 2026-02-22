#!/usr/bin/env bash
# Domain boundary guard: fail if code violates cross-domain use rules.
# Run from repo root. Add to CI to prevent regressions.
# See design/refactor/PLAN.md Phase 10 and this directory's README.

set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
VIOLATIONS=0

# CLI must not reach into context::frame::storage; use context::frame::open_storage or api instead.
if grep -rq --include='*.rs' 'context::frame::storage' src/cli/ 2>/dev/null; then
  echo "Boundary violation: src/cli/ must not use context::frame::storage (use context::frame::open_storage or api)."
  VIOLATIONS=$((VIOLATIONS + 1))
fi

# Legacy top-level composition removed; use context::query::composition.
if grep -rq --include='*.rs' 'crate::composition::' src/ 2>/dev/null; then
  echo "Boundary violation: crate::composition:: is removed; use crate::context::query::composition."
  VIOLATIONS=$((VIOLATIONS + 1))
fi

# Tooling is removed; no references in src.
if grep -rq --include='*.rs' 'crate::tooling::' src/ 2>/dev/null; then
  echo "Boundary violation: crate::tooling:: is removed; use crate::cli, crate::workspace, crate::agent."
  VIOLATIONS=$((VIOLATIONS + 1))
fi

if [ "$VIOLATIONS" -gt 0 ]; then
  echo "Total violations: $VIOLATIONS"
  exit 1
fi
echo "Domain boundary check passed."
exit 0
