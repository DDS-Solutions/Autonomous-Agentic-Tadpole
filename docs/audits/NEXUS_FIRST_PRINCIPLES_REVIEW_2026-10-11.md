> [!IMPORTANT]
> **@docs ARCHITECTURE:Core**
> **Failure Path**: Passing checks mistaken for proof of accounting, ownership, or lifecycle correctness.
> **Telemetry Link**: Search `[NEXUS_FIRST_PRINCIPLES_REVIEW_2026-10-11]` in review evidence.
>
> ### AI Assist Note
> Records a review of the current working tree, grades, open findings, and focused correction plans.
> ### 🔍 Debugging & Observability
> Fresh evidence is retained under `.tmp/nexus-review/`; earlier full-suite evidence remains under `.tmp/nexus-audit/`.

# Section 1: Executive Summary & Overview (The Nexus Architect)

**Review-time overall grade: C+. Six findings were open: three P1 and three P2.** All six subsequently received corrections and passing regressions. Five further findings from continuation were also corrected; the [closure report](./NEXUS_FIRST_PRINCIPLES_CLOSURE_2026-10-11.md) records all eleven and current grades. The observations and grades below preserve the original review evidence. The architecture supports useful local orchestration, and recent corrections strengthen transactional terminal writes, strict optimistic concurrency, runner registration, and background-worker regression coverage. This review reproduced accounting loss, invalid execution ownership, incomplete mission cleanup, and broken frontend history behavior. Passing compilation and static gates do not establish those runtime invariants.

This is a review of the current, uncommitted working tree. Production source was not changed during this review. Temporary reproductions and graph analysis are isolated under `.tmp/nexus-review/`. Historical closure reports describe the findings identified at those times; they do not mean every possible defect has been closed.

The review followed scheduler admission through runner setup, inference, heartbeat, and terminal persistence; quota recording through background flushing and admission; event reception through scheduled execution; memory consolidation through authorization, provider execution, and persistence; and frontend history actions through settings, HTTP, and message mutation. Authentication, graph routes/services, foreign-key deletion, and audit tooling were also inspected. This is focused end-to-end review of these boundaries, not a fresh line-by-line certification of every repository file.

Grades measure whether the implementation maintains its contracts during normal operation, errors, cancellation, and concurrency. A means those contracts are demonstrated across relevant boundaries; B means sound foundations with bounded gaps; C means concrete failures require correction; D means critical contracts are broadly unreliable; F means the reviewed behavior is unusable or unsafe. These are engineering judgments, not percentages derived from test counts.

| Area | Grade | First-principles assessment |
|---|---|---|
| Architectural integrity | B− | Useful module boundaries and shared helpers; agent ownership still spans SQL, registry state, and task tracking without uniform enforcement. |
| Reliability and correctness | C | Accepted charges can disappear; a live agent can be reclaimed; setup failure can leave an active mission; history state can be corrupted. |
| Security | B− | Server-issued operator identity and memory-only configured credentials are sound foundations; quota durability and inconsistent frontend authentication weaken the controls. |
| Testing rigor | B | Meaningful worker and transaction tests exist, but adjacent failure boundaries and multi-agent isolation remain untested. |

P1 denotes a high-priority correctness or control failure; P2 denotes an affected feature or isolation defect. No P0 finding was established in the inspected paths. All six findings below were **open at review time**; their implemented corrections and current verification are recorded in the linked closure report.

# Section 2: Pillar I — Architectural Integrity (The Master Architect)

The directives/orchestration/execution separation is recognizable in the project. Rust runner modules separate context, provider execution, lifecycle, and terminal persistence; TypeScript stores hold UI state while shared services normally own HTTP settings and authentication. The important architectural issue is responsibility crossing those boundaries: direct SQL recovery bypasses the agent lifecycle service, and UI actions bypass the shared HTTP service. Additional generic repositories or distributed services would not address those particular failures.

The Force Graph handlers in [intelligence.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/routes/intelligence.rs:58) and their service implementations were reviewed. Graph query limits and path validation are present, and CPU graph traversal uses the blocking pool. Both graph representations were rebuilt locally with the current compiled library:

