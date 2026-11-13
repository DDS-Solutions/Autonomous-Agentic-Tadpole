> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Registry:Skills**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[SKILL]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

---
name: lint-and-validate
description: "Automatic quality control, linting, and static analysis procedures. Use after every code modification to ensure syntax correctness and project standards. Triggers onKeywords: lint, format, check, validate, types, static analysis."
when_to_use: "When running linters, type checkers, or code formatters. After any code change that needs quality validation."
allowed-tools: Read, Glob, Grep, Bash
---

# Lint and Validate Skill

> **MANDATORY:** Run appropriate validation tools after EVERY code change. Do not finish a task until the code is error-free.

### Procedures by Ecosystem

#### Rust (`server-rs` & `src-tauri`)
1. **Compilation Check:** `cargo check --workspace` (or `python execution/cargo_fast_check.py`)
2. **Linter (Clippy):** `cargo clippy --workspace --all-targets --all-features -D warnings`
3. **Tests & Invariants:** `cargo test`

#### Node.js / TypeScript (`src/`)
1. **Lint/Fix:** `npm run lint` or `npx eslint "path" --fix`
2. **Types:** `npx tsc --noEmit`
3. **Security:** `npm audit --audit-level=high`

#### Python (`execution/`)
1. **Linter (Ruff):** `ruff check "path" --fix` (Fast & Modern)
2. **Security (Bandit):** `bandit -r "path" -ll`
3. **Types (MyPy):** `mypy "path"`

#### Sovereign Layer 3 Integrity Suite
1. **Parity Guard:** `python execution/parity_guard.py` (Validates 143 routes, env vars, invariants)
2. **AI Context Alignment:** `python execution/verify_ai_context.py`
3. **Security & Secrets:** `python execution/security_scan.py`

## The Quality Loop
1. **Write/Edit Code**
2. **Run Linter & Typechecks:** Multi-ecosystem check via `python .agent/skills/lint-and-validate/scripts/lint_runner.py .`
3. **Run Sovereign Gates:** `python execution/parity_guard.py`
4. **Fix & Repeat:** Submitting code with failures is NOT allowed.

## Error Handling
- If `cargo clippy` fails: Resolve borrows, unneeded clones, and lint warnings immediately.
- If `lint` fails: Fix style or syntax issues immediately.
- If `tsc` fails: Correct type mismatches before proceeding.
- If `parity_guard` fails: Reconcile router, env-var, or documentation parity mismatches.

---
**Strict Rule:** No code should be committed or reported as "done" without passing these checks.

---

## Scripts

| Script | Purpose | Command |
|--------|---------|---------|
| `lint_runner.py` | Unified polyglot lint check | `python .agent/skills/lint-and-validate/scripts/lint_runner.py <project_path>` |
| `type_coverage.py` | Type coverage analysis | `python .agent/skills/lint-and-validate/scripts/type_coverage.py <project_path>` |


[//]: # (Metadata: [SKILL])
