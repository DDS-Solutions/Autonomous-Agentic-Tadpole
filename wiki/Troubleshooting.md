# 🔧 Troubleshooting

## Engine Won't Start

### Missing `NEURAL_TOKEN`

```
Error: NEURAL_TOKEN is required but not set
```

**Fix:** Set `NEURAL_TOKEN=your-token` in `.env`.

---

### Database Locked

```
error: Failed to open database: database is locked
```

**Cause:** Another process has `data/tadpole.db` open (possibly another engine instance).

**Fix:**
```bash
# Find and kill the process holding the lock
lsof data/tadpole.db      # Linux/Mac
# Windows: use Process Explorer or restart

# Or remove the WAL files (safe if engine is stopped)
rm data/tadpole.db-wal
rm data/tadpole.db-shm
```

---

### Port Already in Use

```
Error: Address already in use (os error 98)
```

**Fix:**
```bash
# Find what's using port 8000
netstat -ano | findstr :8000     # Windows
lsof -i :8000                    # Linux/Mac

# Stop it, or change the port in .env:
PORT=8001
```

---

## Tests Hanging (408 Timeout)

**Symptom:** `cargo test` hangs indefinitely, eventually times out.

**Cause:** Test sends a request but never calls `state.notify_boot_complete()`, so the boot-gate middleware blocks forever.

**Fix:** Add before `create_router()`:
```rust
let state = AppState::new_minimal_mock().await;
state.notify_boot_complete();  // ← Required!
let app = create_router(state.into());
```

---

## 401 Unauthorized on All Requests

**Check 1:** Is the token correct?
```bash
curl -v http://127.0.0.1:8000/v1/engine/health \
  -H "Authorization: Bearer $NEURAL_TOKEN"
```

**Check 2:** Is `NEURAL_TOKEN` set in `.env`?
```bash
cat .env | grep NEURAL_TOKEN
```

**Check 3:** Is the engine reading the right `.env`?
```bash
# Engine reads .env from WORKSPACE_ROOT (default = current directory)
WORKSPACE_ROOT=/correct/path npm run engine
```

---

## CORS Errors in Browser

**Symptom:** `Access to fetch at '...' has been blocked by CORS policy`

**Fix:** Add your frontend origin to `ALLOWED_ORIGINS`:
```ini
ALLOWED_ORIGINS=http://localhost:5173,http://127.0.0.1:5173
```

---

## Memory Endpoints Return 501

**Symptom:** `/v1/search/memory` returns `501 Not Implemented`

**Cause:** The `vector-memory` Cargo feature is not enabled.

**Fix:**
```bash
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory
```

Or if you just need the fallback SQLite search (no LanceDB):
The IKS endpoints (`/v1/agents/:id/memory`) still work without this feature.

---

## `cargo build` Fails: Missing Native Library

**Symptom:** `error: could not find native library...`

**Cause:** Usually from `vector-memory` (LanceDB requires Arrow C++) or `neural-audio` (Whisper requires libtorch).

**Fix:** Don't enable these features on Windows unless the native deps are installed:
```bash
# Safe build — no native deps
cargo build --manifest-path server-rs/Cargo.toml
```

---

## Dashboard Shows Stale Data

**Cause:** WebSocket connection dropped and didn't reconnect.

**Fix:**
1. Refresh the browser
2. Check the engine is still running: `curl http://127.0.0.1:8000/v1/engine/health`
3. Check browser console for WebSocket errors

---

## Backup Script Fails

```
ValueError: VACUUM INTO requires a file path, not `:memory:`
```

**Fix:** Ensure `DATABASE_URL` in `.env` points to a file path, not `:memory:`.

```ini
# Correct
DATABASE_URL=sqlite:./data/tadpole.db

# Wrong (for backup purposes)
DATABASE_URL=sqlite::memory:
```

---

## Engine Crash / Panic Log

If the engine crashes, check the panic log:

```bash
cat sidecar_panic.log
# or
cat $WORKSPACE_ROOT/sidecar_panic.log
```

Panic logs include the thread name, panic message, and source location.

---

## Parity Guard Failures

```bash
python execution/parity_guard.py
# Error: Documentation version mismatch: README says v1.1.57 but version.json says v1.1.58
```

**Fix:** Run the version sync:
```bash
npm run version:sync
```

---

## Getting More Help

1. Check `docs/ARCHITECTURE.md` for design context
2. Check `docs/OPERATIONS_MANUAL.md` for detailed runbooks
3. Enable debug logging: `RUST_LOG=debug npm run engine`
4. Review the audit trail: `/v1/oversight/audit-trail`
5. Run parity guard: `python execution/parity_guard.py`
6. Open an issue on GitHub with the panic log + steps to reproduce
