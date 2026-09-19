# 🛡️ Directive: Mandatory Dual-Pass Nexus Protocol (SOP-NEXUS-01)

> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Local optimization myopia, uncoordinated async scheduling races, broken foreign key cascades, or client body authorization bypasses.
> - **Telemetry Link**: Search `[nexus_dual_pass]` in audit logs.
>
> ### AI Assist Note
> Core engineering invariant standard for the Tadpole OS codebase.
>
> ### 🔍 Debugging & Observability
> Traceability via `execution/nexus_adversarial_guard.py` and `parity_guard.py`.

---

## 🎯 Primary Objective
Eliminate "Local Optimization Myopia" across all code generation and patching activities. Ensure that every code modification is evaluated not merely to make the immediate function succeed, but under an adversarial stress test verifying global invariants across concurrency, persistence, scheduling, and trust boundaries.

---

## 📋 The 2-Pass Architecture

Every code edit within `server-rs`, `src`, or `execution/` must pass through two sequential gates:

### Pass 1: Functional Synthesis (The Builder)
- Implement the requested feature, endpoint, or bug fix.
- Ensure strict type safety, idiomatic patterns, and compiler success (`cargo check` / `tsc`).
- Verify standard unit tests pass.

### Pass 2: Adversarial Stress Test (The Nexus Auditor)
Before declaring any code modification complete, the engineer must explicitly evaluate and document the following four core invariants:

1. **Scheduler Invariants (Async Concurrency):**
   - *Question:* Can a spawned asynchronous task (e.g. `tokio::spawn`) complete or error before the parent thread executes the subsequent statement (e.g. tracking map insertion)?
   - *Standard:* Never rely on implicit thread scheduling order. Synchronize lifecycle handles (via channels, pre-allocation, or atomic registration) before fallible execution begins.

2. **Relational Invariants (Database & Foreign Keys):**
   - *Question:* What foreign keys reference this table? Is deletion or insertion child-first?
   - *Standard:* With `PRAGMA foreign_keys = ON`, parent rows cannot be deleted while child rows exist in `mission_history`, `mission_logs`, `swarm_context`, `agent_directives`, or metadata tables. Deletions must proceed in strict bottom-up dependency order.

3. **Concurrency Invariants (Locking & Contention):**
   - *Question:* Does a retry loop or reconciliation quietly turn Optimistic Concurrency Control (OCC) into an untracked Last-Write-Wins clobber?
   - *Standard:* On version collision, fail fast with `409 Conflict` and force the caller to re-read fresh state. Never auto-stamp stale in-memory structs with the latest database version.

4. **Trust Boundary Invariants (Security & Authorization):**
   - *Question:* Does this security, quota, suspension, or role check inspect unauthenticated client request body properties?
   - *Standard:* Authorization decisions must derive strictly from trusted cryptographic session contexts or validated security headers (`x-tadpole-role`), never from client-controlled JSON bodies (`payload.user_id`, `payload.auto_resume`).

---

## 🛠️ Automated Enforcement
This protocol is deterministically enforced by:
- `execution/nexus_adversarial_guard.py`: Static AST & regex scanner targeting known anti-patterns.
- `execution/parity_guard.py`: Automated repository health audit.

<!-- Telemetry Tag: [nexus_dual_pass] -->
