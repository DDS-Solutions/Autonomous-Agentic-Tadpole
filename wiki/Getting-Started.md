> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[Getting_Started]` in audit logs.
>
> ### AI Assist Note
> Core technical resource for the Tadpole OS Sovereign infrastructure.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# 🚀 Getting Started

## Prerequisites

| Requirement | Version | Purpose |
|-------------|---------|---------|
| **Rust toolchain** | 1.99.0+ (2021 edition) | Compile `server-rs` engine & workspace crates |
| **Node.js + npm** | LTS (≥22) | React dashboard + tooling |
| **Python** | 3.10+ | Execution scripts & Day-1 bootstrap (`execution/`) |
| **SQLite** | 3.35+ | Local database (bundled via sqlx) |
| **Ollama** *(optional)* | Latest | Local model inference |

---

## Installation

### 1. Clone the repository

```bash
git clone https://github.com/DDS-Solutions/Autonomous-Agentic-Tadpole.git
cd Autonomous-Agentic-Tadpole
```

### 2. Install Node dependencies

```bash
npm install
```

### 3. Bootstrap your environment config (Recommended: < 15s)

```bash
npm run init
# Non-interactive option:
python execution/tadpole_init.py --non-interactive --preset hybrid
```

This generates a cryptographically secure 64-hex `NEURAL_TOKEN`, probes local Ollama and Docker sandbox availability, and writes a hardened `.env` (never committed to git). To enable tiered RBAC (`Operator` vs `Admin` vs `ReadOnly`), pass `--tiered-tokens` or set `NEURAL_TOKEN_ADMIN` and `NEURAL_TOKEN_READONLY` in `.env`.

### 4. Start the Rust engine

```bash
npm run engine
# equivalent: cargo run --manifest-path server-rs/Cargo.toml
```

Engine starts on `http://127.0.0.1:8000` by default.

### 5. Start the React dashboard & complete the First-Run Wizard

Open a **second terminal**:

```bash
npm run dev
```

Dashboard opens at `http://localhost:5173` in **Focus Mode** (6 core views) with the 4-step **First-Run Overlord Wizard** to verify your memory-only session token, choose a Model Tier Preset, confirm safety guardrails, and run the native 4-probe Overlord Readiness Check (`POST /v1/benchmarks/readiness`). Press `Ctrl+Shift+E` anytime to switch to **Expert Mode** (all 18 views).

---

## Windows Shortcuts

Pre-built batch files are available for Windows users:

| File | Action |
|------|--------|
| `start_AA_tadpole.bat` | Start both engine and dashboard |
| `start_backend.bat` | Engine only |
| `start_frontend.bat` | Dashboard only |
| `stop_AA_tadpole.bat` | Stop all processes |

---

## Verify Installation

```bash
# Check engine health
curl http://127.0.0.1:8000/v1/engine/health

# Expected response
{
  "status": "healthy",
  "version": "1.1.59",
  "heartbeat": "2026-06-26T...",
  "database": { "status": "healthy", "pool_size": 5, ... },
  "uptime_seconds": 12
}
```

---

## Optional: Enable Heavy Features

Default builds are lightweight. Opt-in to heavy native features:

```bash
# Vector memory (LanceDB + Arrow)
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory

# Neural audio (Whisper)
cargo run --manifest-path server-rs/Cargo.toml --features neural-audio

# Both
cargo run --manifest-path server-rs/Cargo.toml --features vector-memory,neural-audio
```

---

## Optional: Production Build

```bash
# Build React dashboard into dist/
npm run build

# Engine automatically serves dist/ as SPA on the same port
npm run engine
```

Open `http://127.0.0.1:8000` — no separate Vite server needed.

---

## Python Execution Layer

Install Python dependencies (only required for execution scripts):

```bash
pip install -r execution/requirements.txt   # if present
# or per-script: pip install httpx python-dotenv
```

Test the environment:

```bash
python execution/verify_ai_context.py
python execution/parity_guard.py
```

[//]: # (Metadata: [Getting_Started])
