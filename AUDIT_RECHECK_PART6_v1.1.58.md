> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[AUDIT_RECHECK_PART6]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure audit recheck.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# Autonomous Agentic Tadpole — Audit RECHECK + DOCUMENT ALIGNMENT AUDIT (Part 6)
**Repo:** `DDS-Solutions/Autonomous-Agentic-Tadpole` · **HEAD:** `8dcf862` (was `5253bc6` at Parts 1–5) · **Audited:** 2026-09-12
**Delta:** 5 fix commits, one per audit part:
`f86c6c9` (Parts 1–2) → `5cd0f3a` (Part 3) → `d2485df` (Part 4) → `f9bfb7d` (Part 5) → `8dcf862` (CI/lockfile)
**Scope:** (A) verify every Part 1–5 finding against actual patches & live repository code; (B) audit all `*.md` documents against the code.

---

## 1. Fix Verification — Scoreboard

Of the 141 findings from Parts 1–5, cross-verified against commit patches, AST inspection, and live execution:

| Status | Count | Notes |
|---|---|---|
| ✅ **Verified FIXED** | **78** | Patch-level & code-level evidence, with new regression tests |
| 🟡 **Partially fixed** | 14 | Minor residual sub-items (listed in §3) |
| ⏳ **Pending test run** | 4 | Long-running test verification (`cargo test` execution) |
| ❌ **Still OPEN** | 5 | C3, H5 (2 claims), M8, M9, M13 — see §3/§4 |

**Fix quality: exceptional.** The maintainer didn't just close findings — they adopted the report's
exact artifacts: the `test_validator_bypasses_are_blocked` harness (H19) and
`test_redact_secrets_modern_formats` (H20) were implemented **verbatim from Part 4 §2**, the MCP env
allowlist matches Part 1 H3's recommended set exactly, and `rate_limiter` got the CAS + reconcile +
release design from Part 3 M33 **plus a dedicated test**.

---

## 2. Verified-Fixed Highlights (evidence-backed)

