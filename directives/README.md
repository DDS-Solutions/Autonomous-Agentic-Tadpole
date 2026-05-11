# Directives Index

This directory contains operating directives used by Tadpole OS agents and workflows.

## Core Directives

These files should be treated as the current high-authority set:

- `IDENTITY.md`: system identity, versioned user-agent guidance, and agent stance.
- `GOVERNANCE.md`: governance gates, proof-of-work expectations, and escalation rules.
- `GEMINI.md`: orchestration protocol for Gemini/agent execution.
- `AUTONOMY_MANIFEST.md`: current autonomous audit objectives.
- `orchestrate.md`: mission orchestration checklist.
- `neural_handoff.md`: recursive agent handoff protocol.
- `LONG_TERM_MEMORY.md`: persistent memory behavior.
- `FAULT_REGISTRY.md`: fault tracking.

## Workflow Directives

Most other Markdown files in this directory are workflow templates or business/process playbooks. They are useful, but they should not override the core directives above.

Examples:

- planning and retrospectives
- incident response
- security audit
- market research
- product sync
- deployment and database migration
- support training

## Feature Directives

Feature-specific docs live under:

- `features/`

## Maintenance Rules

- Keep runtime facts in sync with `README.md`, `SYSTEM_MAP.md`, and `docs/ARCHITECTURE.md`.
- Do not point directives at files that do not exist.
- Use `python execution/parity_guard.py` before treating documentation as current.
- Prefer adding new workflow directives with a clear title and scope instead of editing core directives for one-off tasks.

[//]: # (Metadata: [directives_index])
