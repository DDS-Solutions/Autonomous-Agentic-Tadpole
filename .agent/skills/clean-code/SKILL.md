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
name: clean-code
description: Pragmatic coding standards - concise, direct, no over-engineering, no unnecessary comments across TypeScript, Rust, and Python.
when_to_use: "Always active for ALL code writing. Enforces concise, direct coding standards, the testing pyramid, Dual-Pass Nexus invariants, and performance best practices across polyglot ecosystems."
allowed-tools: Read, Write, Edit
version: 2.2
priority: CRITICAL
---

# Clean Code - Pragmatic AI Coding Standards

> **CRITICAL SKILL** - Be **concise, direct, and solution-focused**.

---

## Core Principles

| Principle | Rule |
|-----------|------|
| **SRP** | Single Responsibility - each function/class does ONE thing |
| **DRY** | Don't Repeat Yourself - extract duplicates, reuse |
| **KISS** | Keep It Simple - simplest solution that works |
| **YAGNI** | You Aren't Gonna Need It - don't build unused features |
| **Boy Scout** | Leave code cleaner than you found it |

---

## Naming & Function Rules

### Naming
| Element | Convention | Example |
|---------|------------|----------|
| **Variables** | Reveal intent | `userCount` instead of `n` |
| **Functions** | Verb + Noun | `getUserById()` instead of `user()` |
| **Booleans** | Question form | `isActive`, `hasPermission`, `canEdit` |
| **Constants** | SCREAMING_SNAKE | `MAX_RETRY_COUNT` |

> **Rule:** If you need a comment to explain a name, rename the variable/function.

### Functions
| Rule | Constraint |
|------|-------------|
| **Small** | Max 20 lines; ideal range 5-10 lines |
| **One Thing** | Single purpose; one level of abstraction per function |
| **Few Args** | Max 3 arguments; prefer 0-2 (use objects for more) |
| **No Side Effects** | Do not mutate inputs unexpectedly |

---

## Code Structure & Patterns

| Pattern | Application |
|---------|-------------|
| **Guard Clauses** | Return early for edge cases to avoid `else` blocks |
| **Flat > Nested** | Max 2 levels of nesting. If 3+, extract to a function |
| **Composition** | Compose small, pure functions into complex workflows |
| **Colocation** | Keep related logic/styles/tests physically close |

---

## Polyglot & Rust Clean Code Standards

### Rust Core Paradigms (`server-rs` & `src-tauri`)
| Practice | Guideline |
|----------|-----------|
| **Error Handling** | Always use `Result<T, AppError>` or `Option<T>` with `?`. **Never** use `.unwrap()` or `.expect()` in production route handlers. |
| **Ownership & Borrowing** | Prefer borrowing (`&str`, `&[T]`) over premature `.clone()`. Use `Arc<T>` only for shared state across async boundaries. |
| **Async Task Safety** | Always capture `tokio::task::JoinHandle`. Never spawn fire-and-forget tasks without tracking or structured cancellation. |
| **RAII Resource Management** | Scope locks (`tokio::sync::MutexGuard`, DB transactions) tightly; drop locks before awaiting across network boundaries. |
| **Zero-Cost Abstractions** | Leverage iterators and combinators (`.map()`, `.filter()`, `.and_then()`) over manual mutable loops where readable. |

### Dual-Pass Nexus Invariant Protocol
Before marking any feature or patch complete, verify the four system invariant gates:
1. **Scheduler Invariants:** Spawned background tasks retain handles; calling threads register tracking before completion.
2. **Relational Invariants:** Foreign key deletions are child-first under SQLite `PRAGMA foreign_keys = ON`.
3. **Concurrency Invariants:** Optimistic Concurrency Control (OCC) retry loops must never degrade into untracked Last-Write-Wins.
4. **Trust Boundaries:** Authorization, quota, and role checks must inspect authenticated session context—never untrusted request bodies.

---

## AI Operational Style

### ⚡ The "Fast-Path" (User Directed)
*When the user provides a direct instruction:*
- **Feature Request** $\rightarrow$ Write it directly.
- **Bug Report** $\rightarrow$ Fix it immediately; do not explain the bug unless asked.
- **Ambiguous Requirement** $\rightarrow$ Ask for clarification; do not assume.

### 🛡️ The "Safety-Path" (Script Directed)
*When a validation script (Linter, Type-Check, etc.) finds an error:*
1. **Run** $\rightarrow$ **Parse** $\rightarrow$ **Summarize** (see "Script Output Handling" below).
2. **Wait for confirmation** before applying fixes.

---

## 🧪 Testing Pyramid Strategy
Every change must adhere to the following distribution:

1. **Unit Tests (70%)**: Test individual functions/classes in isolation. (Fastest)
2. **Integration Tests (20%)**: Test interaction between modules/API endpoints.
3. **E2E Tests (10%)**: Test critical user journeys (Playwright/Cypress). (Slowest)

