> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Operations_Manual]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# 🏥 Operations Manual

Runbooks for day-2 production operations. See also `docs/OPERATIONS_MANUAL.md` for the full version.

---

## 📋 Health Monitoring

### Check Engine Health

```bash
curl http://127.0.0.1:8000/v1/engine/health | jq .
```

Key fields to monitor:

| Field | Alert Threshold |
|-------|----------------|
| `database.wal_size_mb` | > 100 MB → trigger backup |
| `budget.total_spent_usd` | > 80% of limit |
| `uptime_seconds` | Unexpected reset = crash |

---

## 🗄️ Database Backup & Restore

### Backup (WAL-safe hot backup)

```bash
# Run manual backup
python execution/backup_sqlite.py

# Backups are written to:
# data/backups/tadpole_YYYYMMDD_HHMMSS.db
# data/backups/tadpole_YYYYMMDD_HHMMSS.meta.json
```

Recommended schedule: daily via cron or Windows Task Scheduler.

```bash
# Example cron (Linux/Mac)
0 2 * * * cd /path/to/tadpole && python execution/backup_sqlite.py

# List backups
ls data/backups/
```

### Restore

```bash
# Stop the engine first, then restore from a specific backup path or filename:
python execution/restore_sqlite.py data/backups/tadpole_20260626_020000.db
# Optional flags: --allow-unverified (skip .meta.json SHA-256 check), --force (bypass live-engine check)
```

The restore script:
1. Checks that the engine is not actively running (aborts unless `--force` is passed)
2. Verifies the SHA-256 hash matches the companion `.meta.json` manifest
3. Runs `PRAGMA integrity_check` on the backup
4. Backs up the active `data/tadpole.db` to `tadpole.db.bak`, restores via SQLite `.backup()`, and cleans stale `-wal`/`-shm` sidecars
5. Verifies table row-count parity (automatically rolling back from `tadpole.db.bak` on mismatch)

---

## 🔑 Token Rotation

### Zero-Downtime Rotation Procedure

```bash
# Step 1: Generate a strong new token
python -c "import secrets; print(secrets.token_hex(32))"
# Output: abc123def456...

# Step 2: Update .env to stage the transition
NEURAL_TOKEN=current-token        # still primary
NEURAL_TOKEN_OLD=current-token    # kept valid
NEURAL_TOKEN_NEW=new-token        # pre-validated

# Step 3: Restart engine (both tokens now accepted)
npm run engine

# Step 4: Update all clients (API keys, dashboard settings, CI secrets)
# Verify: curl -H "Authorization: Bearer new-token" http://127.0.0.1:8000/v1/engine/health

# Step 5: Promote and clean
NEURAL_TOKEN=new-token
# Remove NEURAL_TOKEN_OLD and NEURAL_TOKEN_NEW from .env

# Step 6: Final restart
npm run engine
```

Or use the automated script:

```bash
python execution/rotate_token.py                  # generates a secure 64-hex token & sets 300s grace window
# Restart engine, migrate clients, then confirm:
python execution/rotate_token.py --confirm
```

---

## 📊 Monitoring with Prometheus + Grafana

### Prometheus Configuration

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'tadpole'
    static_configs:
      - targets: ['127.0.0.1:8000']
    metrics_path: '/metrics'
    bearer_token: 'your-neural-token'
```

The pre-built alert rules are in `monitoring/alerts.yml`. Load with:

```bash
# If using Prometheus with --web.enable-lifecycle
curl -X POST http://localhost:9090/-/reload
```

### Grafana Dashboard

Import `monitoring/grafana/dashboards/tadpole_dashboard.json` via Grafana UI:
1. Open Grafana → Dashboards → Import
2. Upload the JSON file
3. Select your Prometheus data source
4. Click Import

The dashboard includes:
- Engine uptime and heartbeat
- Request rate and error rate
- Database WAL size
- Agent count and status
- Budget spend vs limit
- Memory and CPU usage

---

## 🗑️ GDPR / Data Deletion

Delete an agent and its dependent operational records via the API:

```bash
curl -X DELETE http://127.0.0.1:8000/v1/agents/AGENT_ID \
  -H "Authorization: Bearer $NEURAL_TOKEN"
```

`delete_agent_cascade` (`server-rs/src/agent/persistence.rs`) executes an atomic `BEGIN IMMEDIATE` transaction:
- Rejects agents with `a2a_ledger` counterparty history (`409 Conflict` — suspend instead).
- Preserves the append-only `audit_trail` Merkle chain intact and detaches `oversight_log.mission_id` (`SET NULL`).
- Deletes child-first across `mission_logs`, `swarm_context`, `agent_directives`, `peer_reviews`, `mission_nodes`, `mission_quotas`, `file_revisions`, `mission_history`, `agent_economics_meta`, `agent_quotas`, `sync_manifest`, `fallback_memories`, `agent_hires`, `skill_subscriptions`, `agent_tasks`, and `agents`.

---

## 🔄 Graceful Shutdown

### Normal Shutdown (4-Phase)

```bash
# Uses NEURAL_TOKEN_ADMIN when tiered RBAC is configured
curl -X POST http://127.0.0.1:8000/v1/engine/shutdown \
  -H "Authorization: Bearer $NEURAL_TOKEN"
```

The 4-phase sequence:
1. **Drain** — stop accepting new requests, finish in-flight
2. **Flush** — write telemetry and audit entries
3. **Evict** — clean up security sessions and token caches
4. **Persist** — flush provider/model registries to disk

### Emergency Kill

```bash
# Uses NEURAL_TOKEN_ADMIN when tiered RBAC is configured
curl -X POST http://127.0.0.1:8000/v1/engine/kill \
  -H "Authorization: Bearer $NEURAL_TOKEN"

# Or hard kill (last resort)
stop_AA_tadpole.bat    # Windows
```

---

## 🔍 Audit Trail Review

```bash
# List recent audit entries
curl http://127.0.0.1:8000/v1/oversight/security/audit-trail \
  -H "Authorization: Bearer $NEURAL_TOKEN" | jq .data[-10:]

# Verify chain integrity
curl http://127.0.0.1:8000/v1/oversight/security/integrity \
  -H "Authorization: Bearer $NEURAL_TOKEN"
```

---

## 📋 Pre-Deployment Checklist

- [ ] Run full test suite: `cargo test --manifest-path server-rs/Cargo.toml --bin server-rs`
- [ ] Run parity guard: `python execution/parity_guard.py`
- [ ] Take database backup: `python execution/backup_sqlite.py`
- [ ] Verify health endpoint responds correctly
- [ ] Check audit trail chain integrity
- [ ] Review budget spend levels
- [ ] Confirm all Prometheus alerts are green

[//]: # (Metadata: [Operations_Manual])
