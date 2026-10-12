> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**
> **@docs ARCHITECTURE:Core**
> **Failure Path**: Treating unverified checks or architecture recommendations as closed defects.
> **Telemetry Link**: Search `[NEXUS_FINDING_CLOSURE_2026-10-11]` in audit evidence.
>
> ### AI Assist Note
> Follow-up to the graph-based review; records corrections, remaining verification, and decomposition scope.
> ### 🔍 Debugging & Observability
> Evidence: `.tmp/nexus-audit/`; context and parity guards validate documentation alignment.

# Section 1: Executive Summary & Overview (The Nexus Architect)

The follow-up corrects and locally verifies the documented lifecycle, authorization, audit-tool, and supported scaffold findings. The final Rust rerun passes after dependency cleanup. The approved live npm audit reports zero vulnerabilities after a targeted parser update. Five Rust lockfile advisories/warnings remain visible, and native feature builds and operational capacity/provider checks remain open. This report does not certify every runtime branch or deployment environment.

The initial report is `docs/audits/NEXUS_CODEBASE_REVIEW_2026-10-11.md`. Its remaining-findings paragraphs describe the earlier patch; this closure report records the subsequent work.

The later [background worker test follow-up](./NEXUS_BACKGROUND_WORKER_TESTS_2026-10-11.md) records fourteen additional regressions and a scheduler cancellation cleanup correction. Its verification results supersede the Rust counts in this earlier closure snapshot.

| Finding | Correction | Status |
|---|---|---|
| Delayed saves could recreate deleted agents. | Separate explicit creation from update-only persistence; creation cannot replace duplicates. | Closed; final Rust regression suite passed. |
| Lifecycle transitions could race across registry, runner tracking, and SQL. | Bounded per-agent lock stripes cover gateway transitions, scheduled admission, recruitment, initialization, working memory, and terminal publication. | Closed; final Rust regression suite passed. |
| Batch version adoption converted stale state into a later overwrite; versions were published before commit. | Return batch errors, roll back the entire batch, and publish only committed versions. No blind version refresh/retry. | Closed; final Rust regression suite passed. |
| Old completion could clear a new mission or apply cost twice. | Verify mission ownership; commit agent, mission, logs, task usage, and scheduled usage transactionally; reject duplicate completion. | Closed; final Rust regression suite passed. |
| Dispatch/recovery and recruitment discarded persistence failures. | Propagate failures, persist before publishing, preserve rejected-deletion runners, and use resolved specialist IDs. | Closed; final Rust regression suite passed. |
| Scheduled runners bypassed gateway tracking and reported placeholder cost. | Share the start barrier/tracking helper, release failed admission, propagate run-record failures, and reconcile usage with the exact server-created scheduled run. | Closed; final Rust regression suite passed. |
| Suspension privilege depended on a self-claimed role header. | Dispatch consumes a middleware-created authenticated operator extension. | Closed; final Rust regression suite passed. |
| MemoryQuery was unimplemented; disabled vectors could discard successful saves. | Durable SQLite text save/search in both builds; bounded queries and actor-response deadlines; supervise actors during engine startup/shutdown. | Closed; final Rust regression suite passed. |
| Event and cognitive workers lacked startup integration. | Explicit disabled-by-default startup switches; retained worker handles for shutdown; handle broadcast lag; bounded transactional consolidation and reported-usage metering. | Closed; final Rust regression suite passed. |
| Rust audit output was ignored; annotation scans sampled 30 files and double-counted Python hints. | Parse vulnerabilities and soundness/maintenance warnings, surface unavailable/error states, scan all discovered sources, and share a canonical annotation inventory. | Eleven new reporting tests passed; live dependency findings explicitly retained. |
| Duplicate security scanners produced unsupported lockfile/configuration claims and false alarms. | Share local detection and dependency reporting; validate paths; retain unreadable/unsupported checks as unverified. | Scanner regression and CLI checks passed. |
| npm selector parser had two moderate dependency alerts; an unused Rust dependency carried two maintenance alerts. | Scope the patched parser override to typography; remove unused `backoff` and its `instant` dependency. | Closed; npm audit/build/browser checks and Rust cleanup rerun passed. |
| Boundary/partial-impact/live-sync checks lacked direct regressions. | Add signed query boundaries, incomplete-impact rejection, and live HTTP rebuild with injected SQLite-sync failure. | Closed; final Rust regression suite passed. |

