> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[0004_wal_mode_sqlite]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# ADR 0004: Write-Ahead Logging (WAL) Mode SQLite

## Status: Accepted

## Context

Autonomous agent swarms perform high-frequency read and write updates on persistent agent state. In SQLite's default rollback journal mode, concurrent writes lock the database, throwing `SQLITE_BUSY` errors.

## Decision

Configure the SQLite connection pool to use **WAL (Write-Ahead Logging)** mode. Under WAL mode:
- Readers do not block writers, and writers do not block readers.
- Set `synchronous = NORMAL` to relax strict fsync safety for massive disk write performance gains.
- Configure a `busy_timeout` of 30 seconds to handle locking contention.

## Consequences

- **+** 10-15x write throughput improvement.
- **+** Eliminates read locking stalls during heavy agent writes.
- **-** Slightly higher complexity due to the presence of `.db-wal` and `.db-shm` temporary files.

[//]: # (Metadata: [0004_wal_mode_sqlite])