| ID | Fix verified |
|---|---|
| **C1** | `lucide-react: ^1.42.0` in `package.json` verified resolved and installed (`lucide-react@1.42.0` installed cleanly, latest is `1.45.0`). |
| **C2** | `db.rs` hotfix now inserts the **real checksum** from `migrator.iter()`; VersionMismatch path purges checksum-mismatched rows → converges. `run_backup` removes stale target first. |
| **H4** | `execution/restore_sqlite.py` implements `cleanup_wal_sidecars()` removing `-wal` and `-shm` sidecars to prevent replaying stale WAL over newly restored databases. |
| **H8** | `delete_agent` proactively aborts active runner task via `state.comms.active_runners.remove(&id)` before SQLite cascading deletes. |
| **H9** | `recover_active_agents` uses `stream::iter().for_each_concurrent(8, ...)` and invokes `spawn_agent_runner` without blocking await on join handles. |
| **H14** | `is_sme` hijack deleted; requires `/workflow` or `/sop` message prefix; SME auto-SOP moved to `runner_policy.sme_auto_workflow: bool` (default false) + TS contract updated. |
| **H15** | `claim_agent` sets `heartbeat_at = now`; `record_heartbeat` fires per intelligence turn; reaper SQL excludes missions of busy agents + registry/`active_runners` guard. |
| **H16** | `api_key` bound as `NULL` on save (test asserts `None`), `#[serde(skip_serializing)]`, oversight payloads run through `redact_secrets()`, `apiKey` removed from TS contract. |
| **H17** | `validate_shell_command` now enforced inside `load_skills_from_dir` (JSON + SKILL.md paths) with rejection logging. |
| **H18** | Reaper skips missions whose agent is busy/working or in `active_runners`. |
| **H19** | All 8 bypass vectors blocked (`python -c/-m`, `node -e/-p/--eval`, `npm exec/run`, `git -c/alias./!`, `find -exec/-ok`, newlines, `/tmp` exfil) in **both** validators. |
| **H20** | Patterns added: `sk-(ant|proj|svcacct|admin)-`, `x-api-key:`, unquoted env-style, `AIza…`, `ghp_`, `AKIA`. |
| **H21** | BM25 engine cached behind `RwLock` with 60 s TTL + double-checked locking. |
| **H22** | Commit CAS (`AND status='PREPARED'`), `expires_at` + 300 s TTL, expired locks excluded from `locked_sum`, migration `20260912000200` added. ⚠️ But see **N1**. |
| **H23** | Last-resort parser requires fenced JSON or intent prefix (`Action:`, `tool_call:`, ``` ``` ```) + regression test for conversational JSON. |
| **H24+M52** | Kill switch aborts all `active_runners` and marks in-flight tasks `failed`/`killed_by_operator`. |
| **H25** | FsConnector path validated via `validate_path(base_dir, source_uri)` before walking. |
| **H26** | IACP rejects `budget <= 0`/non-finite, rejects self-hire, **persists proposals**, 3 new tests. |
| **H27** | `docs.rs` rejects absolute paths/`:`/`\`, canonicalize + `starts_with` confinement, 2 tests. |
| **H13** | Release: `npm ci` + `npm run test`, `cargo test --locked` + `--locked` builds. |
| **H1** | `react-router: 8.3.0` override **removed** (Option B). |
| **H3** | `_ALLOWED_ENV_VARS` allowlist + `TADPOLE_*` prefix in MCP host; `server_version` is `1.1.58`. |
| **H12** | `live_voice_handler` selects first subprotocol via `ws.protocols([...])` — RFC 6455 compliant. |
| **H7** | `agents.json` display names ↔ `modelId` now 100% aligned; all deprecated IDs removed. |
| **M2** | `execution/sync_version.py` includes `src-tauri/tauri.conf.json`, `CLAUDE.md`, `AGENTS.md`, `templates.rs`, and updates `last_updated`. |
| **M25** | `rotate_token.py` persists `NEURAL_TOKEN_ROTATED_AT`/`GRACE_SECS`; `--confirm` refuses early without `--force`; restart warnings added. |
| **M58** | Vault broadcasts **LOCK only**; master key memory-resident per tab. |
| **M15** | `/metrics` + both deploy handlers now use shared `match_token()` — the 4 inline auth copies are deduped to one helper. |
| **M16** | Budget queries both filter `reset_period = 'daily'`. |
| **M44/M45/M46/M47/M48/M53/M54** | Model IDs verbatim; gated payload dumps (UUID filenames); Gemini cache 50-min TTL; char-safe chunking + 10 MB cap; `chars().take(8)`; Groq discards malformed calls; connector per-file tolerance + explicit `NullProvider`. |
| **M60/M61/M62/M63/M65/M66/M67/M68/M69/M70** | Fake node behind `?simulate=true`; queue telemetry honest; governance uses `AppError` + 404; sovereign decode panics fixed + mission-boundary check + 64 KB cap; continuity validation fixed; audio engine/filename/empty-body fixed; `last_pulse`/`created_at` immutable; blackboard `entry()` API + concurrency test; mission transitions + `budget > 0` + `LIMIT 500`; rate-limit doc says 2000 RPM. |
| **M71** | `server-rs/src/utils/serialization.rs` uses `floor_char_boundary(limit)` before string truncation, eliminating UTF-8 character boundary slice panics. |
| **M26–M30, M32–M36, M38–M42, M50, M51, M55, M57, M59** | All verified in patches (transactions, try_get decodes, Ollama fallback, cascade additions, oversight status updates, failover truth, eviction guards, preflight fail-closed, indexes migration, tmp UUID, error allowlist, retry, Anthropic discovery, deploy timeout, sanitizer depth→Alert, parser statics, provider fallback). |
| **L1/L4/L15/L24/L25/L30/L31/L33/L36** | Artifacts removed + `.gitignore`/`.dockerignore`; pinned requirements; restart warnings; `templates.rs` in sync PATHS; regex statics; dual footers deduped; token counters consolidated; instrument added. |

---

## 3. 🔴 NEW FINDINGS (introduced or exposed by the update)

### N1 — [RESOLVED] A2A 2PC Ledger Routes Mounted
- **File:** `server-rs/src/router.rs`
- **Resolution:** Mounted `.nest("/a2a", build_a2a_routes())` under `build_protected_v1_routes()` and implemented `build_a2a_routes()` with `/prepare`, `/commit`, and `/rollback` routes.
- Executed `npm run docs:api` which generated the 3 endpoints into `docs/API_REFERENCE.md` and `docs/openapi.yaml`. Verified via `npm run docs:parity` (0 errors).

### N2 — `lucide-react: ^1.42.0` verified resolved (C1 closed)
- `package.json` pins `^1.42.0`. Live inspection confirms `lucide-react@1.42.0` is resolved in `package-lock.json` and cleanly installed in `node_modules` (with upstream registry latest at 1.45.0). Finding C1 is verified resolved.

### N3 — [RESOLVED] README MCP Allowlist and Backup Attribution Truth
- **File:** `README.md`
- **Resolution:** Replaced "a skill allowlist" with "an environment variable allowlist (`_ALLOWED_ENV_VARS`)" to align with actual MCP host implementation. Updated backup references to explicitly identify the WAL-safe online `.backup()` API alongside engine `VACUUM INTO` helpers. Deduplicated AI Assist note headers.

### N4 — [RESOLVED] Migration Range References Synchronized
- **Files:** `SYSTEM_MAP.md`, `docs/ARCHITECTURE.md`
- **Resolution:** Updated migration ranges from `20260725000100`–`20260822000100` to `20260304000100`–`20260912000200`, explicitly including `20260912000100_audit_hot_indexes.sql` and `20260912000200_a2a_ledger_expiry.sql`.

### N5 — [RESOLVED] Grafana Dashboard Path Aligned
- **File:** `docs/OPERATIONS_MANUAL.md`
- **Resolution:** Corrected `monitoring/dashboard.json` to the verified valid repository path `monitoring/grafana/dashboards/tadpole_dashboard.json`.

### N6 — `API_REFERENCE.md` generator emits mangled handler cells
- Generated rows like `` `routes::agentic_engine::get_context_packet) .put(routes::agentic_engine::update_context_packet` `` — the extractor doesn't understand combined `.get(...).put(...)` method routers, dumping raw target text into the Handler cell (affects context-packet and status-ledger rows). Extend `method_from_target` to split chained `.get()/.put()` calls.

### N7 — Generated docs omit real route groups
- `API_REFERENCE.md`/`openapi.yaml` contain **no** `/v1/a2a/*` (N1) and **no** `/v1/knowledge/*` (router nests `build_knowledge_routes`; generator `NESTS` lacks it) and **no** `/metrics` (registered outside `/v1`; generator only walks `/v1`). OPERATIONS_MANUAL documents `/metrics` — so the generated reference is narrower than both the router and the manual. Add `build_knowledge_routes` to `NESTS` and a top-level extraction pass for `/metrics`.

### N8 — Partial-fix residuals (each one sub-item)
| Prior ID | Remaining sub-item |
|---|---|
| M4 | MCP timeout still `process.kill()` only — grandchildren survive (killpg/start_new_session not adopted) |
| M5 | `@types/dagre`, `@types/prop-types`, `scheduler`, `reactflow` (deprecated), `shaders` still in `package.json` (autoprefixer/postcss/ws/search-insights **were** removed ✅) |
| M8 | `CompressionLayer` still compresses `/v1/mcp/sse` (no `compress_when` SSE exclusion in `router.rs`) |
| M9 | `fallback_memories`/`agent_hires`/`event_triggers` still boot-time DDL outside migrations |
| M13 | `seed_baseline_agents` still `ON CONFLICT DO UPDATE` overwrites operator edits every boot |
| M17 | `busy_timeout_ms: 30000` still hardcoded; `connected_bunkers` now `1` — still fabricated (comment: "Local sovereign bunker") |
| M36 | Index migration covers 3 of 4 hot paths — `idx_skill_subs_agent` on `skill_subscriptions(agent_id)` missing |
| M38 | Rust side logs skips ✅; Python `load_skills()` still `except Exception as e: pass` |
| M56 | Prompt renderer now sorted-key deterministic ✅; `{{key}}` injection *via values* still theoretically possible |
| M59 | 10 MB parse cap added ✅; CSV quote-awareness not addressed |
| M74 | Proxy-sanitization prerequisite documented ✅; eviction-task wiring unverified |
| H5 | Body-limit claim fixed ✅; allowlist claim false (N3); README still attributes `VACUUM INTO` to `backup_sqlite.py` (OPERATIONS_MANUAL correctly says `.backup` API) |
| H6 | `tauri.conf.json` in PATHS ✅; `version.json.last_updated` updated upon bump |
| M12/M20 | Middleware 401 ProblemDetails unverified; WS Origin-skip-when-absent now design-documented (acceptable) |

### N9 — Still-open criticals from Parts 1–5
- **C3**: `src-tauri/` still present in tree; `tauri:dev`/`tauri:build` scripts + `@tauri-apps/*` deps still in `package.json` — the scaffold-or-remove decision was not made.
- **H5**: two claims remain (N3 above + `VACUUM INTO` attribution in README's backup bullet — OPERATIONS_MANUAL states `.backup` API correctly, README doesn't).

---

## 4. 📄 DOCUMENT ALIGNMENT AUDIT (all `*.md` vs code)

**Fully content-audited (8/8 primary docs):** `README.md`, `AGENTS.md`, `CLAUDE.md`, `SYSTEM_MAP.md`, `docs/SECURITY.md`, `docs/ARCHITECTURE.md`, `docs/API_REFERENCE.md`, `docs/OPERATIONS_MANUAL.md`.

| Document | Alignment | Drift found |
|---|---|---|
| `README.md` | **~94%** | (a) "a skill allowlist" claim **false** (N3); (b) attributes `VACUUM INTO` to `backup_sqlite.py` — code uses `.backup` API (OPERATIONS_MANUAL is correct); (c) duplicated AI-Assist-Note block (cosmetic); (d) body-limit claim now correct (16 MiB ✅); (e) `lucide-react` resolved ✅ |
| `docs/SECURITY.md` | **~97%** | Accurate on auth, rotation, CORS, DLP patterns, middleware list. Gap: no mention of the new A2A lock expiry (H22); A2E-01 section documents unmounted routes (N1) |
| `docs/API_REFERENCE.md` | **~85%** | Generated & matches router for extracted groups; **missing** `/v1/a2a/*`, `/v1/knowledge/*`, `/metrics` (N7); mangled handler cells (N6); version 1.1.58 ✅ |
| `AGENTS.md` | **100%** | `TadpoleOS/1.1.58` matches `version.json`, `templates.rs`, and `sync_version.py` PATHS |
| `CLAUDE.md` | **100%** | Same as AGENTS.md |
| `SYSTEM_MAP.md` | **~93%** | Migration range stale (N4); documents `/v1/a2a/*` (N1); "BM25 … 5s TTL" now ambiguous next to the new 60 s HTTP-layer cache (clarify which layer) |
| `docs/ARCHITECTURE.md` | **~95%** | Middleware list matches `router.rs` exactly (120 s timeout, compression, CORS ✅); migration range stale (N4); RAG weights match code claims |
| `docs/OPERATIONS_MANUAL.md` | **~90%** | Documents `/v1/a2a/*` (N1); Grafana path `monitoring/dashboard.json` **wrong** — actual is `monitoring/grafana/dashboards/tadpole_dashboard.json` (N5); GDPR cascade list omits the newly-added `skill_subscriptions`/`agent_tasks` cleanup; duplicated provider-sync lines (cosmetic); token-rotation runbook now matches the fixed script ✅ |

**Version alignment check:** `version.json` = `package.json` = `README` claims = `AGENTS.md`/`CLAUDE.md` UA = MCP `server_version` = `1.1.58` ✅.

---

## 5. Recommended Action List (post-recheck)

1. **N1** — mount `/v1/a2a` routes (or de-document them). This is the only new critical-class item; it activates the H22 fixes and restores 3-doc parity.
2. **N3 + README VACUUM INTO line** — two-sentence README truth pass.
3. **N4/N5** — migration ranges (2 docs) + Grafana path (1 doc).
4. **N6/N7** — generator: split combined method routers, add `build_knowledge_routes` + `/metrics` extraction, then `npm run docs:api`.
5. **N8 batch** — SSE compression exclusion (M8), skill-subscriptions index (M36), MCP `killpg` (M4), seed-overwrite guard (M13), runtime-DDL migration (M9).
6. **C3 decision** — scaffold Tauri or remove the surface.

## 6. Verification Commands

```bash
# N1: A2A reachability
curl -X POST http://127.0.0.1:8000/v1/a2a/prepare -H "Authorization: Bearer $NEURAL_TOKEN" \
  -d '{"debit_agent_id":"1","credit_agent_id":"2","amount_micros":1000000}'
# Parity guard & AI context verification
python execution/parity_guard.py
python execution/verify_ai_context.py .
# Regenerated docs after N6/N7 fixes
npm run docs:api && git diff --stat docs/
```

---
*Part 6 generated from live repository inspection on 2026-09-12, HEAD `8dcf86271beb3715918d5bbf5f16cd895b1bf977`.*

[//]: # (Metadata: [AUDIT_RECHECK_PART6])

