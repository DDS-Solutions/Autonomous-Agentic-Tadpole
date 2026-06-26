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
| `database.wal_size_bytes` | > 100MB → trigger backup |
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
# data/backups/tadpole_YYYYMMDD_HHMMSS.sha256
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
# List available backups
python execution/restore_sqlite.py --list

# Restore from a specific backup (dry-run first)
python execution/restore_sqlite.py --backup data/backups/tadpole_20260626_020000.db --dry-run

# Execute restore (stops engine first)
python execution/restore_sqlite.py --backup data/backups/tadpole_20260626_020000.db
```

The restore script:
1. Stops the engine if running
2. Verifies SHA-256 hash matches manifest
3. Runs `PRAGMA integrity_check` on the backup
4. Replaces `data/tadpole.db` atomically
5. Restarts the engine

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
python execution/rotate_token.py --new-token "new-token-value"
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
    metrics_path: '/v1/engine/health'
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

To perform a cascading deletion of all data for a specific agent:

```sql
-- Run in SQLite (engine must be stopped first)
DELETE FROM agent_memories WHERE agent_id = 'AGENT_ID';
DELETE FROM agent_tasks WHERE agent_id = 'AGENT_ID';
DELETE FROM audit_trail WHERE agent_id = 'AGENT_ID';
DELETE FROM knowledge_store_meta WHERE source_agent_id = 'AGENT_ID';
DELETE FROM agents WHERE id = 'AGENT_ID';
```

Or via the API:

```bash
curl -X DELETE http://127.0.0.1:8000/v1/agents/AGENT_ID \
  -H "Authorization: Bearer $NEURAL_TOKEN"
```

The API cascade-deletes all associated memories, tasks, and oversight entries.

---

## 🔄 Graceful Shutdown

### Normal Shutdown (4-Phase)

```bash
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
curl -X POST http://127.0.0.1:8000/v1/engine/kill \
  -H "Authorization: Bearer $NEURAL_TOKEN"

# Or hard kill (last resort)
stop_AAtadpole.bat    # Windows
```

---

## 🔍 Audit Trail Review

```bash
# List recent audit entries
curl http://127.0.0.1:8000/v1/oversight/audit-trail \
  -H "Authorization: Bearer $NEURAL_TOKEN" | jq .data[-10:]

# Verify chain integrity
curl http://127.0.0.1:8000/v1/oversight/audit-trail/verify \
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
