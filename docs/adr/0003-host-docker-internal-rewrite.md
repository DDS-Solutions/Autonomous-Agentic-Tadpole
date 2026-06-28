> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[0003_host_docker_internal_rewrite]` in audit logs.
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
> - **Telemetry Link**: Search `[0003-host-docker-internal-rewrite]` in audit logs.

# ADR 0003: Host Docker Internal Rewrite

## Status: Accepted

## Context

Running provider endpoints (e.g. local Ollama) in Docker containers requires pointing requests to `http://host.docker.internal:11434`. However, running the engine natively requires `http://localhost:11434`. Hardcoding these breaks local parity.

## Decision

Implement an `AddressResolver` utility within the networking layer that detects the runtime host context (native vs. Docker container) and dynamically rewrites `localhost` or `127.0.0.1` to `host.docker.internal` if executed from inside a Docker environment.

## Consequences

- **+** Out-of-the-box local developer parity.
- **+** Code config remains identical between containerized and native environments.
- **-** Adds a lightweight latency check during first connection setup.

[//]: # (Metadata: [0003_host_docker_internal_rewrite])
