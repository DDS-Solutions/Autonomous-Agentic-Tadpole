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
name: simplify-code
description: Reduce complexity of over-engineered code. Identify unnecessary abstractions, remove dead code, flatten deep nesting, and simplify logic while preserving behavior.
when_to_use: "When code is over-engineered, overly abstract, deeply nested, or more complex than needed. When user asks to 'simplify', 'clean up', 'reduce complexity', or 'make this simpler'. NOT for adding new features."
allowed-tools: Read, Write, Edit, Grep, Glob
effort: medium
---

# Simplify Code — Reduce Unnecessary Complexity

> The best code is the code you don't have to write. The second best is the code anyone can read.

## Core Principle

```
Complexity is a cost. Every abstraction, every indirection, every clever pattern
adds cognitive load. Simplify ruthlessly unless complexity serves a clear purpose.
```

---

## Simplification Checklist

### 1. Unnecessary Abstractions
| Smell | Simplification |
|---|---|
| Wrapper class that just delegates | Remove wrapper, use the inner class directly |
| Factory that creates only one type | Replace with direct constructor |
| Strategy pattern with one strategy | Replace with simple function |
| Interface with one implementation | Remove interface, use the class |
| Abstract class with one child | Merge into the child class |
| Config object for 2 values | Use function parameters |

### 2. Dead Code
| Smell | Action |
|---|---|
| Unused imports | Remove |
| Unreachable branches | Remove (check tests first) |
| Commented-out code | Remove (it's in git history) |
| Unused variables/functions | Remove |
| TODO comments older than 6 months | Remove or create issue |
| Feature flags for launched features | Remove flag, keep the code |

### 3. Deep Nesting
```javascript
// ❌ Before: 4 levels deep
function process(data) {
  if (data) {
    if (data.items) {
      for (const item of data.items) {
        if (item.active) {
          doSomething(item)
        }
      }
    }
  }
}

// ✅ After: Early returns + filter
function process(data) {
  if (!data?.items) return

  data.items
    .filter(item => item.active)
    .forEach(doSomething)
}
```

### 4. Over-Parameterized Functions
```typescript
// ❌ Before: 8 parameters
function createUser(name, email, age, role, dept, active, verified, avatar) { }

// ✅ After: Object parameter
function createUser(opts: CreateUserOpts) { }
```

### 5. Premature Optimization
| Smell | Simplification |
|---|---|
| Custom cache for <100 items | Remove cache, measure first |
| Memoization on cheap functions | Remove memo |
| Lazy loading for small modules | Use direct import |
| Redundant wrapper around single stdlib function | Inline or call stdlib directly |

---

## Polyglot Simplification Idioms

### Rust (`server-rs` & `src-tauri`)
```rust
// ❌ Before: Verbose error matching and manual unwraps
fn get_user_email(id: &str, db: &Database) -> Result<String, AppError> {
    match db.find_user(id) {
        Ok(user_opt) => match user_opt {
            Some(user) => Ok(user.email),
            None => Err(AppError::NotFound("User not found".into())),
        },
        Err(e) => Err(AppError::Database(e)),
    }
}

// ✅ After: Idiomatic combinators and the `?` operator
fn get_user_email(id: &str, db: &Database) -> Result<String, AppError> {
    db.find_user(id)?
        .map(|u| u.email)
        .ok_or_else(|| AppError::NotFound("User not found".into()))
}
```

### Python (`execution/`)
```python
# ❌ Before: Imperative accumulation
def get_active_agent_ids(agents):
    result = []
    for agent in agents:
        if agent.get("status") == "active":
            result.append(agent.get("id"))
    return result

# ✅ After: List comprehension with get() guard
def get_active_agent_ids(agents):
    return [a["id"] for a in agents if a.get("status") == "active"]
```

---

## 🛡️ Invariant Protection Gate (NEVER SIMPLIFY)

> [!CAUTION]
> In Tadpole OS, code that appears complex often enforces critical **Dual-Pass Nexus Invariants**. You must NEVER simplify, inline, or remove code that enforces these gates:

| Invariant Category | What Looks Complex | Why It MUST Be Preserved |
|-------------------|--------------------|---------------------------|
| **Scheduler Invariants** | Storing `tokio::task::JoinHandle` in registry tables | Prevents orphaned background tasks and enables structured shutdown. |
| **Relational Invariants** | Multi-step child-first deletion sequences | Required when SQLite `PRAGMA foreign_keys = ON` to prevent constraint violations. |
| **Concurrency Invariants** | Version-checking retry loops (OCC) | Prevents silent Last-Write-Wins overwrites under concurrent agent missions. |
| **Trust Boundaries** | Redundant server-side role and quota checks | Request bodies from clients cannot be trusted; auth context must be re-verified. |

---

## Simplification Protocol

### Step 1: Identify Complexity
```
- Count nesting levels (target: ≤3)
- Count function parameters (target: ≤4)
- Count lines per function (target: ≤30)
- Count abstractions per feature (target: ≤2)
- Check for dead code (target: 0)
```

### Step 2: Verify Understanding
Before simplifying, ensure you understand:
- What the code does (not what it looks like it does)
- Why it was written this way (maybe there's a reason)
- What tests cover it (simplification must not break tests)

### Step 3: Simplify Incrementally
```
1. Remove dead code first (safest)
2. Flatten nesting with early returns
3. Inline trivial abstractions
4. Merge related functions
5. Simplify data structures
```

### Step 4: Verify Behavior Preserved
```bash
# Node / Frontend
npm run test           # All existing tests still pass
npm run build          # Still compiles

# Rust Backend (server-rs & src-tauri)
cargo check --workspace
cargo test

# Sovereign Parity & Invariants
python execution/parity_guard.py
python execution/verify_ai_context.py
```

---

## When NOT to Simplify

| Situation | Why Keep Complexity |
|---|---|
| **Dual-Pass Nexus Invariants** | Concurrency, scheduler tracking, relational cascades, trust checks |
| **Performance-critical hot path** | Optimization may look complex but is necessary |
| **Required by framework/library** | External constraints (Axum extractors, Tauri IPC, React Error Boundaries) |
| **Explicitly requested pattern** | User chose this architecture |
| **Will need extension soon** | Abstraction prepares for known growth |

> **Ask first:** "This pattern seems over-engineered. Should I simplify it, or is there a reason for the abstraction?"

[//]: # (Metadata: [SKILL])
