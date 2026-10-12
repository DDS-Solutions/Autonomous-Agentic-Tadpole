> [!IMPORTANT]
> **@docs ARCHITECTURE:Core**
> **Failure Path**: Treating scoped regression success as proof of production durability or every repository feature.
> **Telemetry Link**: Search `[NEXUS_FIRST_PRINCIPLES_CLOSURE_2026-10-11]` in correction evidence.
>
> ### AI Assist Note
> Records corrections, first-principles grades, verification, and focused decomposition for F1–F11.
> ### 🔍 Debugging & Observability
> Current evidence is retained under `.tmp/nexus-fixes/`; the original reproductions remain under `.tmp/nexus-review/`.

# Section 1: Executive Summary & Overview (The Nexus Architect)

**All eleven scoped findings are corrected and covered by passing regressions. Overall scoped grade: B+.** This includes the six findings from the [first-principles review](./NEXUS_FIRST_PRINCIPLES_REVIEW_2026-10-11.md) and five found during continuation. The changes restore accounting, execution ownership, cancellation recovery, frontend authentication/history, multi-agent memory/recovery isolation, concurrent writer progress, and Rust graph references. No new dependency, migration, or public HTTP schema was introduced for these corrections.

This assessment covers the current uncommitted working tree and the traced boundaries in the original review. Existing audit changes were retained. It does not certify every repository file or claim production-provider, desktop, sustained-load, or optional native-feature verification. Grades measure contracts maintained under failure, cancellation, and concurrency; test counts are supporting evidence rather than the grading formula.

| Area | Review grade | Current scoped grade | Reason |
|---|---|---|---|
| Architectural integrity | B− | A− | Domain persistence owns shared updates; catalog/recovery/settlement responsibilities are separated. Event executors are bounded. Remaining decomposition and SQLite writer limits are explicit. |
| Reliability and correctness | C | A− | Eleven defects have executable protection, including partial paid cancellation, preemption, temporary storage recovery, writer deadlock, registry lock lifetime, and orphan recovery isolation. Abrupt process termination remains outside the verified contract. |
| Security posture | B− | B+ | Quota settlement maintains admission accounting and frontend actions supply the configured credential. This pass makes no fresh advisory-cleanliness claim. |
| Testing rigor | B | A− | Full Rust, frontend, Python, and Chromium suites pass, with targeted failure injection and real local provider fixtures. Native features and production integrations remain unverified. |

An A across the entire codebase is not established by this scoped correction. Six P1 and five P2 scoped findings are closed; dependency advisories, recommendations, and operational verification limits remain explicit.

The continuation found SQLite read-to-write upgrade deadlocks (F7), omitted qualified/generic Rust graph calls (F8), lost acknowledged usage and run identity during cancellation (F9), and a registry shard lock retained across database awaits (F10). Initial reproductions and failed intermediate fixtures are retained as evidence. The original public-API cancellation probe now records the acknowledged $0.00002 consistently in mission history, scheduled-run history, and pending quota charges; its earlier output recorded zero in all three.

The final adjacent check found per-agent write errors stopping the stale-recovery sweep (F11, P2). Recovery now continues other candidates, returns the first error after the sweep, and owns commit/publication through caller cancellation. The two-agent regression and the final full Rust suite pass: 571 tests, with one existing ignored test.

# Section 2: Pillar I — Architectural Integrity (The Master Architect)

The corrections use the existing Rust lifecycle locks, SQLx transactions, strict versioned writes, and terminal module. `create_mission_in_tx` allows initialization to compose mission insertion, activation, and agent assignment in one transaction. Stale recovery uses the same terminal transaction under the already-held lifecycle lock. Both scheduler and orchestrator call that recovery policy; the orchestrator's unversioned registry reconciliation was removed.

