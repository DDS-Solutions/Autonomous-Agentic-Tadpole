> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[0005_secret_redaction_patterns]` in audit logs.
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
> - **Telemetry Link**: Search `[0005-secret-redaction-patterns]` in audit logs.

# ADR 0005: Runtime Secret Redaction Pattern

## Status: Accepted

## Context

Agents generate detailed reasoning steps and logs during execution. If they output environment variables, database keys, or provider API keys to logs, standard output, or telemetry streams, it can lead to credential leakage.

## Decision

Implement a centralized `SecretRedactor` service inside the server middleware and MCP host that holds a list of configured sensitive environment values (e.g. `GOOGLE_API_KEY`, `NEURAL_TOKEN`). 
All telemetry payloads and logs are filtered through a regex pattern matcher that replaces any occurrence of these keys with `[REDACTED]`.

## Consequences

- **+** Prevents accidental exposure of keys in logs or cloud syncs.
- **-** Lightweight string parsing overhead on telemetry channels.

[//]: # (Metadata: [0005_secret_redaction_patterns])
