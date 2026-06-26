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
