# ADR 0001: Four-Phase Graceful Shutdown

## Status: Accepted

## Context

The engine runs 15+ background services that must drain in-flight work before exit. A single-phase shutdown caused:
- Dropped requests.
- Budget data loss.
- Swarm agent state inconsistency.

## Decision

Implement `ShutdownOrchestrator` with 4 distinct phases, allocating a 500ms drain delay to each:
1. **Ingestion & Networking** — Stop accepting new API tasks, complete in-flight requests.
2. **Telemetry & Monitoring** — Flush metrics logs, close tracing pipelines.
3. **Security & Cleanup** — Evict memory rate-limiters, clear caches.
4. **Persistence** — Flush final budget usages, save remaining agent states.

## Consequences

- **+** Zero data loss during graceful shutdowns.
- **+** Predictable shutdown timeline (~2 seconds).
- **-** Shutdown requires ~2 seconds vs. immediate kill signal.
