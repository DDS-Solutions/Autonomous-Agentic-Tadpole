# Section 1: Executive Summary & Overview (The Nexus Architect)

**Follow-up status:** [The finding-closure report](./NEXUS_FINDING_CLOSURE_2026-10-11.md) records subsequent corrections and verification. Remaining findings below describe the initial review state.

The checked configurations compile, the regression suites pass, and this review corrected confirmed failures in graph analysis, asynchronous graph presentation, agent deletion, knowledge pagination, and the test harness. The repository still has operational and architectural limits; passing tests and a static graph do not certify every function, external integration, or high-load scenario.

> ### AI Assist Note
> **Knowledge Heritage**
> @docs ARCHITECTURE:Core
> Review date: 2026-10-11. Workspace: `G:\Autonomous-Agentic-Tadpole`.
> ### 🔍 Debugging & Observability
> **Failure Path**: Mistaking heuristic graph edges, smoke checks, or scanner output for complete runtime verification.
> **Telemetry Link**: Search `[nexus_codebase_review]` in this report and local audit evidence.

**Scope and evidence.** The tracked inventory contains 1,445 files, including 847 source files and 162,753 source lines after corrections. Every source file has an inventory entry with its path, line count, SHA-256, and graph index count. All tracked Python sources parse. The persisted review graph indexes 846 of those source files; `src/index.css` has no supported symbol extraction. This is inventory/index coverage, not a statement that every source line was manually reviewed or executed.

The runtime Rust/TypeScript symbol graph rebuilt successfully with 3,643 nodes, 17,441 edges, and 706 parsed files. The separate persisted review graph contains 5,507 nodes, 45,998 edges, six communities, and 250 stored flows. It also has 4,553 risk-index entries; these are scoring records, not 4,553 confirmed defects. SQLite `quick_check` passed. The original operator graph database was read, while audit rebuilds were written to a separate database under `.tmp/nexus-audit`.

The trace inventory maps all 138 discovered HTTP routes to candidate handler symbols and their transitive static dependencies, and records reverse dependencies for each changed source file. The browser suite visits all 18 registered application pages plus the landing page. Names, imports, and references can produce ambiguous graph edges; dynamic dispatch, JSX callbacks, and external processes can omit edges. The graph's unused-symbol and test-relationship labels are hypotheses. No code was deleted solely because a graph label called it unused.

**Verified results:** 852 frontend unit tests; 520 server Rust tests, including integration and documentation tests, with one existing ignored test; 55 Python unit tests; four tests across the two WASM codec crates; and 19 Chromium smoke checks passed. Production build, lint, default-feature server compilation, native-shell compilation, route parity, AI context alignment, and the adversarial invariant guard passed. The local security scanner returned zero findings in 3,165 scanned files.

**Practical limits.** Live paid model/provider calls, production deployment/restore, optional `vector-memory` and `neural-audio` builds, desktop UI operation, sustained load, and full feature workflows against a running authenticated backend were not exercised. Browser page checks validate the built UI and its error boundaries; they do not prove every business operation. The optional health assertion runs only when a backend responds successfully.

Evidence is retained locally in `.tmp/nexus-audit/trace-inventory.json`, `.tmp/nexus-audit/trace-summary.json`, `.tmp/nexus-audit/graph-build.json`, `.tmp/nexus-audit/rebuilt-graph.db`, and the logs listed in Section 5. These are ignored local artifacts; the report is the reviewable repository document.

# Section 2: Pillar I — Architectural Integrity (The Master Architect)

**Architecture.** The directives/orchestration/execution structure is recognizable. React pages call typed API services; Axum routes delegate to Rust services and persistence; execution scripts perform local operational checks. Graph discovery, parsing, cache management, and synthesis already use separate modules. `ArcSwap` gives readers immutable graph snapshots without holding a write lock across CPU-heavy rebuilds. These boundaries are useful and should be preserved.

The graph rebuild formerly duplicated dry/live construction and released its concurrency gate when the HTTP future disappeared, although blocking work could continue. The corrected implementation uses one construction path and an owned RAII guard inside a detached Tokio worker. The guard now covers initialization, construction, publication, and persisted graph synchronization. A database-sync failure returns a warning. This removes duplication and makes the existing abstraction reliable without adding another framework.

