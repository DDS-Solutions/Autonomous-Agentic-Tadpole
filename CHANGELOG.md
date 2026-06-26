# Changelog

All notable changes to A-A Tadpole OS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.1.58] - 2026-06-26

### Added
- **SQLite Hot Backup & Restore**: Added [`execution/backup_sqlite.py`](execution/backup_sqlite.py) and [`execution/restore_sqlite.py`](execution/restore_sqlite.py) with WAL-mode-safe `VACUUM INTO` hot backups, SHA-256 integrity hashing, and `PRAGMA integrity_check` verification. Added `run_backup()` and `check_integrity()` Rust helpers to [`server-rs/src/db.rs`](server-rs/src/db.rs).
- **Zero-Downtime Token Rotation**: Auth middleware in [`server-rs/src/middleware/auth.rs`](server-rs/src/middleware/auth.rs) now validates `NEURAL_TOKEN_OLD` and `NEURAL_TOKEN_NEW` alongside the primary `NEURAL_TOKEN`, enabling a safe grace-window rotation procedure. Added `execution/rotate_token.py` script.
- **MCP Subprocess Security Hardening**: Refactored `execution/tadpole_mcp_server.py` to use `asyncio.create_subprocess_exec` (shell=False), `shlex` command splitting, skill allowlist, JSON Schema input validation, hard 30-second timeout, and `resource.setrlimit` CPU/memory limits on Linux.
- **Extended Health Endpoint**: `/v1/engine/health` now returns `database` (WAL size, pool stats), `budget` (total spent, limit), `swarm` (agent count, status), and `uptime_seconds` structured fields.
- **Monitoring Stack**: Added `monitoring/alerts.yml` (Prometheus rules for CPU, WAL size, memory, budget, error rate) and `monitoring/grafana/dashboards/tadpole_dashboard.json` (pre-built Grafana dashboard with 6 panels).
- **Rust Integration Test Suite**: Added 4 integration test modules — `backup_restore_tests.rs`, `token_rotation_tests.rs`, `health_endpoint_tests.rs`, `shutdown_orchestrator_tests.rs` — covering the P0/P1 hardening surface.
- **Python Unit Tests**: Added 5 unit test modules under `tests/unit/` — `test_backup_integrity.py`, `test_token_rotation.py`, `test_mcp_sandbox.py`, `test_snapshot_state.py`, `test_verify_ai_context_fix.py`.
- **Token Rotation Runbook**: Added full runbook to `docs/OPERATIONS_MANUAL.md` and `docs/SECURITY.md` covering zero-downtime rotation, rollback, and verification steps.
- **GDPR Cascading Deletion Runbook**: Documented the cascading `DELETE` procedure for agent data in `docs/OPERATIONS_MANUAL.md`.
- **Snapshot State Tool**: Added `execution/snapshot_state.py` for sandbox state checkpointing.

### Fixed
- **CI Workflow Parse Error**: Corrected missing `uses:` keyword in `.github/workflows/ci.yml` that caused GitHub Actions to fail with a workflow parse error.
- **Backup Test Migration**: Fixed `NOT NULL constraint failed: agents.metadata` in backup integration test by aligning INSERT columns with the actual migration schema.
- **Boot Gate Deadlock in Tests**: Fixed 3 integration tests hanging indefinitely by calling `state.notify_boot_complete()` before routing requests through the Axum `oneshot()` helper — the boot middleware was blocking until the signal was sent.

### Changed
- **Compiler Warning Elimination**: Eliminated all 70 Rust compiler warnings — feature-gated imports in `knowledge_store`, `graph/mod.rs` re-exports, unused variables in `networking.rs` and `memory.rs`, and scaffolded actor subsystem dead code suppressed with targeted `#[allow(...)]` attributes.
- **`docs/OPERATIONS_MANUAL.md`**: Major update adding Backup/Restore, Token Rotation, MCP Hardening, Observability, and GDPR sections.
- **`docs/SECURITY.md`**: Updated with dual-token rotation mechanism, MCP subprocess boundary hardening, and RCE mitigation threat model.

---

## [1.1.57] - 2026-06-26


### Added
- **Centralized Configuration Manager**: Added [config.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/config.rs) validating `.env.schema` keys (e.g., `PORT`, `BIND_ADDRESS`, `WORKSPACE_ROOT`) and utilizing closure-injection (`load_internal`) for side-effect-free unit testing.
- **Environment Schema Specification**: Added [.env.schema](file:///g:/Autonomous-Agentic-Tadpole/.env.schema) documenting types, default values, and sensitive markers for all engine environment variables.
- **Modular Startup Framework**: Decomposed the monolithic `startup.rs` into a clean [startup/](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/startup/) module directory with [mod.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/startup/mod.rs) and 5 specialized system service submodules under [services/](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/startup/services/).
- **Phased Shutdown Orchestrator**: Integrated a 4-phase graceful shutdown sequencer inside `startup/mod.rs` to drain networking, flush telemetry, trigger security evictions, and persist data budgets before process termination.
- **Docker-Transparent Address Resolver**: Added the [networking/](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/networking/) resolver module, optimizing Swarm networking via multi-strategy DNS candidate resolution (Local -> Docker Bridge -> Gateway) and caching.
- **SymbolNode Speet Types Export**: Added `SymbolNode` export in [bridge.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/bridge.rs) to expose type-safe graph contract bindings to Tauri/Specta frontends.

### Changed
- **Error Pipeline XSS Hardening**: Upgraded `ProblemDetails::new` in [error.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/error.rs) into a **Truncate (2048 bytes) -> Sanitize (HTML escape) -> Redact** pipeline to eliminate stored XSS risks in error response payloads.
- **Panic Hook Hardening**: Modified the emergency panic hook in [main.rs](file:///g:/Autonomous-Agentic-Tadpole/server-rs/src/main.rs) to restrict `sidecar_panic.log` permissions to `0o600` on Unix platforms and report write failures explicitly to `stderr`.
- **System Documentation Alignment**: Fully updated [docs/SECURITY.md](file:///g:/Autonomous-Agentic-Tadpole/docs/SECURITY.md), [docs/OPERATIONS_MANUAL.md](file:///g:/Autonomous-Agentic-Tadpole/docs/OPERATIONS_MANUAL.md), and [SYSTEM_MAP.md](file:///g:/Autonomous-Agentic-Tadpole/SYSTEM_MAP.md) to reflect the modularized startup services, config validations, and networking modules.
- **AI Context Marker Expansion**: Ran `awaken.py` to achieve 100% compliance across all 623 files, adding missing `AI Assist Note` or `Debugging & Observability` context blocks.

---
