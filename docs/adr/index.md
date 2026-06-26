# Architectural Decision Records (ADR)

This folder contains formal records of architectural design decisions made for Tadpole OS.

## Decision Log

- **[ADR 0001: Four-Phase Graceful Shutdown](file:///g:/Autonomous-Agentic-Tadpole/docs/adr/0001-four-phase-shutdown.md)** — Implements structured subsystem teardown to guarantee zero data loss.
- **[ADR 0002: Tokio Worker Pool Sizing](file:///g:/Autonomous-Agentic-Tadpole/docs/adr/0002-tokio-worker-sizing.md)** — Configures custom executor bounds to prevent thread pool starvation.
- **[ADR 0003: Host Docker Internal Rewrite](file:///g:/Autonomous-Agentic-Tadpole/docs/adr/0003-host-docker-internal-rewrite.md)** — Translates connection URLs dynamically to support Docker vs. native configurations.
- **[ADR 0004: Write-Ahead Logging (WAL) Mode SQLite](file:///g:/Autonomous-Agentic-Tadpole/docs/adr/0004-wal-mode-sqlite.md)** — Configures SQLite persistence for high concurrency agent write flows.
- **[ADR 0005: Runtime Secret Redaction Pattern](file:///g:/Autonomous-Agentic-Tadpole/docs/adr/0005-secret-redaction-patterns.md)** — Implements automatic filtering to prevent API key leaks in telemetry and logs.
