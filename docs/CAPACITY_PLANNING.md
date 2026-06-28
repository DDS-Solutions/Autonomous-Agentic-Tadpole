> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[CAPACITY_PLANNING]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift or legacy terminology.
> - **Telemetry Link**: Search `[CAPACITY_PLANNING]` in audit logs.

# Tadpole OS Capacity Planning

This document details the hardware profiling, sizing formulas, and memory limits for running the Tadpole OS agent engine.

## Tokio Runtime Sizing

The Tokio multi-threaded scheduler is sized dynamically based on hardware limits and cgroup specifications:
- **Worker Threads**: `worker_threads = min(TOKIO_WORKER_THREADS, cpu_count, cgroup_pids_max / 4)` (defaults to active parallel cores with a minimum of 4).
- **Blocking Threads**: `blocking_threads = min(32, worker_threads * 8)` (defaults to 32 to prevent pool starvation).
- **Thread Stack Size**: `stack_size = 4MB` (defaults to 4MB to prevent stack overflows on deep recursive agent AST graph walking and compilation).

## SQLite Pool Sizing

Database scaling and concurrency boundaries:
- **Max Connections**: `max_connections = 25` (optimized for concurrent transaction writes).
- **Min Connections**: `min_connections = 3`.
- **Acquire Timeout**: `8s` (threads will wait up to 8s to check out a connection).
- **Busy Timeout**: `30s` (WAL mode automatic retry lock-holding window before returning `SQLITE_BUSY`).

## Baseline Performance (Measured on 2 vCPU / 4GB RAM)

| Metric | Target | Alert Threshold |
|--------|--------|-----------------|
| p50 `/health` latency | < 10ms | > 50ms |
| p99 `/v1/intelligence/graph` | < 200ms | > 500ms |
| p99 `/v1/agents` (100 agents) | < 100ms | > 300ms |
| Memory (idle) | < 500MB | > 1GB |
| Memory (load) | < 1.5GB | > 2.5GB |
| SQLite WAL size | < 10MB | > 50MB |

## Container Sizing (Kubernetes / Docker Limits)

```yaml
resources:
  limits:
    cpu: "2000m"
    memory: "4Gi"
  requests:
    cpu: "500m"
    memory: "1Gi"
```

[//]: # (Metadata: [CAPACITY_PLANNING])
