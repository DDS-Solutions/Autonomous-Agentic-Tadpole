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
