# 🚀 Getting Started

## Prerequisites

| Requirement | Version | Purpose |
|-------------|---------|---------|
| **Rust toolchain** | 1.75+ (2021 edition) | Compile `server-rs` engine |
| **Node.js + npm** | LTS (≥20) | React dashboard + tooling |
| **Python** | 3.10+ | Execution scripts in `execution/` |
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

### 3. Create your environment config

```bash
cp .env.example .env
```

Edit `.env` and set the required values:

```ini
# Required — master bearer token for all protected API routes
NEURAL_TOKEN=your-secret-token-here

# Optional — Ollama local inference
OLLAMA_HOST=http://localhost:11434

# Optional — Cloud provider keys
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
GOOGLE_API_KEY=AIza...
```

> **Security note:** Never commit `.env` to git. It is already listed in `.gitignore`.

### 4. Start the Rust engine

```bash
npm run engine
# equivalent: cargo run --manifest-path server-rs/Cargo.toml
```

Engine starts on `http://127.0.0.1:8000` by default.

### 5. Start the React dashboard

Open a **second terminal**:

```bash
npm run dev
```

Dashboard opens at `http://localhost:5173`.

---

## Windows Shortcuts

Pre-built batch files are available for Windows users:

| File | Action |
|------|--------|
| `start_AA_tadpole.bat` | Start both engine and dashboard |
| `start_backend.bat` | Engine only |
| `start_frontend.bat` | Dashboard only |
| `stop_AAtadpole.bat` | Stop all processes |

---

## Verify Installation

```bash
# Check engine health
curl http://127.0.0.1:8000/v1/engine/health

# Expected response
{
  "status": "healthy",
  "version": "1.1.58",
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
pip install -r requirements.txt   # if present
# or per-script: pip install httpx python-dotenv
```

Test the environment:

```bash
python execution/verify_ai_context.py
python execution/parity_guard.py
```
