> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[trace_audit_plan]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# Codebase Trace Audit & Remediation Plan

## Goal
Resolve all identified lint breaks, service API gaps, and test omissions discovered during the end-to-end AST graph trace across `G:\Autonomous-Agentic-Tadpole`.

## Tasks
- [x] Task 1: Resolve TypeScript lint breaker and hook warnings → Verify: Run `npm run lint` and confirm 0 errors and 0 warnings.
- [x] Task 2: Remove unused imports in Rust test files → Verify: Run `cargo check --workspace` in `server-rs` with 0 warnings.
- [x] Task 3: Bridge the frontend intelligence API gap by adding `get_impacted_tests` to `intelligence_api_service.ts` → Verify: Run `npx tsc --noEmit`.
- [x] Task 4: Expand route test coverage for `/v1/intelligence/resolve` and `/v1/intelligence/impacted-tests` in `server-rs/src/routes/intelligence_tests.rs` → Verify: Run `cargo test --manifest-path server-rs/Cargo.toml`.
- [x] Task 5: Final Release Gate Verification → Verify: Run `python execution/verify_all.py . --no-e2e` and confirm 100% pass across all categories.

## Done When
- [x] `npm run lint` succeeds with 0 errors.
- [x] `cargo check --workspace` in `server-rs` succeeds with 0 warnings.
- [x] All 848 Vitest tests, 55 Python tests, and Rust integration tests pass.
- [x] Full release gate (`verify_all.py`) completes with 0 failures.

## Notes
- Adheres strictly to the 3-Layer Architecture (Directives / Orchestration / Execution).
- Dual-Pass Nexus Invariants must be enforced on any Rust route changes.
- Avoid code bloat: retain lean, pragmatic fixes without unnecessary layers of abstraction.

[//]: # (Metadata: [trace_audit_plan])