- Complete symbol graph: 3,699 nodes, 18,019 edges, 708 parsed files.
- Persistent graph: 5,626 nodes and 47,580 edges; SQLite integrity check returned `ok`.
- Review impact is retained in `.tmp/nexus-review/review-impact.json`. Name resolution estimates 364 affected files for metering, 76 for persistence, and 318 for the runner entry module.

Those impact counts include ambiguous name-based relationships. Graph risk records and candidate-test labels are heuristics, not confirmed vulnerabilities or measured coverage. For example, the graph labels no candidate tests for cognitive memory even though that module has inline tests. Manual call tracing and executable reproductions determine the findings.

One scaling concern remains separate from the six defects: [event_bus.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/system/event_bus.rs:86) awaits the entire triggered mission before receiving another event. The broadcast channel has capacity 100. Slow inference therefore increases lag and can drop intervening events even though the monitor correctly continues after a lag notification. If reliable triggers are required, use bounded admission with tracked execution and an explicit overflow policy; durable replay is a separate product requirement. An unbounded task spawn would replace one bottleneck with another.

Recommended decomposition should follow responsibilities, with correctness fixes reviewed separately:

| Priority | File | Focused decomposition |
|---|---|---|
| High | [persistence.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/persistence.rs) | Separate versioned agent writes, deletion, and claim/recovery operations; make recovery use the same ownership policy as admission. |
| High | [runner/mod.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/mod.rs) | Keep mission ownership and terminal cleanup explicit across setup, workflows, inference, and cancellation. Reuse the existing terminal module. |
| High | [metering.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/security/metering.rs) | Separate quota admission/reset from buffered accounting and durable flushing; define ownership of pending charges. |
| Medium | [sovereign_store.ts](G:/Autonomous-Agentic-Tadpole/src/stores/sovereign_store.ts) | Move history/leaves/revert HTTP calls into the existing service layer; centralize replacement of messages and their index. |
| Medium | [cognitive_memory.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/knowledge_store/cognitive_memory.rs) | Separate per-agent consolidation from sweep orchestration so failures remain local to an agent. |
| Medium | [event_bus.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/system/event_bus.rs) | Separate receiving/filtering from bounded job execution if trigger load warrants it. |

# Section 3: Pillar II — Reliability & Robustness (The QA Principal)

**F1 — P1: Buffered quota usage is discarded before a successful database commit.** [metering.rs:203](G:/Autonomous-Agentic-Tadpole/server-rs/src/security/metering.rs:203) sets agent entries to zero before beginning the transaction; mission entries do the same at line 227. Any begin, update, or commit error returns without restoring those amounts. The background flusher logs the error and retries, but the next attempt has no charge to persist. While a flush is pending, admission can also observe charges absent from both committed storage and the buffer.

The local Rust reproduction recorded $0.60 against a $0.50 quota, rejected the update with a SQLite trigger, removed the trigger, and retried. Both agent and mission quotas persisted $0.00 and admitted a new $0.10 request. These are calls to the actual public `BudgetGuard`, not copied implementation logic. **Correction:** keep pending charges counted until commit, serialize ownership of flush batches, preserve concurrent increments, and restore failed or cancelled batches. Check update row counts. Rust ownership of a batch alone does not guarantee database durability.

**F2 — P1: Stale-agent recovery permits reclaiming a still-running agent.** [persistence.rs:689](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/persistence.rs:689) clears SQL status and mission fields without the agent lifecycle lock, registry synchronization, tracked-runner checks, or a version increment. The scheduler uses a 300-second threshold. Heartbeats occur at turn/tool boundaries, whereas local OpenAI-compatible requests can legitimately have a 600-second timeout in [openai.rs](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/openai.rs:279). A slow supported request can therefore be treated as stale.

The reproduction kept a tracked Tokio task live and assigned an old heartbeat. Recovery changed SQL status to `idle`, left registry status `busy` and version 7 unchanged, and admitted a second claim. Scheduled spawning then uses the gateway's existing preemption policy, which can abort the original execution. **Correction:** recover under lifecycle ownership, synchronize persisted and registry state with strict version checks, and distinguish an expired execution from a live request within its allowed timeout. Preserve a way to terminate truly hung executions rather than treating any tracked handle as proof of health.