# Section 2: Pillar I — Architectural Integrity (The Master Architect)

The directives/orchestration/execution structure is retained. Rust ownership and short-lived async locks manage transition lifetimes; provider inference runs outside lifecycle locks. Fixed stripes bound synchronization memory even with arbitrary IDs, and batch saves acquire stripes consistently before taking snapshots and opening a transaction. SQLite remains the persistent authority; this is an in-process coordination policy, not a claim of multi-process serializability.

Terminal persistence is decomposed into `server-rs/src/agent/runner/terminal.rs`. Success, failure, and abort paths share the same transactional ownership checks. The two annotation entry points share `execution/skills/lint-and-validate/scripts/type_coverage.py`. Duplicate security scanner entry points share the compatibility layer in `execution/skills/vulnerability-scanner/scripts/security_scan.py`, local detection in `execution/security_scan.py`, and complete dependency parsing. These changes remove duplicated behavior without introducing a generic distributed transaction framework.

The current graph inventory covers 849 source files and all 138 route handlers. The refreshed persistent graph contains 5,589 nodes, 46,861 edges, 4,620 risk records, six communities, and 250 flow snapshots. Risk records are scoring entries, not confirmed defects. CSS remains unindexed. Static name resolution, ambiguous callers, traversal limits, and the flow cap prevent these counts from proving runtime coverage. Evidence: `graph-build.json`, `trace-inventory.json`, and `trace-closure.log`. Changed-file impact includes newly added source files as well as tracked edits.

| Priority | File | Useful decomposition |
|---|---|---|
| High | `server-rs/src/agent/persistence.rs` | Separate agent row/OCC writes, connector/skill synchronization, cascade cleanup, and inline fixtures. Keep transaction ownership explicit. |
| High | `server-rs/src/agent/runner/synthesis.rs` | Separate provider context, tool-turn orchestration, completion parsing, and fixtures. |
| High | `server-rs/src/routes/agentic_engine.rs` | Separate task/receipt routes from hiring, economics, and subscription routes. |
| High | `src/logic/command_processor.ts` | Separate command parsing from domain dispatch handlers behind the existing facade. |
| Medium | `server-rs/src/state/mod.rs` | Separate production construction, mock construction, and persistence orchestration. |
| Medium | `server-rs/src/agent/types.rs` | Separate identity/configuration DTOs from mission/usage/runner state; preserve serde contracts. |
| Medium | `server-rs/src/agent/runner/oversight.rs` | Separate authorization decisions, queue coordination, and telemetry. |
| Medium | `server-rs/src/intelligence/graph_store.rs` | Separate extraction/discovery from risk, community, and flow derivation. |
| Medium | `server-rs/src/routes/templates.rs` | Separate registry access/cache from validated installation and removal. |
| Medium | `execution/generate_api_reference.py` | Separate route discovery, OpenAPI generation, and Markdown rendering. |

These are architectural recommendations, not defects being marked fixed. Large decompositions are deliberately kept separate from this correctness patch. Cross-process contention, blocking-pool saturation, external providers, native audio/vector dependencies, and desktop runtime behavior require operational evidence.

# Section 3: Pillar II — Reliability & Robustness (The QA Principal)

Creation inserts only a missing identity, starts its durable version, and synchronizes manifests/subscriptions transactionally. Updates require the existing identity and matching version. Delete commits before registry removal and runner cancellation; stale later updates cannot insert that identity. Financial ledger references still prevent deletion, and child-first cleanup preserves oversight evidence.

Gateway and scheduler admission hold an agent lifecycle guard until its tracking handle is registered. The task waits on a oneshot start barrier, and cleanup compares its task ID before evicting a handle or resetting status. Scheduled run metadata is supplied by the server while admission is locked. Workflow completion releases its owning agent's claim even if that agent did not execute a mission loop. Recruitment commits before publishing, returns the actual specialist ID, and avoids holding an outer SQL transaction while awaiting lifecycle locks.

Terminal completion checks the current mission ID before mutating state. A single transaction records the terminal mission, agent counters/status, log, task counters, and available scheduled-run usage. A failed write rolls back the mission as well as the agent. Duplicate or superseded completion cannot clear a new mission or double-charge. Budget metering uses the existing quota buffer without a detached persistence task or an OCC-clobber fallback.