The three layers retain distinct roles: documentation/directives state the contracts, Rust orchestration owns mission and task lifecycles, and provider/adaptor/Python execution performs work behind those gates. Parity and context checks verify their declared links. The changes improve single-responsibility boundaries and dependency direction using concrete functions/modules rather than a new service or repository framework. Tracked task spawning still lives in the HTTP agent module; moving it into the agent domain is a remaining focused refactor. SQLite still serializes writers, 64 lifecycle stripes can briefly serialize unrelated identities, and bounded event execution needs sustained-load evidence before a capacity claim.

The runner owns the initialized mission before fallible journal/context/provider work. A single guarded execution helper covers normal inference and deterministic workflows. Workflow steps retain the generated mission identity and aggregate all reported usage, including Sentinel re-prompts, before finalization. TypeScript's small `sovereign_api_service.ts` reuses the existing request layer. Zustand publishes history and its derived ID index in one update. Cognitive consolidation has a per-agent helper and a sweep that continues after local errors.

The continuation separates provider/model catalog storage into `server-rs/src/agent/persistence/catalog.rs`, lifecycle claim/recovery into `server-rs/src/agent/persistence/recovery.rs`, and persistence tests into `server-rs/src/agent/persistence/tests.rs`. Quota settlement moves into `server-rs/src/security/metering/settlement.rs`. The shared versioned agent update helper now belongs to persistence; domain recovery and scheduling no longer depend on the HTTP route implementation. Public operations retain their existing interfaces. Read-before-write transactions acquire SQLite writer intent before reading; no optimistic-concurrency retry or global application mutex was added.

Server-owned mission receipts share acknowledged usage across context clones and retain the original scheduled-run/task identity. Initialization and ordinary terminal persistence own their short commit/publication boundaries independently of caller cancellation. New mission initialization waits for the stopped execution, then settles its receipt before replacing mission ownership. Failed cleanup retains the receipt; recovery can reconcile an already-failed scheduled run for that same mission. Committed terminal persistence or agent deletion removes retained contexts. These receipts remain volatile across process termination.

The directed Force Graph handlers in `server-rs/src/routes/intelligence.rs` were inspected. Both graph representations were rebuilt against the corrected compiled library:

- Complete symbol graph: **3,756 nodes, 73,514 edges, 716 parsed files**.
- Persistent graph: **5,713 nodes, 171,314 edges**; SQLite `quick_check` returned **`ok`**.
- Reverse traversal estimates 429 affected files for metering, 421 for persistence/recovery, and 428 for the runner entry module. Full results are in `.tmp/nexus-fixes/review-impact.json`.

Graph resolution includes ambiguous names and incomplete cross-language links. Qualified/generic call capture expands the graph substantially, while global name matching still overestimates some edges. Candidate-test labels are heuristic and can omit inline regressions. Manual tracing and full suites supplement graph-directed checks. Graph risk records are not verified vulnerabilities.

| Priority | File | Focused decomposition status |
|---|---|---|
| High | [persistence.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/persistence.rs) | Catalog, recovery, and tests extracted; versioned row writes and relational deletion remain together. Consider a deletion module if that responsibility grows. |
| High | [metering.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/security/metering.rs) | Settlement extracted while preserving the same shared lock and buffers. Admission/reset and quota configuration remain in the parent. |
| High | [routes/agent.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/routes/agent.rs) | Move tracked execution spawning and cancellation orchestration into an agent domain module, preserving the registration barrier and current functions. Keep HTTP extraction/response handling in routes. |
| Medium | [runner/mod.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/mod.rs) | Move lifecycle-focused tests into a dedicated test module and keep the outer ownership boundary readable. Existing context/workflow/terminal modules already provide useful separation. |
| Medium | [event_bus.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/system/event_bus.rs) | Filtering is separate from bounded tracked execution. A durable delivery/replay subsystem would require an explicit product contract and load evidence. |
| Medium | [state/mod.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/state/mod.rs) | Separate runtime assembly, mock construction, and state/event operations. The repeated hub constructors make adding server-owned state easy to miss. Keep AppState as the existing facade. |
| Medium | [graph_store.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/intelligence/graph_store.rs) | Separate language extraction/reference resolution from graph refresh orchestration and tests. The database module already provides a useful storage boundary. |