> **Rule:** New features must include at least one Unit Test before being marked complete.

---

## 🛠️ Version Control (Git)
- **Commit Messages**: Use the imperative mood.
  - ✅ `Add user authentication logic`
  - ❌ `Added user authentication logic` or `Fixing some bugs`
- **Atomic Commits**: One logical change per commit.

---

## Anti-Patterns (DON'T)

| ❌ Pattern | ✅ Fix |
|-----------|-------|
| Commenting obvious code | Delete the comment; make code self-documenting |
| Helper for a one-liner | Inline the code |
| Factory for 2 objects | Direct instantiation |
| `utils.ts` dump | Move function to the domain where it is used |
| "First, I will..." | Just write the code |
| Magic numbers | Define as `CONSTANTS` at the top of the file |
| God functions | Split by responsibility (SRP) |

---

## 🔴 Before Editing ANY File (THINK FIRST!)

**Perform this mental trace before writing:**
1. **Upstream**: What imports this file? (Will I break them?)
2. **Downstream**: What does this file import? (Are the interfaces changing?)
3. **Coverage**: Which tests cover this logic? (Will they fail?)
4. **Scope**: Is this a shared component? (Does the change leak into other views?)

> 🔴 **Rule:** Edit the target file AND all affected dependent files in the **SAME task**.

---

## 🔴 Self-Check Before Completing (MANDATORY)

| Check | Question |
|-------|----------|
| ✅ **Goal met?** | Did I do exactly what the user asked? |
| ✅ **Files edited?** | Did I update all dependent imports and signatures? |
| ✅ **Code works?** | Did I verify the logic or run local compiler/tests? |
| ✅ **No errors?** | Do Lint, TypeScript, and `cargo check` pass? |
| ✅ **Test Added?** | Is there a unit/invariant test for new logic? |
| ✅ **Parity Intact?** | Does `python execution/parity_guard.py` report 0 errors? |

---

## Verification Scripts (MANDATORY)

### Layer 3 Sovereign Execution Suite
| Domain | Purpose | Command |
|--------|---------|---------|
| **Core Parity** | Parity Guard (Routes, Invariants, Docs) | `python execution/parity_guard.py` |
| **Context Alignment** | AI Context & System Map | `python execution/verify_ai_context.py` |
| **Security Audit** | Sovereign Gitleaks & Secret Scanner | `python execution/security_scan.py` |
| **Rust Engine** | Rust Fast Check (`server-rs` / `src-tauri`) | `python execution/cargo_fast_check.py` |
| **Unified Linter** | Multi-Language Lint & Type Analysis | `python .agent/skills/lint-and-validate/scripts/lint_runner.py .` |

### Agent $\rightarrow$ Specialized Script Mapping
| Agent | Script | Command |
|-------|--------|---------|
| **frontend-specialist** | UX Audit | `python .agent/skills/frontend-design/scripts/ux_audit.py .` |
| **frontend-specialist** | A11y Check | `python .agent/skills/frontend-design/scripts/accessibility_checker.py .` |
| **backend-specialist** | API Validator | `python .agent/skills/api-patterns/scripts/api_validator.py .` |
| **mobile-developer** | Mobile Audit | `python .agent/skills/mobile-design/scripts/mobile_audit.py .` |
| **database-architect** | Schema Validate | `python .agent/skills/database-design/scripts/schema_validator.py .` |
| **seo-specialist** | SEO Check | `python .agent/skills/seo-fundamentals/scripts/seo_checker.py .` |
| **seo-specialist** | GEO Check | `python .agent/skills/geo-fundamentals/scripts/geo_checker.py .` |
| **performance-optimizer** | Lighthouse | `python .agent/skills/performance-profiling/scripts/lighthouse_audit.py <url>` |
| **test-engineer** | Test Runner | `python .agent/skills/testing-patterns/scripts/test_runner.py .` |
| **test-engineer** | Playwright | `python .agent/skills/webapp-testing/scripts/playwright_runner.py <url>` |
| **Any agent** | Type Coverage | `python .agent/skills/lint-and-validate/scripts/type_coverage.py .` |
| **Any agent** | i18n Check | `python .agent/skills/i18n-localization/scripts/i18n_checker.py .` |

### 🔴 Script Output Handling (READ $\rightarrow$ SUMMARIZE $\rightarrow$ ASK)

**When running a validation script, you MUST:**
1. **Capture** all output.
2. **Parse** errors vs warnings.
3. **Summarize** using the following format:

```markdown
## Script Results: [script_name.py]

### ❌ Errors Found (X items)
- [File:Line] Error description
### ⚠️ Warnings (Y items)
- [File:Line] Warning description
### ✅ Passed (Z items)
- Check name passed

**Should I fix these X errors?**
```

4. **Wait for user confirmation** before fixing.
5. **Re-run** script after fixing to confirm resolution.

[//]: # (Metadata: [SKILL])

--- End of SKILL.md ---