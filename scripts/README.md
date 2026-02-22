# Scripts

## check_domain_boundaries.sh

Ensures no cross-domain internal reach-through after Phase 10. Run from repo root:

```bash
./scripts/check_domain_boundaries.sh
```

**Rules enforced:**

- `src/cli/` must not use `context::frame::storage`; use `context::frame::open_storage` or api.
- No use of removed `crate::composition::`; use `crate::context::query::composition`.
- No use of removed `crate::tooling::`; use `crate::cli`, `crate::workspace`, `crate::agent`.

Add this script to your CI pipeline (e.g. in the same job as `cargo test` or a dedicated step). To extend the boundary matrix, edit the script and document new rules here and in `design/refactor/PLAN.md` Phase 10.
