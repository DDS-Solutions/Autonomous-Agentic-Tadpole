# Nexus Deep Audit: Remediation & Verification Plan (2026-10-20)

> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[NEXUS_REMEDIATION_PLAN_2026-10-20]` in audit logs.
>
> ### AI Assist Note
> Core technical remediation plan for the A-A-Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py` and `verify_ai_context.py`.

---

## 1. Plan Overview & Remediation Strategy

This plan establishes an end-to-end, deterministic engineering execution path to resolve and verify the **36 confirmed findings (12 P1, 23 P2, 1 P3)** identified in the 2026-10-20 Deep Audit.

### Execution Phasing & Dependency Graph

```mermaid
flowchart TD
    subgraph Phase 1: Build & Gate Restoration
        R01[R01: Oversight TS2339 Contract]
        R02[R02: BaseApiService no-useless-catch]
        R19[R19: API Transport Span Leaks]
    end

    subgraph Phase 2: Execution Boundary & Authorization
        S01[S01: IPC Bridge Security Bypass]
        S02[S02: Execution-Time ACL & Safe Mode]
        S03[S03: Mutation Verifier Fail-Open & Aliases]
        S04[S04: Shared Tool Cache Leak & Bypass]
        S07[S07: Breadcrumb Symlink Sandbox Escape]
        R03[R03: Categorical Dispatch Context Reset]
    end

    subgraph Phase 3: Financial & Audit Invariants
        S05[S05: Audit Stripped-Signature Bypass]
        S06[S06: A2A u64 to i64 Negative Wrap]
        R04[R04: Concurrent A2A Prepare Over-Cap]
        R05[R05: A2A Commit Partial Spend Failure]
        R06[R06: Expired Lock Commit Acceptance]
        S10[S10: Audit Private Key Permissions]
    end

    subgraph Phase 4: Actor Concurrency & Filesystem Reliability
        R07[R07: Supervisor Detached Task & Duplicate Spawn]
        R08[R08: Dynamic Skill Approval Rejection]
        R09[R09: Verification Failure False SUCCESS]
        R10[R10: Path Normalization UTF-8 Scalar Panic]
        R11[R11: File Lease Collation & Ownership]
        R12[R12: Scheduled Job Multi-Field Partial Update]
        S09[S09: Filesystem TOCTOU Race Condition]
        S11[S11: PowerShell Backup Python Interpolation]
    end

    subgraph Phase 5: Intelligence & Infrastructure Parity
        A01[A01: AST Parser Arrow Function Extraction]
        A02[A02: Review Graph Stale Snapshot & Exclusions]
        A03[A03: Unintegrated Durable & Blackboard Modules]
        A04[A04: CI Trigger Workspace Path Filters]
        A05[A05: Docker Cargo Manifest & Lock Isolation]
        A06[A06: Tailwind CSS Token Namespace]
        R13[R13: Live Voice Token Storage Location]
        R14[R14: Cron Builder Recurring Stepping Semantics]
        R15[R15: Vector Memory Schema & Method Drift]
        R16[R16: Knowledge Search Limit Cast]
        R17[R17: Default SQL Knowledge Query Matching]
        R18[R18: Impacted Tests Obfuscated Path Selection]
        S08[S08: Python Modular Skills Host Guard]
    end

    Phase 1 --> Phase 2
    Phase 2 --> Phase 3
    Phase 3 --> Phase 4
    Phase 4 --> Phase 5
```

---

## 2. Work Breakdown Structure (WBS)

### Phase 1: Build & Gate Restoration (Immediate Blockers)
**Goal**: Unblock `npm run build` and `npm run lint` so continuous integration is operational.