**F3 — P1: Context-preparation failure leaves an initialized mission active.** [runner/mod.rs:509](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/runner/mod.rs:509) awaits fallible context preparation after creating the mission, but assigns the cleanup guard's context only after that await succeeds. An early error leaves the guard unable to identify and finalize the mission. Gateway error cleanup releases agent fields, but does not finalize that mission row.

The reproduction invoked the actual public runner with workspace ID `...`. It returned `Invalid workspace ID`, while the migrated database mission remained `active` and the direct runner's agent remained `busy`. Adjacent branches also set `completed = true` before terminal persistence and discard errors from `fail_mission`, so terminal-write failure needs an explicit caller-level test. **Correction:** establish cleanup ownership immediately after mission initialization, await failure cleanup for setup errors, and disarm cleanup only after the terminal transaction succeeds. Preserve the original execution identity so recovery cannot mutate a replacement mission.

**F5 — P2: History replacement leaves a stale message index.** [sovereign_store.ts:262](G:/Autonomous-Agentic-Tadpole/src/stores/sovereign_store.ts:262) replaces `messages` without rebuilding `message_index_by_id`. Lookup, append, deduplication, and updates depend on that index. The frontend reproduction loaded message B over message A, then delivered a late update for A: the store overwrote B's text while retaining B's identity. **Correction:** replace messages and their derived index in one Zustand update. Unknown or stale message IDs must not address another message.

**F6 — P2: One agent's consolidation failure aborts the entire sweep.** The loop in [cognitive_memory.rs:107](G:/Autonomous-Agentic-Tadpole/server-rs/src/agent/knowledge_store/cognitive_memory.rs:107) propagates summarization and persistence errors with `?`. An eligible suspended or quota-denied agent, or a failing provider, stops processing later agents for that cycle. Because its source memories are preserved, the same failure can recur and repeatedly block later agents. This applies when the optional cognitive-memory pipeline is enabled. **Correction:** handle per-agent failures inside the sweep, retain the existing denial and memory-preservation checks, and continue processing other agents. This finding is established by source tracing; a multi-agent regression remains to be added.

Existing transaction boundaries, strict conflict returns, and conditional runner-handle removal are valuable. The remaining failures arise mainly from state-transition ordering and inconsistent ownership, rather than Rust memory unsafety. No new connection or handle leak was established in the inspected paths; that conclusion does not cover unexercised providers or native feature builds.

# Section 4: Pillar III — Security Posture (The Chief Auditor)

**F4 — P2: Four frontend actions bypass configured authentication and the backend URL.** History, leaves, and revert in [sovereign_store.ts:248](G:/Autonomous-Agentic-Tadpole/src/stores/sovereign_store.ts:248), plus promotion in [ArtifactPromotionCard.tsx:44](G:/Autonomous-Agentic-Tadpole/src/components/chat/ArtifactPromotionCard.tsx:44), read `localStorage['NEURAL_TOKEN']` and use relative URLs. The configured API token is held in settings memory and excluded from persisted storage. No production writer for that legacy key was found.

The frontend reproduction configured a synthetic valid settings token, called history loading with mocked HTTP, and observed `Authorization: Bearer null`. Against protected routes this produces authorization failure; a separately configured backend URL is also ignored. The three store actions do not check HTTP success and swallow failures, leaving users without actionable feedback. **Correction:** use the existing shared API service, encode path segments, validate response shape, and surface failures to the calling UI. Do not restore credential persistence as a workaround. This is a broken authentication integration, not evidence of a server authentication bypass.

Inspected server controls use the middleware-created `AuthenticatedOperator` extension rather than trusting client body role claims. Token matching uses fixed-length digest comparison, and rejected WebSocket protocol credentials are redacted. Event SQL filters use bound parameters. Graph service paths are checked against workspace boundaries. The configured frontend token remains memory-only at rest.

