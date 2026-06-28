> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Home]` in audit logs.
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
> - **Telemetry Link**: Search `[Home]` in audit logs.

# 🐸 Autonomous Agentic Tadpole — Wiki

> **Version:** v1.1.59 &nbsp;|&nbsp; **Engine:** Rust 2021 + Axum &nbsp;|&nbsp; **Dashboard:** React 19 + TypeScript &nbsp;|&nbsp; **DB:** SQLite / SQLx

---

A-A Tadpole is a **local-first operating system for autonomous AI agent swarms** — a production-grade control plane combining a Rust engine, React operations dashboard, SQLite persistence, governed skill execution, and live mission telemetry in a single deployable unit.

## 📚 Wiki Pages

| Page | Description |
|------|-------------|
| **[Home](Home)** | Overview and navigation |
| **[Getting Started](Getting-Started)** | Installation, prerequisites, first run |
| **[Architecture](Architecture)** | System design, layers, data flow, module map |
| **[API Reference](API-Reference)** | All REST and WebSocket endpoints |
| **[Configuration](Configuration)** | Environment variables, feature flags |
| **[Security Model](Security-Model)** | Auth, sandboxing, token rotation, audit trail |
| **[Operations Manual](Operations-Manual)** | Production runbooks, backup, monitoring |
| **[Development Guide](Development-Guide)** | Local dev setup, contributing, code style |
| **[Testing Guide](Testing-Guide)** | Test suites, coverage, CI pipeline |
| **[Troubleshooting](Troubleshooting)** | Common issues, debug techniques |

---

## ⚡ Quick Start (TL;DR)

```bash
npm install
cp .env.example .env          # set NEURAL_TOKEN=your-secret-token
npm run engine                # terminal 1 — Rust engine on :8000
npm run dev                   # terminal 2 — React dashboard on :5173
```

Open `http://localhost:5173` · Health: `http://127.0.0.1:8000/v1/engine/health`

## 🗺️ System At A Glance

```
┌──────────────────────────────────────────────────────────────┐
│               React Dashboard  (port 5173)                    │
│     TypeScript · Vite · Zustand · React Router · Tailwind    │
└──────────────────────────┬───────────────────────────────────┘
                           │ HTTP/WS  Bearer token
┌──────────────────────────▼───────────────────────────────────┐
│              Rust Axum Engine  (port 8000)                    │
│  /v1 Routes · AppState Hubs · Workers · MCP Host             │
│  SQLite via sqlx migrations · System Actors                   │
└──────────────────────────┬───────────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────────┐
│          Python Execution Layer  (execution/)                 │
│  Skills · MCP server · Backup · Token Rotation · Audit       │
└──────────────────────────────────────────────────────────────┘
```

## 🏗️ Core Design Principles

1. **Local-first** — No cloud control plane required
2. **3-Layer Architecture** — Directives → Orchestration (Rust) → Execution (Python)
3. **Governed Execution** — Budget guards, approval queues, and audit trails on every action
4. **Sovereign Security** — Zero-trust auth, shell scanning, constant-time token comparison
5. **Self-Annealing** — Errors update directives; the system grows stronger from every failure

---
*Last updated: 2026-06-26 · v1.1.59 Agentic Engine P0 Release*

[//]: # (Metadata: [Home])
