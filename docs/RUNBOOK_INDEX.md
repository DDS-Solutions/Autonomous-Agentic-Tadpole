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
| **Cost spike** | `python execution/parity_guard.py --check=budget` | Verifies cost aggregation and billing limits. |
| **Merkle integrity error** | `python execution/sovereign_audit.py` | Audits Merkle chain signature hashes for tampering. |

## Execution Safety

| Symptom | Script / Command | Description |
|---------|------------------|-------------|
| **Suspect MCP tool** | `python execution/mcp_audit.py --skill=<name>` | Scans the manifest parameter types and permissions. |
| **AI Context Drift** | `python execution/verify_ai_context.py` | Validates file assist notes and trace scopes. |
| **Broken File Links** | `python execution/verify_ai_context.py --fix` | Auto-heals broken references and missing docs blocks. |
