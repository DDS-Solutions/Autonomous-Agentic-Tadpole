# 🗺️ TadpoleOS System Architecture Map
**Version**: 1.1.57 (Sovereign Kernel)
**Context**: This map bridges the gap between legacy architectural requests and the current Rust-based actor infrastructure.

---

## 📂 Mapping Table

| Requested Component | Actual System Path | Language | Purpose |
| :--- | :--- | :--- | :--- |
| **server-scripts** | `server-rs/src/` | Rust | Core business logic, Actor handlers, and Provider bridges. |
| **server-structure** | `server-rs/src/system/` | Rust | Kernel boot sequence, Orchestrator, and Lifecycle management. |
| **database** | `server-rs/migrations/` | SQL | Schema definitions and migration logic. |
| **requirements.txt** | `server-rs/Cargo.toml` | TOML | Backend dependency manifest. |
| **package.json** | `package.json` | JSON | Frontend and Swarm orchestration dependencies. |
| **domain-serialization** | `src/domain/` | TS | Frontend state and DTO serialization layers. |

---

## 🛠️ Mission Critical Files
1.  **Boot Entry**: `server-rs/src/main.rs`
2.  **Actor Loop**: `server-rs/src/system/actors/mod.rs`
3.  **Security Hub**: `server-rs/src/state/hubs/sec.rs`
4.  **Neural Bridge**: `server-rs/src/agent/openai.rs` (Ollama/Gemma 4 Connector)

## 📡 Database Access
- **Path**: `data/tadpole.db` (SQLite)
- **Primary Registry**: `data/agents.json`

---
**Directive**: Autonomous agents should prioritize auditing the `server-rs` directory for technical debt and `tokio` blocking operations.