1. **R01 (P1): Reconcile Oversight Dashboard Contract**
   - **Target Files**:
     - [`src/pages/Oversight_Dashboard.tsx`](file:///G:/Autonomous-Agentic-Tadpole/src/pages/Oversight_Dashboard.tsx#L425)
     - [`src/data/mock_oversight.ts`](file:///G:/Autonomous-Agentic-Tadpole/src/data/mock_oversight.ts#L25)
   - **Remediation**:
     - Define an authoritative `OversightEntry` interface in a shared contracts module containing optional `error_code?: string` and `help_link?: string`, or extract these properties from the nested `tool_call.params` / `result.error`.
     - Update [`Oversight_Dashboard.tsx`](file:///G:/Autonomous-Agentic-Tadpole/src/pages/Oversight_Dashboard.tsx#L425) to safely access these fields through typed accessors without unchecked type assertions.
   - **Pass 1 Verification**: `npm run build` succeeds (`tsc -b && vite build` exits 0).

2. **R02 & R19 (P2): API Client Catch Block & Trace Span Finalization**
   - **Target File**:
     - [`src/services/base_api_service.ts`](file:///G:/Autonomous-Agentic-Tadpole/src/services/base_api_service.ts#L178-L309)
   - **Remediation**:
     - Remove redundant outer `try { ... } catch (err) { throw err; }`.
     - Wrap the request lifecycle in a `try / catch / finally` block that:
       1. Records error status and attributes in `use_trace_store` upon rejection or network failure.
       2. Clears the timeout timer in the `finally` block to prevent timer leaks.
       3. Combines caller-provided `AbortSignal` with the deadline controller.
   - **Pass 1 Verification**: `npm run lint` exits 0 with zero warnings/errors.

---

### Phase 2: Execution Boundary & Authorization Hardening
**Goal**: Enforce server-side security checks, live capabilities, and context inheritance on all tool executions.

1. **S01 (P1): Secure Local IPC Bridge**
   - **Target File**:
     - [`server-rs/src/agent/mcp/ipc_bridge.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/mcp/ipc_bridge.rs#L310-L336)
   - **Remediation**:
     - Eliminate `AppState::new_minimal_mock()`.
     - Inject the real running `Arc<AppState>` and a verified IPC caller identity into `ToolContext`.
     - Route all tool calls through `run_zero_trust_pipeline` so policy checks, audit logs, and rate limits apply identically to IPC calls.
     - Add strict framing limits (max 64 KB per line) and connection timeouts.

2. **S02 (P1): Enforce Role ACLs & Safe Mode at Tool Execution**
   - **Target Files**:
     - [`server-rs/src/agent/runner/tools/mod.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/tools/mod.rs#L128)
     - [`server-rs/src/agent/runner/tools/capability.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/tools/capability.rs)
     - [`server-rs/src/agent/runner/tools/security.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/tools/security.rs)
   - **Remediation**:
     - Call `CapabilityToken::verify(&token, &tool_name, &ctx.mission_id)` during the zero-trust pipeline pre-execution check.
     - Enforce `ctx.safe_mode`: immediately deny all write/mutation tools (`write_file`, `delete_file`, `shell_execute`, etc.) if `safe_mode == true`.
     - Enforce role ACLs at execution time, treating an empty skill grant as default-deny rather than unrestricted.

3. **S03 (P1): Canonicalize Mutation Verification Arguments & Fail Closed**
   - **Target Files**:
     - [`server-rs/src/agent/runner/tools/mod.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/tools/mod.rs#L163)
     - [`server-rs/src/agent/verification_gate.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/verification_gate.rs#L146)
     - [`server-rs/src/agent/runner/fs_tools.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/fs_tools.rs#L120)
   - **Remediation**:
     - Standardize argument extraction using typed schemas. Map `path`, `file_path`, `filename`, `file_name`, and `file` to a single canonical target.
     - When graph query returns error or empty results, mark evidence as `IncompleteEvidence` and trigger human oversight instead of falling back to self-reported blast radius (`unwrap_or(proposal.reported_blast_radius)`).

4. **S04 (P1): Authorize Tool Cache & Scope Invalidation**
   - **Target Files**:
     - [`server-rs/src/agent/runner/intelligence.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/intelligence.rs#L629)
     - [`server-rs/src/agent/runner/tools/cache.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/tools/cache.rs#L29)
   - **Remediation**:
     - Execute authorization and policy checks *prior* to cache lookup.
     - Include `agent_id`, `mission_id`, and security policy version in the cache key.
     - Track dependencies on all parameter aliases and bound the cache with an LRU eviction policy (max 1,000 entries / 50 MB).

5. **S07 (P1): Eliminate Unvalidated Fallback Paths in Codebase Read**
   - **Target File**:
     - [`server-rs/src/agent/runner/mission_tools.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/mission_tools.rs#L383-L411)
   - **Remediation**:
     - Delete calls to `SafePath::from_trusted(root.join(resolved))` and `SafePath::from_trusted(alt_path)`.
     - Route all candidate paths found via breadcrumbs or common directories through `crate::utils::security::validate_path(&root, resolved_path)`.
     - Reject symlinks resolving outside the authorized workspace boundary.

6. **R03 (P1): Preserve Context Across Categorical Tool Dispatch**
   - **Target Files**:
     - [`server-rs/src/agent/runner/mod.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/mod.rs#L160, #L201)
     - [`server-rs/src/agent/runner/tools/dispatcher.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/tools/dispatcher.rs#L152)
     - [`server-rs/src/agent/runner/swarm.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/swarm.rs#L75)
   - **Remediation**:
     - Refactor `ToolContext` to carry `depth`, `ancestor_chain`, `safe_mode`, `primary_goal`, and `accounting_receipt`.
     - Eliminate `..Default::default()` resets in `RunContext::from_tool_ctx`.
     - Ensure child missions increment depth ($depth = parent\_depth + 1$) and inherit safe mode and spend caps.

---

### Phase 3: Financial & Audit Invariants
**Goal**: Guarantee transactional atomicity, correct integer boundaries, and cryptographic integrity in A2A transactions and audit trails.

1. **S06 (P1): Checked Integer Bounds & Positive Amount Enforcement in A2A**
   - **Target Files**:
     - [`server-rs/src/routes/a2a.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/routes/a2a.rs#L94, #L115)
     - [`server-rs/migrations/20260725000200_a2a_ledger_schema.sql`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/migrations/20260725000200_a2a_ledger_schema.sql)
   - **Remediation**:
     - Replace `payload.amount_micros as i64` with `i64::try_from(payload.amount_micros)`.
     - Validate that `payload.amount_micros > 0` and `payload.amount_micros <= MAX_TRANSACTION_MICROS` ($1,000,000,000 = \$1,000$).
     - Add database `CHECK (amount_micros > 0)` constraint in SQLite migration.

2. **R04 (P1): Atomic A2A Prepare via Immediate Transaction**
   - **Target File**:
     - [`server-rs/src/routes/a2a.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/routes/a2a.rs#L58-L128)
   - **Remediation**:
     - Open a `pool.begin()` transaction with SQLite `BEGIN IMMEDIATE` isolation.
     - Execute the daily reset check, pending sum calculation, cap validation, and reservation insert within the same transaction.
     - Commit atomically; if projected spend exceeds the daily limit, roll back and return HTTP 400.

3. **R05 & R06 (P1/P2): Atomic Commit and Expired Lock Rejection**
   - **Target File**:
     - [`server-rs/src/routes/a2a.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/routes/a2a.rs#L132-L175)
   - **Remediation**:
     - In `commit_transaction`, check `WHERE lock_id = ? AND status = 'PREPARED' AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))`.
     - Execute the ledger status update and `agent_economics_meta` spend accumulation inside a single database transaction. If the second update fails, roll back the transaction so the ledger is not marked `COMMITTED` with zero recorded spend.

4. **S05 (P1): Enforce Cryptographic Signatures in Audit Verification**
   - **Target File**:
     - [`server-rs/src/security/audit.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/security/audit.rs#L346-L364, #L416)
   - **Remediation**:
     - When `self.verifying_key.is_some()`, require `entry.signature.is_some()`. If `entry.signature` is `None` or invalid, return `false` immediately (fail closed).
     - Add unambiguous delimiters to the hash payload:
       `hasher.update(format!("{}:{}:{}:{}:{}:{}", len(prev_hash), prev_hash, len(agent_id), agent_id, ...))` to prevent concatenation collisions.

5. **S10 (P2): Restrict File Permissions on Generated Private Audit Keys**
   - **Target File**:
     - [`server-rs/src/security/audit.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/security/audit.rs#L113)
   - **Remediation**:
     - On Unix systems, set file mode `0600` via `std::os::unix::fs::OpenOptionsExt`.
     - On Windows systems, configure restrictive DACLs granting access only to the executing user account.

---

### Phase 4: Actor Concurrency & Filesystem Reliability
**Goal**: Fix supervisor task detachment, path handling panics, file leases, and script injections.

1. **R07 (P1): Fix Task Detachment & Duplicate Spawning in Supervisor**
   - **Target File**:
     - [`server-rs/src/system/actors/supervisor.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/system/actors/supervisor.rs#L168-L260)
   - **Remediation**:
     - Retain `join_handle` inside an inner loop monitoring task completion and cascade signals.
     - On `origin == name_str` or `RecvError::Lagged`, continue the *inner* loop without exiting the child ownership scope.
     - If restarting, explicitly call `handle.abort()` and await task completion before spawning a replacement child.

2. **R08 & R09 (P2): Dynamic Skill Approval Receipts & Verification Truthfulness**
   - **Target File**:
     - [`server-rs/src/agent/runner/tools/mod.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/tools/mod.rs#L249, #L446, #L508)
   - **Remediation**:
     - Generate a signed, single-use `ApprovalReceipt` upon human approval and pass it to `handle_dynamic_skill`.
     - When `run_verification_script` fails, return a typed `Err(AppError::VerificationFailed)` and record `FAILED_UNVERIFIED` in the audit ledger rather than `EXECUTED SUCCESSFULLY`.

3. **R10 (P2): Prevent Path Normalization Panics on Multibyte UTF-8**
   - **Target File**:
     - [`server-rs/src/adapter/filesystem.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/adapter/filesystem.rs#L50-L58)
   - **Remediation**:
     - Replace `cleaned.len() >= 2` with character-aware prefix matching:
       ```rust
       let mut chars = cleaned.chars();
       if let (Some(first), Some(second)) = (chars.next(), chars.next()) {
           if first.is_ascii_alphabetic() && second == ':' {
               cleaned = cleaned[first.len_utf8() + second.len_utf8()..].to_string();
           }
       }
       ```

4. **R11 (P2): Canonicalize File-Write Leases**
   - **Target Files**:
     - [`server-rs/src/agent/runner/fs_tools.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/fs_tools.rs#L144)
     - [`server-rs/src/security/conflict.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/security/conflict.rs#L33)
   - **Remediation**:
     - Key leases by canonicalized absolute path and workspace ID.
     - Return an RAII guard carrying a unique generation UUID; release only if the generation matches.

5. **R12 (P2): Transactional Multi-Field Scheduled Job Updates**
   - **Target File**:
     - [`server-rs/src/agent/continuity/scheduler.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/continuity/scheduler.rs#L73)
   - **Remediation**:
     - Execute multi-field job updates in a single SQL statement or inside a `BEGIN IMMEDIATE` transaction with optimistic concurrency check (`version = ?`).

6. **S11 (P2): Sanitize PowerShell Database Backup Arguments**
   - **Target File**:
     - [`scripts/backup_db.ps1`](file:///G:/Autonomous-Agentic-Tadpole/scripts/backup_db.ps1#L26-L37)
   - **Remediation**:
     - Pass database and backup paths as command-line arguments to Python (`sys.argv[1]`, `sys.argv[2]`) instead of interpolating strings into `$pyScript`.

---

### Phase 5: Intelligence & Infrastructure Parity
**Goal**: Update AST parsers, reconcile review graphs, align build contexts, and resolve vector memory drift.

1. **A01 & R18 (P2): AST Parser Arrow Functions & Qualified Symbols**
   - **Target Files**:
     - [`server-rs/src/utils/parser.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/utils/parser.rs#L117, #L159)
     - [`server-rs/src/intelligence/service.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/intelligence/service.rs#L447)
   - **Remediation**:
     - Add AST node matching for `lexical_declaration` and `variable_declarator` containing `arrow_function`.
     - In `get_impacted_tests`, preserve actual workspace-relative file paths before obfuscation and output actionable test paths.

2. **A02 (P2): Graph Discovery Exclusions**
   - **Target Files**:
     - [`server-rs/src/intelligence/graph_store.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/intelligence/graph_store.rs#L378)
     - [`server-rs/src/intelligence/graph/discover.rs`](file:///G:/Autonomous-Agentic-Tadpole/server-rs/src/intelligence/graph/discover.rs#L42)
   - **Remediation**:
     - Exclude pattern `target*` (including `target_check`, `target_clippy`, `target_test`).
     - Align exclusion lists between persisted and live graph discovery.

3. **A04 & A05 (P2): CI Filters & Docker Cargo Workspace Context**
   - **Target Files**:
     - [`.github/workflows/ci.yml`](file:///G:/Autonomous-Agentic-Tadpole/.github/workflows/ci.yml#L7, #L132)
     - [`Dockerfile`](file:///G:/Autonomous-Agentic-Tadpole/Dockerfile#L54)
   - **Remediation**:
     - Include `Cargo.toml`, `Cargo.lock`, and `rust-toolchain.toml` in CI path filters.
     - Copy the root workspace manifest and lockfile in `Dockerfile` and build with `--locked`.

4. **A06 (P3): Tailwind Typography Token Namespace**
   - **Target File**:
     - [`src/index.css`](file:///G:/Autonomous-Agentic-Tadpole/src/index.css#L60)
   - **Remediation**:
     - Rename custom properties from `--font-size-*` to `--text-*` to align with Tailwind CSS v4 variables.

5. **R13, R14, R15, R16, R17 (P2): Client & Knowledge Repairs**
   - Reconcile `Live_Voice_Hub` token read with in-memory settings store.
   - Clarify recurring cron interval semantics in `cron_builder.ts`.
   - Update `VectorMemory` methods or feature-gate callers in `knowledge_store`.
   - Add bounds checking for query limit in `knowledge_store/search.rs`.

---

## 3. Dual-Pass Verification Matrix

For every phase, both passes must be executed and recorded:

| Phase | Pass 1: Synthesis & Type Safety | Pass 2: Adversarial Invariant Gate |
| :--- | :--- | :--- |
| **Phase 1** | `npm run build` && `npm run lint` | Inspect trace store: 0 leaked spans on 500 error |
| **Phase 2** | `cargo check --manifest-path server-rs/Cargo.toml` | **Gate 4 (Trust Boundaries)**: IPC denied without auth; safe-mode blocks mutation injection |
| **Phase 3** | `cargo check --manifest-path server-rs/Cargo.toml` | **Gate 2 & 3 (Relational & Concurrency)**: 10 concurrent A2A prepares never exceed daily cap; stripped signatures fail verification |
| **Phase 4** | `cargo check --manifest-path server-rs/Cargo.toml` | **Gate 1 (Scheduler Invariants)**: Supervisor cascade lag produces zero duplicate child tasks |
| **Phase 5** | `cargo check` && `python execution/parity_guard.py .` | Graph rebuild ignores `target_*`; impacted tests return valid existing test paths |

---

## 4. Execution Checkpoints & Sign-Off

- [x] **Checkpoint 1**: Phase 1 complete. Frontend build and ESLint pass cleanly in CI.
- [x] **Checkpoint 2**: Phase 2 complete. Zero-Trust pipeline verified against IPC and safe mode attacks.
- [x] **Checkpoint 3**: Phase 3 complete. A2A stress test passes with zero negative spend or cap overruns.
- [x] **Checkpoint 4**: Phase 4 complete. Supervisor and filesystem UTF-8 panics verified resolved.
- [x] **Checkpoint 5**: Phase 5 complete. Parity guard passes, graph exclusions verified, and full documentation synced.

<!-- Telemetry Tag duplicate reference: [NEXUS_REMEDIATION_PLAN_2026-10-20] -->