Event monitoring now tracks at most 16 concurrent complete job executors and retains the current event's unmatched execution slots as one pending batch. A blocked provider no longer blocks every later event while capacity remains. Stopping the monitor lets admitted executors finish ordinary run-record cleanup. With a broadcast capacity of 100, saturation can still drop events; lag is logged and processing resumes. Per-event fan-out is still materialized as a batch. The bounded concurrency, later-event progress, lag-recovery, duplicate-mapping, and disabled-job tests do not establish lossless delivery or sustained-load capacity. No new service framework or dependency was added.

# Section 3: Pillar II — Reliability & Robustness (The QA Principal)

| Finding | Implemented behavior | Regression evidence |
|---|---|---|
| F1 — P1, lost quota charges | Agent and mission charges remain buffered until their shared transaction commits. Admission is excluded during settlement; concurrent increments remain pending. Failed transactions preserve both buffers. An owned Tokio worker completes settlement if its caller is cancelled, and affected-row checks detect missing quota rows. | Failure on either quota table, retry without duplication, caller cancellation, concurrent recording, blocked admission, competing flushes, missing quota creation, and invalid numeric costs. |
| F2 — P1, live agent reclaimed | Recovery acquires lifecycle ownership, preserves live tracked tasks, rechecks heartbeat/state, and publishes strictly versioned committed state. Orphan missions use shared terminal persistence; matching finished handles are removed conditionally. | A live stale runner cannot be reaped or reclaimed; after termination recovery increments the version and permits admission. Injected orphan recovery failure rolls back; stale snapshots conflict. |
| F3 — P1, initialized mission left active | Initialization is transactional. Cleanup context is established before the next fallible operation. Setup errors await terminal failure persistence. Cleanup is disarmed after persisted finalization; terminal-write failure preserves mission identity for recovery and reaches the caller. | Invalid workspace finalizes failure without provider calls; initialization-write failure leaves no ghost mission; terminal-write failure retains ownership and later recovers; a real two-step workflow completes the generated mission and accounts all four provider responses. |
| F5 — P2, stale history index | Successful history replacement publishes messages and their new index together. Late updates for absent IDs cannot overwrite another message; failed history/revert responses preserve displayed state and selection. | History replacement, late updates, append, deduplication, failure notification, and acknowledged reversion followed by refresh. |
| F6 — P2, sweep aborted by one agent | Per-agent errors are logged and other agents continue. The sweep returns its first error after processing the remaining agents, preserving observability and existing single-agent failure behavior. | A suspended or provider-failing first agent preserves its memories; a healthy later agent still consolidates. Existing quota, empty-output, source-conflict, and provider-failure tests also pass. |
| F7 — P1, SQLite upgrade deadlock | Read-before-write creation/update/deletion/batch/initialization wrappers use `BEGIN IMMEDIATE`; strict expected-version updates remain unchanged. | Four-connection fixture: eight parallel writers complete, exactly one same-version update succeeds, concurrent deletion preserves seed rows and foreign-key integrity. Original capacity probe reproduced SQLite code 6. |
| F8 — P2, graph misses Rust calls | Rust references include scoped/associated and generic function/method calls. | Exact cross-module graph edge plus byte-accurate Unicode spans, one capture per call, and exclusion of strings/comments. |
| F9 — P1, cancellation loses acknowledged charges | Shared receipt records successful provider responses before the next await; cleanup uses the owning server-created run identity. Owned initialization/terminal boundaries survive caller cancellation, and preemption settles the stopped owner first. | Partial-turn cancellation, completed-step cancellation, preemption, temporary cleanup failure/recovery, and cancellation during initialization/terminal wait pass. Mission, scheduled run, agent economics, and pending quota charge agree; no retained receipt remains after successful cleanup. |
| F10 — P1, registry shard deadlock | Context resolution clones the agent snapshot before database awaits, releasing DashMap's read lock. | A deliberately blocked database query leaves the agent mutable; the 16-job saturation and full suite pass. The previous full run stalled on this defect and was stopped. |
| F11 — P2, orphan recovery sweep stops early | A private per-agent recovery helper isolates errors; the owned sweep logs failures, processes remaining candidates, and returns the first error afterward. | A rejecting trigger on the first orphan preserves its transaction/ownership while the later orphan becomes idle with a failed mission. Removing the trigger recovers the first once without rewriting the already-recovered agent. |