F1 weakens quota enforcement even with correct authentication: an acknowledged usage record must survive a database error before subsequent admission can be trusted. Security therefore includes durability and concurrency, not just input filtering. This review contacted no advisory service or external provider and makes no new dependency-cleanliness claim; the earlier live dependency audit and its residual items remain documented in the prior closure report.

# Section 5: Pillar IV — Testing Rigor (The Testing Expert)

The most critical reviewed logic is `AgentRunner::run`, including admission, initialized mission ownership, and terminal commit. The following plan tests observable state and provider calls rather than reproducing implementation details:

| Path | Scenario | Required assertions |
|---|---|---|
| Happy | Valid scheduled mission completes through a controlled local provider. | One provider execution; exact run-to-mission identity; terminal mission/run persisted; costs recorded once; agent and tracking released. |
| Failure: validation | Invalid workspace or missing model/provider context after initialization. | An initialized mission reaches a persisted failure state; no provider call; agent and tracking released; error reaches caller. |
| Failure: persistence | Reject initialization or terminal updates with SQLite triggers. | No partial committed state; cleanup failure is observable; owned recovery remains possible; usage is neither lost nor duplicated. |
| Failure: cancellation | Cancel during preparation and inference. | Only the owning execution is finalized; failure is recorded; handles released; no late cleanup mutates a replacement. |
| Edge | Competing scheduled admissions and a heartbeat older than the reap threshold during an allowed slow request. | One execution retains ownership; valid live execution is not reclaimed; truly expired execution can be recovered. |
| Edge | Terminal delivery repeated or stale version supplied after replacement. | No duplicate charge; stale writes conflict; replacement mission and tracking survive. |
| Edge | Empty output, exact quota boundary, zero usage, invalid numeric costs, and denied/suspended execution. | Defined terminal outcome; finite non-negative accounting; denied paths make zero provider calls. |

Additional focused regressions should cover failed/cancelled quota flushing for both buffer types, concurrent recording during flush, configured-token/URL handling for all four frontend actions, history replacement followed by late updates and deduplication, and a consolidation sweep with a denied/failing agent before a healthy agent.

**Fresh validation:** `cargo check --offline --locked` and the repository's TypeScript 7 compiler passed. Parity verification passed with 138 documented routes; final context verification passed 1,182 files, including this report; the adversarial static guard reported zero matches. The latter result is a source-pattern check and does not override the semantic defects reproduced here.

The isolated Rust driver successfully reproduced F1 for both agent and mission accounting, F2, and F3 against the current compiled library. Two isolated Vitest cases successfully reproduced F4 and F5 against the actual stores. These probes deliberately assert the defective observations: their passing result demonstrates reproducibility, not that a fix exists. Evidence is in `.tmp/nexus-review/rust-probes.log` and `frontend-probes.log`; they are excluded from the regular suite.

The earlier same-day full Rust run passed 547 tests with one existing ignored test, including all fourteen new background-worker regressions. Earlier frontend, Python, and browser results remain historical evidence; those full suites were not rerun for this review. Optional native audio/vector builds, production providers, desktop behavior, and sustained load remain outside the new runtime evidence.

The mandatory invariant gates were reviewed explicitly:

- **Scheduler:** The registration barrier prevents completion before handle insertion; task-ID comparisons protect replacement cleanup. F2 still breaks admission ownership through stale recovery.
- **Relational:** Mission children, the self-referencing session tree, and scheduled-run foreign keys were inspected alongside existing child-first deletion. This review changes no table, foreign key, or deletion order.
- **Concurrency:** Core versioned writes return conflicts without adopting a newer version in a retry. F2's direct SQL recovery bypasses that version discipline and leaves registry snapshots stale.
- **Trust boundaries:** Inspected privileged routes use server-issued identity; suspension and quota decisions use server-owned state. F1 concerns persistence of that authoritative quota state; F4 concerns supplying the actual configured credential.

The review recommended addressing F1–F3, then F4–F6, adding targeted regressions with each fix. That correction sequence is now documented in the linked closure report. Architectural decomposition remains a focused maintenance recommendation.

<!-- Metadata: [NEXUS_FIRST_PRINCIPLES_REVIEW_2026-10-11] -->
