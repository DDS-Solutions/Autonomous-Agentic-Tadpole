> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Core**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Search `[tauri_readme]` in audit logs.
>
> ### AI Assist Note
> Technical overview and initialization guidelines for the optional Tauri v2 desktop shell.
>
> ### 🔍 Debugging & Observability
> Traceability via `parity_guard.py`.

# Tauri Desktop Shell (Optional Target)

This directory contains configuration for building Tadpole OS as a native desktop application with [Tauri v2](https://v2.tauri.app/).

## Status

The primary runtime architecture for Tadpole OS is:
1. **Rust Engine (`server-rs`)**: Native Axum/Tokio HTTP & WebSocket control plane on `127.0.0.1:8000`.
2. **Web Dashboard (`src/`)**: React 19 / TypeScript SPA served directly by the Rust engine or Vite dev server.

Desktop containerization via Tauri is an **optional packaging target**. To initialize a full native Tauri desktop runner:

1. Scaffold the Tauri v2 Rust project harness:
   ```bash
   npx tauri init
   ```
2. Verify `tauri.conf.json` maps frontend distribution to `../dist`.
3. Run desktop dev:
   ```bash
   npm run tauri:dev
   ```
4. Build desktop release:
   ```bash
   npm run tauri:build
   ```

<!-- Telemetry Tag: [tauri_readme] -->