Cancellation safety here means cancellation of the caller while the Tokio runtime remains alive. Quota buffers and mission receipts are volatile across process termination. A live runner is preserved by stale recovery; provider/tool deadlines and explicit cancellation remain responsible for ending live executions. Persistent database unavailability prevents durable completion, so ownership and received usage remain retained and errors observable. A response not yet acknowledged by the caller cannot be accounted from that receipt. Process termination and replay/reconciliation of externally charged requests require a durable usage ledger and integration evidence; they are not established by these changes.

The scoped OCC check in `execution/nexus_adversarial_guard.py` now evaluates function bodies independently. Its earlier whole-file expression incorrectly combined a conflict handler, version assignment, and test retry from unrelated functions. Three tests confirm detection within real same-function retries, including indented methods, while rejecting cross-function combinations. The guard remains a source-pattern heuristic, not a semantic proof.

# Section 4: Pillar III — Security Posture (The Chief Auditor)

**F4 — P2 is closed.** History, leaves, revert, and artifact promotion use the existing authenticated request layer with the configured in-memory token and backend URL. Path segments are encoded. Malformed successful responses and HTTP errors propagate to store/UI notification handling. The UI catches those reported errors without producing unhandled promises or reporting promotion success. No credential persistence was added.

Suspension and quota decisions continue to inspect server-owned agent/security state; privileged route identity comes from middleware-created `AuthenticatedOperator`. No new client-body role or authorization claim was introduced. Quota cost recording/admission rejects non-finite and negative values. SQL values remain bound parameters; test-trigger identifiers are fixed fixture literals. Source memory replacement retains identity/text comparisons inside its transaction.

All provider fixtures bind to loopback and assert `User-Agent: TadpoleOS/1.1.58`; Chromium uses the same identity. Python analysis uses fixed local paths and isolated workspace temporary files. Initial sandbox runs exposed denied temporary-folder/socket access; only identified audit processes were stopped, and local tests were rerun with the necessary access. No external provider or advisory service was contacted in this correction pass.

The prior dependency audit's residual items remain in the [earlier closure report](./NEXUS_FINDING_CLOSURE_2026-10-11.md); this pass does not reclassify them as fixed or suppress advisories. Production credentials, remote providers, native audio/vector integrations, and sustained malicious-load behavior were not exercised. Mission accounting metadata comes from tracking handles and initialized server contexts, not request-body claims. Event debug logging records the event kind without exposing arbitrary webhook payloads.

# Section 5: Pillar IV — Testing Rigor (The Testing Expert)

**Pass 1 — Synthesis:** The corrected Rust code passes `cargo check --offline --locked`; the repository's TypeScript 7 compiler and production frontend build pass. Targeted ESLint passes for modified/new frontend files. The build retains existing chunk/dynamic-import warnings, which are maintenance notices rather than failed compilation.

**Pass 2 — Adversarial verification:** Twenty-four Rust, twelve frontend, and three Python regression cases were added for these findings, bounded event dispatch, and the guard correction. The complete current results are:

| Verification | Result | Evidence under `.tmp/nexus-fixes/` |
|---|---|---|
| Rust library/integration/documentation tests | **571 passed, 1 existing ignored** | `rust-recovery-isolation.log` |
| Frontend tests | **864 passed in 127 files** | `frontend-full.log` |
| Python unit tests | **69 passed** | `python-complete.log` |
| Chromium production-build smoke tests | **19 passed** | `e2e.log` |
| Rust and TypeScript compilation; frontend build | Passed | `cargo-check-recovery-isolation.log`, `typecheck-final.log`, `build.log` |
| Frontend lint | Passed | `frontend-lint.log` |
| Route/documentation parity | Passed, 138 routes | `parity-recovery-isolation.log` |
| AI context metadata | Passed, 1,192 files | `context-closure.log` |
| Adversarial source guard | Passed, zero matches | `adversarial-recovery-isolation.log` |
| Original cancellation reproduction | Passed: exact acknowledged cost retained across mission/run/quota | `cancellation-before.log`, `cancellation-after.log` |
| Final codebase graphs and integrity | Complete build, `quick_check = ok` | `graph-final.log`, `graph-build.json`, `impact-final.log` |
| Repository invariant suite | **44 passed in 11 files**; included in frontend coverage | `invariants-complete.log` |

The ignored Rust test is the existing Drive G SSCP smoke test. Chromium checks page rendering and the health contract; it does not perform production-provider missions. Full suite evidence supplements the targeted worker tests previously requested for scheduler, event monitoring, and cognitive memory.

The most critical function remains `AgentRunner::run`. Its three-path plan and current coverage are:

| Path | Scenario and observable assertions | Status |
|---|---|---|
| Happy | Controlled provider completes a scheduled mission; run/mission identity, terminal status, released agent/handle, and exact cost are checked. Two-step workflow checks generated mission identity and aggregate tokens/cost. | Passing scheduler, runner, and workflow regressions. |
| Failure: validation | Invalid workspace after initialization fails the real mission; agent becomes idle and active count returns to zero; no provider is contacted. | Passing new regression. |
| Failure: persistence | Reject initialization agent write: no partial mission. Reject terminal mission write: returned error, retained ownership, later recovery succeeds. Reject quota writes: both charges retained and retry settles once. Reject one orphan's write: other orphan recovers before the error returns. | Passing trigger-injection regressions. |
| Failure: cancellation | Cancel a partial turn, a workflow after a paid step, initialization waiting for SQL, or a terminal caller. Preempt a paid workflow. Reject cleanup then recover. Check owning run/mission IDs, exact costs/tokens, released state, preserved replacement, and one quota charge. | Passing six new cancellation/ownership cases and existing scheduler/gateway/settlement regressions. |
| Edge | Competing claims/flushes/writers, same-version update conflict, stale heartbeat with a live task, orphan recovery, blocked SQL with a writable registry, 17 triggered jobs with 16 slots, invalid costs, absent credentials, malformed history, empty output, and denied agent before a healthy agent. | Passing regression suites. |
| Extended evidence | Abrupt process termination/restart, permanently unavailable terminal storage, multi-process load, native providers, desktop integration, and sustained trigger overflow. | Not established by these local suites. |

The four mandatory invariant gates were addressed explicitly:

1. **Scheduler:** The gateway's lifecycle/registration barrier and task-ID comparisons remain enforced. Settlement, stale recovery, and short initialization/terminal workers own their operations without registering independent agent executions. Initialized receipts preserve the original tracked task identity. An unclaimed initialization result drops its recovery guard. Provider work remains cancellable under the registered execution. The stopped receipt signals completion without reacquiring the lifecycle lock, preventing preemption wait deadlock.
2. **Relational:** No schema or deletion-order change was introduced. Mission children and self-referencing nodes were reviewed; existing child-first deletion, ledger rejection, and `PRAGMA foreign_keys = ON` tests pass. New initialization/recovery tests exercise atomic transaction rollback.
3. **Concurrency:** Recovery uses strict versioned writes and returns conflicts. No retry adopts a newer version to overwrite another writer. `BEGIN IMMEDIATE` avoids SQLite upgrade deadlock without weakening OCC. Registry snapshots release shard locks before SQL awaits. Competing flushes and duplicate cleanup settle once; token accumulation saturates instead of wrapping.
4. **Trust boundaries:** Checks use authenticated extensions and authoritative registry/quota state. Frontend credentials come from configured memory-only settings. Suspension/quota-denied memory tests make zero provider calls.

<!-- Metadata: [NEXUS_FIRST_PRINCIPLES_CLOSURE_2026-10-11] -->
