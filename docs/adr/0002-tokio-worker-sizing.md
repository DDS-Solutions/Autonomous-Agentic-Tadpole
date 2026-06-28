> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[0002_tokio_worker_sizing]` in audit logs.
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
> - **Telemetry Link**: Search `[0002-tokio-worker-sizing]` in audit logs.

# ADR 0002: Tokio Worker Pool Sizing

## Status: Accepted

## Context

Running intensive code parser AST walking, LLM completions, and vector database operations concurrently on a single thread pool can saturate worker threads, leading to HTTP request timeouts.

## Decision

Configure a custom Tokio Runtime Builder with:
- **worker_threads**: Match CPU cores (min 4).
- **max_blocking_threads**: 32 (forces blocking operations to execute on background helper pools).
- **thread_stack_size**: 4MB (accommodates deep recursive AST parses without triggering overflows).

Allows environment overrides via `TOKIO_WORKER_THREADS`, `TOKIO_MAX_BLOCKING_THREADS`, and `TOKIO_THREAD_STACK_SIZE_MB`.

## Consequences

- **+** Higher system concurrency and request survival.
- **+** Prevents stack overflow panics in parser tasks.
- **-** Slightly higher baseline RAM overhead due to thread stacks.

[//]: # (Metadata: [0002_tokio_worker_sizing])
