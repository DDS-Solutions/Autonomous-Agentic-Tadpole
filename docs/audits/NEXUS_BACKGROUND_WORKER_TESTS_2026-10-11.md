> [!IMPORTANT]
> **@docs ARCHITECTURE:Core**
> **Failure Path**: Worker failure paths without direct regression coverage.
> **Telemetry Link**: Search `[NEXUS_BACKGROUND_WORKER_TESTS_2026-10-11]` in audit evidence.
>
> ### AI Assist Note
> Records the requested background worker tests, cancellation correction, and verification.
> ### 🔍 Debugging & Observability
> Evidence is retained under `.tmp/nexus-audit/worker-tests-*.log`.

# Section 1: Executive Summary & Overview (The Nexus Architect)

The requested scheduler, event-monitor, and cognitive-memory regression coverage is complete. All fourteen new tests pass: five scheduler paths, four event paths, and five consolidation paths. The complete Rust suite passes 547 tests, with one existing ignored test.

The first run with local provider access exposed a real scheduler cancellation defect: aborting a runner skipped its normal tracking cleanup. The executor now awaits mission recovery and conditionally removes the cancelled execution's handle, preserving a replacement execution. The existing strict persistence helper is shared within the crate; no database schema or public API is added.

# Section 2: Pillar I — Architectural Integrity (The Master Architect)

Tests remain next to the three source modules. A small `#[cfg(test)]` fixture in `server-rs/src/test_support.rs` shares a loopback OpenAI-compatible provider and a state with real SQLite-backed quotas between scheduler and cognitive-memory tests. It is excluded from production builds and adds no dependency or testing framework.

The Force Graph handlers in `server-rs/src/routes/intelligence.rs` were reviewed. The local graph driver rebuilt both graph representations: the complete symbol graph contains 3,699 nodes, 18,019 edges, and 708 parsed files; the persistent graph contains 5,620 nodes and 47,278 edges. The read-only trace inventory checked graph integrity and reverse call/reference relationships. This analysis is a static approximation: name resolution includes ambiguous callers and its test labels are not measured coverage. The full Rust suite is included in validation because scheduler admission interacts with runner persistence, gateway preemption, and shutdown.

# Section 3: Pillar II — Reliability & Robustness (The QA Principal)

| Area | New regression coverage |
|---|---|
| Scheduler | Rejecting SQLite trigger prevents run insertion, releases the database claim, and leaves no run/runner. |
| Scheduler | Cancelled execution records failure, increments the failure counter, finalizes the mission, and releases registry/database state and tracking. |
| Scheduler | Cancellation recovery preserves a replacement handle and active mission. |
| Scheduler | Eight competing admissions skip while the original execution retains ownership; only that execution reaches the provider. |
| Scheduler | Successful execution records the exact run's mission and aggregate reported usage and releases the agent. |
| Event monitor | A deterministic queue overflow is followed by processing both a retained event and a later event. |
| Event routing | Duplicate mappings admit a job once; disabled jobs create no run or runner and do not mutate the agent. |
| Event filtering | Quotes, percent signs, underscores, and SQL-shaped text are matched as literal bound input. |
| Cognitive memory | Provider failure and whitespace-only output preserve source IDs/text/classification, agent version, and usage. |
| Cognitive memory | Suspension and buffered quota exhaustion prevent all provider requests. |
| Cognitive memory | Successful consolidation compresses both sources, creates one summary, persists reported usage, and is not charged again on a second scan. |

The provider blocks its first request with a semaphore where an in-flight execution is required. Request notifications coordinate tests without timing sleeps. The event lag test publishes beyond the real channel capacity without yielding on a current-thread runtime. Asynchronous waits are bounded.

The four adversarial gates are addressed as follows:

- **Scheduler:** The existing registration barrier remains intact. Cancellation compares the tracked task ID before recovery and removes a handle only when that ID still matches. The replacement regression covers late cleanup.
- **Relational:** No schema or deletion behavior changes. `scheduled_job_runs.job_id` references `scheduled_jobs` with cascading deletion; mission children retain their existing foreign keys. Trigger rejection and terminal persistence exercise the migrated SQLite schema with foreign keys enabled.
- **Concurrency:** Recovery uses the lifecycle lock, strict versioned persistence, and terminal mission identity checks. No retry adopts a newer version or bypasses an optimistic concurrency conflict.
- **Trust:** Suspension and quotas are read from server-owned registry state and the real budget guard. Tests make no client-supplied role or quota authoritative.

# Section 4: Pillar III — Security Posture (The Chief Auditor)

Provider fixtures bind only to `127.0.0.1` on ephemeral ports. Their client bypasses proxy configuration and sends `User-Agent: TadpoleOS/1.1.58`, which the fixture verifies. Credentials are synthetic and no external provider is contacted. Cargo runs offline with the lockfile fixed.

The initial sandbox denied loopback connections; that run is retained as environmental evidence, not a passing result. Local test access was used for subsequent provider tests. Event-filter tests verify parameter isolation; suspension and buffered-quota tests verify that denied inference produces zero requests and preserves source memories. Existing upstream dependency and native/provider validation findings remain outside this test task.

# Section 5: Pillar IV — Testing Rigor (The Testing Expert)

The most critical function is `agent::continuity::executor::execute_job`.

| Path | Action | Required outcome |
|---|---|---|
| Happy | Execute a scheduled mission against the local provider. | Run completes with mission identity and aggregate usage; agent and tracking are released. |
| Failure | Reject run insertion or abort the execution during inference. | Failed insertion releases the claim; aborted execution records failure and finalizes its mission and resources. |
| Edge | Admit eight competing jobs or replace an aborted execution before cleanup. | Competitors skip without preemption; replacement state and tracking survive late cleanup. |

**Pass 1:** `cargo check --offline --locked` passed after the cancellation fix.

**Pass 2:** All fourteen focused worker regressions pass. The complete Rust suite passes 538 library tests, seven hardening integration tests, one intelligence route integration test, and one documentation test: **547 passed, zero failed, one existing ignored**. Parity and adversarial guards report zero violations; the context guard passes all 1,181 scanned files, and graph integrity is `ok`. The documentation build passes with the existing VitePress/Rolldown warnings retained in its log.

Reproduction from `server-rs/`: `cargo check --offline --locked`, then `cargo test --offline --locked --lib worker_regression -- --test-threads=1` for the focused cases or `cargo test --offline --locked -- --test-threads=1` for the full suite. Provider cases require local loopback access.

Final evidence is retained under `.tmp/nexus-audit/`: `worker-tests-cargo-check-final.log`, `worker-tests-targeted-final.log`, `worker-tests-rust-full.log`, `worker-tests-parity.log`, `worker-tests-context.log`, `worker-tests-invariants.log`, `worker-tests-graph-final.log`, `worker-tests-trace-final.log`, `worker-tests-impact.json`, and `worker-tests-docs-build.log`. The initial sandbox-restricted run, fixture correction, and intermediate compilation failures are diagnostic history and are not final passing results.

<!-- Metadata: [NEXUS_BACKGROUND_WORKER_TESTS_2026-10-11] -->