SOLID concerns are concentrated in modules that combine unrelated reasons to change: large DTO collections, state bootstrap plus fixtures, persistence plus filesystem registries, route groups containing financial and task operations, and scripts that mix discovery with document rendering. A file's length alone is insufficient grounds for splitting it. The following candidates have distinct responsibilities that can be separated while retaining existing imports and public behavior:

| Priority | File | Current lines | Focused decomposition |
|---|---|---:|---|
| High | `server-rs/src/agent/persistence.rs` | 1,409 | Separate agent SQL/OCC, provider/model filesystem registries, and cleanup; move fixtures/tests into test modules. Preserve transactional deletion in one function. |
| High | `server-rs/src/agent/runner/synthesis.rs` | 1,243 | Separate mission setup, context construction, and completion/streaming preparation. Keep the runner sequence explicit. |
| High | `server-rs/src/routes/agentic_engine.rs` | 1,123 | Split task/hiring, subscriptions, and ledger handlers by domain; retain route registration in one place. |
| High | `src/logic/command_processor.ts` | 1,083 | Separate parsing from domain command handlers behind the current dispatcher. Avoid a dynamic plugin framework. |
| Medium | `server-rs/src/state/mod.rs` | 1,468 | Move mock builders and fixtures behind `cfg(test)` modules; separate production bootstrap wiring from the state facade. |
| Medium | `server-rs/src/agent/types.rs` | 1,400 | Group identity/configuration, economics, mission/task DTOs, and runtime state; use re-exports to preserve callers. |
| Medium | `server-rs/src/routes/oversight.rs` | 1,072 | Separate review queue access, decision persistence, and proposal resolution. |
| Medium | `server-rs/src/intelligence/graph_store.rs` | 1,052 | Separate lightweight extraction/discovery from risk, community, and flow derivation. Keep the existing database module. |
| Medium | `execution/generate_api_reference.py` | 1,066 | Separate route discovery, OpenAPI generation, and Markdown rendering; preserve the current CLI. |
| Medium | `server-rs/src/routes/templates.rs` | 1,026 | Separate remote registry/cache access from validated installation and rollback. |

These are recommendations, not decompositions performed in this patch. Combining them with correctness fixes would enlarge review scope unnecessarily.

**Scaling and structural debt.** Runtime symbol analysis and persisted code-review analysis are different graph products with different language support. Their counts should not be compared as if they were one data structure. Traversal bounds and the 250-flow snapshot cap also limit apparent completeness. Reverse impact estimates are broad: changed graph analysis reaches 355 candidate files, graph service 308, and agent persistence 75. Shared names inflate some of these estimates; the full relevant regression suites were run instead of trusting a narrow automatically selected set.

`spawn_blocking` work already underway does not stop when its awaited timeout expires. Graph size bounds limit individual work, but simultaneous query admission and CPU saturation still need a measured load test before capacity claims. Rebuild admission is now serialized through the full build/sync lifetime.

Three explicitly scaffolded paths remain: `system/actors/memory_actor.rs` returns an unimplemented error for `MemoryQuery`; `system/event_bus.rs` is described as awaiting wiring; and `agent/knowledge_store/cognitive_memory.rs` likewise awaits wiring. They should remain excluded from feature readiness claims until their intended runtime integration is specified and tested.

# Section 3: Pillar II — Reliability & Robustness (The QA Principal)

