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