Batch persistence rolls back on conflict and leaves in-memory versions unchanged. Mutable working memory persists a clone before publication and rejects an obsolete mission. Progress status respects suspension and mission identity. Actor requests bound both queue admission and response waiting; missing actors produce an error instead of acknowledging discarded memory.

Memory queries use bound SQL parameters and literal substring matching, cap results at 200, reject oversized queries, and identify the SQLite text backend explicitly. Session traversal rejects incomplete/cyclic/depth-limited ancestry. Cognitive batches are bounded, registry references are released before provider inference, and source text/identity comparisons plus summary insertion are transactional. Event listeners report lag and continue with retained events; triggered jobs are admitted sequentially.

The four mandatory adversarial gates are documented here:

1. **Scheduler:** registered executions wait for a start barrier; task-ID comparisons protect replacement handles. Optional workers retain shutdown handles. Supervised actor completion is consumed by its supervisor after handle publication. This is not a claim about every detached task elsewhere.
2. **Relational:** migrated-schema cleanup tests enable foreign keys, preserve ledger/oversight evidence, and check foreign-key consistency. Terminal state and scheduled usage are committed in one transaction.
3. **Concurrency:** no conflict branch adopts a newer version and retries a stale object. Batch rollback, superseded completion, and duplicate completion have direct regression tests.
4. **Trust:** only token validation creates the operator extension. Role headers and body identity/preferences are not authenticated role claims. Scheduled-run ownership comes from server tracking metadata.

# Section 4: Pillar III — Security Posture (The Chief Auditor)

The shared bearer token is an operator capability, including the existing rotation behavior. The patch removes the client role header from the dispatch authorization decision. It does not claim tenant RBAC or separately authenticated roles. Extending that identity model requires an explicit principal/credential design.

The dependency analyzer parses Cargo vulnerabilities, soundness warnings, and maintenance warnings and validates npm count metadata. Missing tools, invalid JSON, tool errors, disabled network audits, and unverified RustSec cache freshness cannot produce an overall pass. Unknown vulnerability/soundness severity is conservatively classified high; maintenance notices are low. Annotation results are explicitly a heuristic, not inferred-type coverage: 404 TypeScript and 134 Python files were read in the local inventory.

The maintained local secret/pattern scanner reported zero findings and zero read errors across 3,166 scanned files. Its existing test, scratch, scanner, local `.env`, and explicit-suppression exclusions still apply. The broad duplicate scanner's earlier 26 flags included detector definitions, runtime environment lookups, test fixtures, safe JSON-LD serialization, synthetic function names, nonexistent yarn/pnpm requirements, and a Next.js-specific header-file check applied to this Rust project. The compatibility entry points now reuse the maintained detector; unsupported configuration checks report unverified. This is not a supply-chain safety or complete secret-log/connector SSRF claim. Analysis subprocesses use fixed argument arrays and local working directories; SQL uses bound values. Audit HTTP identity is `TadpoleOS/1.1.58`.

**Live dependency results:** after explicit user approval for npm metadata disclosure, the npm audit found the typography plugin's parser below the patched 7.1.6 boundary. A scoped override installs 7.1.6, and the repeat audit reports zero vulnerabilities. The upstream issue concerns excessive selector parsing cost; the app and documentation builds and browser checks pass with the patched package. [PostCSS advisory](https://github.com/advisories/GHSA-rj75-hqrm-r3gf).

The separate Cargo audit fetched the public RustSec database into the audit workspace, without uploading the lockfile. Its snapshot is commit `7eebec69c352c7191b1f13eb95dd510eeca5d1de`, updated 2026-10-09. The reusable analyzer still conservatively labels its own no-fetch cache freshness unverified. The following findings remain open:

| Finding | Reachability/evidence | Status |
|---|---|---|
| `rsa` 0.9.10 — RUSTSEC-2023-0071 | Lockfile dependency of optional `sqlx-mysql`; default `cargo tree --invert rsa` has no active dependents. SQLx is configured for SQLite with default features disabled. | Upstream has no patched version; retain the alert and avoid enabling this driver. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2023-0071.html). |
| `lru` 0.12.5 — RUSTSEC-2026-0002 and RUSTSEC-2026-0253 | Optional vector stack; absent from the default active tree, which uses `lru` 0.18.5. | Requires upstream vector-dependency remediation and native build validation; keep `vector-memory` disabled. [Iterator soundness](https://rustsec.org/advisories/RUSTSEC-2026-0002.html), [panic safety](https://rustsec.org/advisories/RUSTSEC-2026-0253.html). |
| `fxhash` 0.2.1 — RUSTSEC-2025-0057 | Active transitive dependency of BM25 2.3.2. Public registry review still reports BM25 2.3.2 as latest stable. | Upstream maintenance debt; no vulnerability is asserted by this notice. |
| `paste` 1.0.15 — RUSTSEC-2024-0436 | Optional native/vector dependency; absent from the default active tree. | Upstream maintenance debt; requires dependency upgrade and feature validation. |

Removing the unused direct `backoff` dependency also removes `instant` and their two maintenance notices. LanceDB's latest stable release is 0.40.0, outside the project's 0.27 compatibility range; a blind upgrade is not represented as a verified fix. The aggregate lockfile audit remains failed with three conservatively classified high and two low findings; no advisories were suppressed.

# Section 5: Pillar IV — Testing Rigor (The Testing Expert)

**Most critical logic: lifecycle admission and terminal persistence.**

| Path | Test inputs/actions | Required outcome |
|---|---|---|
| Happy | Create durable agent, initialize a mission, complete with token usage. | Agent/mission/log/task counters commit; registry publishes the committed version; runner is released. |
| Happy | Complete a scheduled mission under its registered server-created run identity. | The exact scheduled run receives mission ID and real reported cost before completion. |
| Failure | Inject a rejecting SQLite trigger during the terminal agent write. | Agent state is unchanged; mission transition and log insertion roll back. |
| Failure | Persist a stale batch repeatedly after an external version advance. | Every attempt conflicts; no newer version is adopted; other writes in the batch roll back. |
| Failure | Delete a ledger counterparty or forge an operator role header without a token. | Conflict preserves financial evidence and runner; forged role does not authenticate. |
| Edge | Late save after deletion; duplicate create; 32 simultaneous creates of one ID. | No resurrection or overwrite; exactly one creation succeeds. |
| Edge | Old completion after mission replacement; repeated terminal completion. | Replacement state survives and usage is charged once. |
| Edge | Execution completes immediately after scheduling. | Tracking exists before execution starts and is removed after completion. |
| Edge | Negative/zero/maximum knowledge limits; changed consolidation source; duplicate consolidation. | Bounds hold; changed sources roll back; only one summary commits. |

| Verification | Current evidence |
|---|---|
| Pass 1: standard Rust compilation | Default-feature `cargo check` passed after dependency cleanup and final execution-tool changes. |
| Pass 2: Rust regression suite | Final dependency-cleanup run: 533 passed, one existing ignored. |
| Frontend tests/invariants | 852 tests across 125 files passed. |
| Python regression suite | 66 tests passed, including eleven new audit/scanner reporting cases. |
| Production build and lint | Passed. Documentation build passed after fixing six dead links; upstream VitePress/Rolldown warnings remain visible. |
| Production browser smoke | 19 tests passed. |
| Graphs, parity, context, Nexus guards | Graphs rebuilt; parity and Nexus report zero errors; context passed all 1,179 scanned files. |
| Local secret/pattern scan | Zero findings and read errors; 3,166 files scanned. |
| Optional vector/audio profiles | Offline checks stop on missing `arrow` 57.3.1 and `hmac-sha256` 1.1.15 respectively; no compile-pass claim. |
| Live dependency audits | npm zero vulnerabilities; Cargo findings retained as detailed in Section 4. |

Logs and structured evidence are retained under `.tmp/nexus-audit/`. Local tests that require temporary files or loopback sockets ran with approved local test access. Initial sandbox denials and the superseded fixture failures are not final passing results. Sustained load, multi-process contention, production providers, native feature execution, and desktop behavior remain operational validation, rather than assertions inferred from unit tests.

Final evidence files: `cargo-check-closure-verified.log`, `rust-tests-cleanup.log`, `vitest-security.log`, `python-tests-security.log`, `python-audit-dual-pass.log`, `frontend-build-security.log`, `docs-build-security.log`, `e2e-security.log`, `lint-closure.log`, `parity-closure-final.log`, `context-closure-final.log`, `nexus-closure-final.log`, `security-closure.json`, `dependencies-closure-live.json`, `npm-live-fixed.json`, `cargo-live-detail.json`, and the graph/trace artifacts above. Optional-profile failure evidence is `vector-profile-closure.log` and `audio-profile-closure.log`.

<!-- Metadata: [NEXUS_FINDING_CLOSURE_2026-10-11] -->
