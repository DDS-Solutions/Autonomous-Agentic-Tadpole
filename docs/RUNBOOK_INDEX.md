> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[RUNBOOK_INDEX]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# Runbook Index — Symptom ➔ Script Mapping

This runbook directory guides engineers and automated agents in resolving common operational failure modes of Tadpole OS using the appropriate diagnostic and repair scripts.

## Engine & Database Issues

| Symptom | Script / Command | Description |
|---------|------------------|-------------|
| **Port 8000 in use** | `taskkill /F /IM server-rs.exe` | Kills stale engine processes holding the Axum socket. |
| **Missing NEURAL_TOKEN** | `python execution/rotate_token.py` | Generates a new secure token and updates `.env`. |
| **SQLite Locked** | `python execution/db_health_check.py` | Diagnoses current database connection locks. |
| **SQLite DB Corrupt** | `python execution/restore_sqlite.py <backup>` | Restores the database from a verified backup copy. |
| **Migration Failure** | `cargo sqlx migrate run` | Forces database migrations to re-run and sync. |
| **Config Drift** | `python execution/sync_version.py` | Syncs component versions across Cargo and package specs. |

## Swarm & Agent Issues

| Symptom | Script / Command | Description |
|---------|------------------|-------------|
| **Bunker unreachable** | `python execution/swarm_stress_test.py --target=bunker` | Verifies connectivity to model providers. |
| **Agent not recruiting** | `python execution/dispatch_mission.py --debug` | Traces mission-dispatch failures. |
| **Swarm partition** | `python execution/swarm_stress_test.py --partition` | Simulates and heals network partitioning. |
| **Stale agent busy state** | `python execution/test_restoration.py` | Verifies the agent reaping cycle restores stale agents. |

## Budget & Auditing

| Symptom | Script / Command | Description |
|---------|------------------|-------------|
| **Budget exhausted** | `python execution/quick_run.py --audit-budget` | Audits daily spent metrics against configured quotas. |
| **Cost spike / 2PC Lock Limit** | `python execution/parity_guard.py --check=budget` | Verifies A2A 2PC budget locks and 24h rolling spend limits. |
| **Merkle integrity error** | `python execution/sovereign_audit.py` | Audits Merkle chain signature hashes for tampering. |

## Execution Safety & Context Optimization

| Symptom | Script / Command | Description |
|---------|------------------|-------------|
| **Agent Tool Infinite Loop** | `python execution/tool_loop_guard.py` | Enforces 10-iteration ceiling & 3-repetition circuit breaker. |
| **Runtime Execution Errors** | `python execution/evaluate_annealing.py` | Analyzes fault registry and generates self-patching proposals. |
| **Fast Compiler Verification** | `python execution/cargo_fast_check.py` | Fast background Rust compilation check. |
| **BM25 Lexical Search** | `GET /v1/memory/search/bm25?q=<query>` | Sub-millisecond exact keyword/symbol search (< 1ms). |
| **Monologue Context Saturation** | `ContextManager::summarize_history()` | 2-Tier context compression (heuristic compaction & LLM summarizer). |
| **Suspect MCP tool** | `python execution/mcp_audit.py --skill=<name>` | Scans the manifest parameter types and permissions. |
| **AI Context Drift** | `python execution/verify_ai_context.py` | Validates file assist notes and trace scopes. |
| **Broken File Links** | `python execution/verify_ai_context.py --fix` | Auto-heals broken references and missing docs blocks. |

[//]: # (Metadata: [RUNBOOK_INDEX])