| Confirmed failure | Correction | Verification |
|---|---|---|
| Cancelling a rebuild caller released the gate while work could continue, allowing overlapping rebuilds. | The detached worker owns an `Arc`-backed RAII gate through all build stages. | A deterministic cancellation test holds initialization, cancels the caller, verifies a second rebuild conflicts, then releases initialization and observes gate cleanup. |
| Initial graph construction discarded errors and marked the subsystem ready, allowing failed analysis to look like an empty successful result. | Record failure status, return a Rust `Result` to read consumers including MCP, and allow an explicit repair rebuild to replace the failed snapshot and restore readiness. | Missing-workspace regression checks repeated failure, rejection of graph/impacted-test requests, and recovery after a successful rebuild. |
| Rebuild `file_count` counted indexed symbols; two functions in one file reported two files. | Count parsed repository files. Dry-run build-cap outcomes now report warning accurately. | One file/two symbols fixture asserts file count one and node count two. |
| Token-budget resolution omitted metadata/suffix costs and could report artificially low totals; BPE truncation could split UTF-8. | Count name, real relative path, kind, and actual signature; truncate at valid boundaries; retokenize the suffix; return no node if metadata cannot fit. | Budgets 0–220, Unicode, BPE and fallback counting, and existing context tests pass. |
| Traversal reported truncation merely upon reaching an exact cap, yet failed to report omitted nodes at the depth limit. Impacted tests used only 200 symbols. | Flag truncation when an unvisited node is omitted. Use the source graph node cap for impacted tests and return `BR_TRUNCATED` rather than an apparently complete partial test list. | Exact-cap, cycle, wildcard, bounded traversal, and 60-node depth-chain tests pass; existing impacted-test route tests pass. |
| Deletion omitted economics metadata and failed against mission-linked oversight foreign keys. | Perform cleanup in a transaction, delete economics children first, and detach optional oversight mission links while preserving oversight records. | Full migrated-schema deletion test with foreign keys enabled and a clean `foreign_key_check`. |
| Financial ledger foreign keys made deletion fail generically; the route had already aborted the runner before the failure. | Reject either ledger counterparty with an explicit conflict; commit durable deletion before removing registry/runner state. | Ledger records and both agents survive rejection; the route test proves rejected deletion keeps the runner and successful deletion aborts it. |
| Negative SQLite limits could mean unlimited results; signed peer limits cast to `usize` could become enormous. | Clamp list/peer limits to 0–200 and offsets to at least zero; bound direct peer service calls and return early for zero. | Compilation and regression suites pass. Boundary-focused route assertions remain a useful follow-up; scanner/compile success alone is not boundary coverage. |
| Old graph-mode or selection responses could overwrite a newer view. | Abort previous requests, pass `AbortSignal`, and check cancellation before publishing state; clear data during replacement fetches. | Deferred-promise tests resolve old requests after newer requests and verify the current view survives. |
| Python graph extraction skipped `async def`. | Recognize both synchronous and asynchronous Python definitions. | Dedicated extraction test verifies both names and source lines. |
| Tests depended on a fixed operator cache path, external DNS, and a development-server startup/load path. | Isolate SSCP cache paths with temporary UUID directories; use a bounded local refused connection; validate the production frontend bundle. | Rust and browser suites pass with local fixtures. |
| A health assertion was swallowed by an overly broad test catch; page checks could accept a loading/error shell. | Catch only health transport failure, assert successful response contracts outside the catch, and wait for all loading indicators/error headings to disappear. | All 19 browser checks pass against the built application. |

Rust ownership controls graph snapshot and gate lifetimes; `Drop` releases the gate on ordinary error or panic unwinding. React cleanup now controls request publication as well as transport cancellation, which matters when mocks or downstream promises ignore aborts. SQL parameters remain bound values; no concatenated client SQL was introduced.

**Remaining failure paths.** SQL and the live DashMap registry are separate consistency domains: simultaneous dispatch, deletion, and completion still need an explicit lifecycle policy and concurrency testing. The deletion correction prevents aborting a runner on a rejected delete; it does not claim cross-store serializability. Some dispatch recovery/status updates ignore persistence errors. These should be addressed as focused lifecycle work, with deterministic failure injection, rather than by adding a general transaction abstraction across every service.

**Mandatory dual pass.** Pass 1 completed default-feature `cargo check`, frontend compilation, and type/lint checks. Pass 2 completed adversarial regression tests and both Python invariant/parity guards. The four gates were also inspected against the changed paths:

1. **Scheduler:** runner registration already uses a oneshot start barrier and task-ID-aware eviction. Rebuild gate ownership transfers into the worker before any awaited construction; caller cancellation cannot release it early. This review does not prove every spawned task elsewhere uses this pattern.
2. **Relational:** migrations were searched for agent and mission foreign keys. Cleanup preserves oversight and ledger evidence, removes economics children, and passes migrated-schema foreign-key tests. Ledger-linked agents deliberately cannot be deleted.
3. **Concurrency/OCC:** agent writes retain the conditional version comparison and conflict result; this patch introduces no retry that reloads a newer version and silently reapplies a stale full object. Existing stale-version tests pass. Lifecycle races between registry and SQL remain separate from this OCC gate.
4. **Trust:** graph and agent routes remain behind token validation. Client body fields do not establish authentication. The existing suspension role header is not a cryptographically bound principal claim; its limitations are described in Section 4.

No security floor constants were lowered. Traversal bounds import the existing named source constant. Existing invariant tests ran with the frontend suite; exact vocabulary pins and configuration-backed invariants were not replaced with duplicated constants.

# Section 4: Pillar III — Security Posture (The Chief Auditor)

The review confirmed token middleware protects the directed Force Graph APIs, and existing traversal/path-boundary integration tests pass. Query bounds now prevent negative-limit unlimited reads and signed-to-unsigned peer limit expansion. Incomplete impacted-test traversal fails explicitly rather than supporting a false confidence claim about the safe test subset. Financial and oversight records remain intact through rejected or successful agent cleanup as appropriate.

The existing HTTP authentication model validates a shared operator `NEURAL_TOKEN`, including rotation behavior. It does not provide separately authenticated users/roles. `send_task` checks the client `x-tadpole-role` header for suspension override after token validation. A token holder can choose that header. This is unsuitable as a lower-privilege versus administrator boundary: before adding multi-user or delegated tokens, role checks must consume server-verified identity/claims. The current shared-operator model and the client preference `auto_resume` must not be represented as tenant RBAC.

The security scanner found zero secret/pattern findings. This is a heuristic local result, not a dependency vulnerability assessment or proof that all runtime logs redact provider credentials. Secret suppression and unknown-role fail-closed tests passed in the Rust hardening suite. No live credentials or provider responses were printed for this report.

Two analysis-tool limitations were confirmed in source inspection: `.agent/skills/vulnerability-scanner/scripts/dependency_analyzer.py` invokes Cargo audit but leaves its JSON parsing unimplemented, and `.agent/skills/lint-and-validate/scripts/type_coverage.py` samples only the first 30 TypeScript files. Neither is evidence for full Rust dependency safety or full TypeScript annotation coverage. Supply-chain release decisions need parsed advisory results with explicit unavailable/error states; annotation counts should be reported as a sample or replaced by compiler-backed measurement. These tooling gaps were documented rather than treated as successful full-codebase checks.

Analysis tools were inspected and invoked with fixed local arguments. The audit driver used library APIs and a separate local SQLite database without launching the operational engine. Python inventory subprocesses pass argument lists to Git, with no interpolated user text or `shell=True`. Browser preview binds loopback. Audit-created HTTP clients and the final browser configuration identify as `TadpoleOS/1.1.58`; the project-level browser override preserves that identity despite the device preset. Local socket/temp-file tests required approved execution outside the restrictive test sandbox; the initial permission failures were not treated as product defects.

External provider authorization, SSRF protections on all configurable connectors, runtime secret-log behavior under failure, and multi-process database contention require dedicated operational/security validation. No release approval or enterprise security certification is implied by these local checks.

# Section 5: Pillar IV — Testing Rigor (The Testing Expert)

**Most critical logic: graph rebuild and bounded impact/context analysis.** The following three-path plan covers the implemented regressions and identifies remaining test gaps explicitly:

| Path | Inputs and execution | Expected result / evidence |
|---|---|---|
| Happy | Authorized graph fetch; dry-run fixture with two symbols in one file; ordinary context resolution; wildcard impact across callers/tests. | Valid graph DTOs, one file/two nodes, honest budget totals, and reachable test paths. Existing route/engine tests plus new file-count test pass. |
| Happy | Successful live rebuild and persisted sync. | New immutable snapshot and synchronized database, followed by gate release. Both graph representations rebuilt locally; a full HTTP live-rebuild integration test remains a follow-up. |
| Failure | Missing/invalid bearer token, traversal path, malformed query values. | Auth/extractor rejection or forbidden path; existing middleware and intelligence route integration tests pass. |
| Failure | Cancel caller while initialization is blocked; request second rebuild. | First worker retains gate, second call conflicts, releasing the fixture permits cleanup. New deterministic test passes. |
| Failure | Construction failure, worker panic, or SQLite synchronization failure. | Gate releases on worker exit; no failed-build snapshot publication; synchronization failure reports warning. Branches were reviewed; targeted injected-failure tests remain to be added. |
| Failure | Missing workspace during initial construction, followed by workspace repair and an explicit rebuild. | Health remains failed and graph/impacted-test requests error; a successful repair restores readiness and graph requests. Dedicated regression test added. |
| Failure | Omit reachable nodes due to depth/node bound when selecting impacted tests. | Engine marks partial traversal; service returns `BR_TRUNCATED` advising a full suite. Depth/cap engine tests pass; dedicated service-level partial-result assertion remains a follow-up. |
| Edge | Budget zero, smaller than metadata, Unicode text, BPE/fallback counting, 0–220 budget sweep. | No panic, valid UTF-8, reported tokens equal returned content counts and stay within budget. New sweep passes. |
| Edge | Empty graph, no matching symbol, cycles, exact node cap, depth-50 frontier, wildcard file with several starting symbols. | Terminates deterministically; no false exact-cap truncation; omitted depth frontier is flagged. Existing and new graph tests pass. |
| Edge | Workspace discovery/file/node caps; many simultaneous queries with a saturated blocking pool. | Partial analysis is surfaced, retained snapshots stay usable, memory and request latency remain within measured limits. Cap behavior is covered in engine tests; sustained concurrency/capacity measurement remains planned. |
| Edge | Rapid mode/selection changes; resolve old response last; close/unmount with outstanding work. | Old results cannot republish into the active view. New deferred-response tests cover mode and selection races; cleanup is exercised by the component suite. |

**Deletion integrity suite.** Happy path deletes an agent with economics metadata and completed mission while preserving oversight evidence. Failure path attempts deletion of each financial counterparty and verifies both agents, ledger, and active runner survive. Edge path deletes an absent ID and checks `PRAGMA foreign_key_check`. These migrated-schema and route tests pass. Concurrent dispatch versus successful deletion remains a separate stress-test requirement.

| Check | Final result | Local evidence |
|---|---|---|
| Server `cargo check --offline` | Passed | `.tmp/nexus-audit/cargo-check-final.log` |
| Server Rust unit/integration/doc tests | 520 passed; one ignored | `.tmp/nexus-audit/rust-tests-complete.log` |
| Frontend unit/invariant tests | 852 passed in 125 files | `.tmp/nexus-audit/vitest-verified.log` |
| Frontend production build | Passed | `.tmp/nexus-audit/e2e-final.log` |
| Frontend lint | Passed | `.tmp/nexus-audit/lint-verified.log` |
| Chromium built-page checks | 19 passed | `.tmp/nexus-audit/e2e-verified.log` |
| Python unit tests | 55 passed | `.tmp/nexus-audit/python-tests-after.log` |
| Tracked Python AST parsing | No syntax errors | `.tmp/nexus-audit/inventory.json` |
| WASM codec crates | Two tests passed in each crate | `.tmp/nexus-audit/wasm-current.log`, `wasm-legacy.log` |
| Native shell `cargo check --offline` | Passed | `.tmp/nexus-audit/tauri-check.log` |
| Route parity | 138 routes; zero drift | `.tmp/nexus-audit/parity-verified.log` |
| Adversarial invariant guard | Zero reported violations | `.tmp/nexus-audit/nexus-verified.log` |
| AI context alignment | Passed | `.tmp/nexus-audit/context-verified.json` |
| Local security pattern scan | Zero findings / 3,165 files | `.tmp/nexus-audit/security-verified.json` |
| Graph database integrity | `quick_check`: ok | `.tmp/nexus-audit/trace-inventory.json` |

These results substantiate the corrections and checked configurations. They do not replace sustained load, optional-feature build matrices, measured test coverage, or authenticated end-to-end business workflow tests.

<!-- Metadata: [nexus_codebase_review] -->
