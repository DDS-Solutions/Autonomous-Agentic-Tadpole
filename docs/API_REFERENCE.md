# Tadpole OS API Reference

> [!IMPORTANT]
> **AI Assist Note (Knowledge Heritage)**:
> This document is part of the "Sovereign Reality" documentation.
> - **@docs ARCHITECTURE:Documentation**
> - **Failure Path**: Information drift, legacy terminology, or documentation mismatch.
> - **Telemetry Link**: Cross-reference with `execution/parity_guard.py` results.
>
> ### AI Assist Note
> API reference generated from `server-rs/src/router.rs`.
>
> ### Debugging & Observability
> Traceability via `execution/parity_guard.py`.

**Version**: 1.1.57
**Source of truth**: `server-rs/src/router.rs`

The Rust engine binds to `127.0.0.1:8000` by default and nests application routes under `/v1`.

## Authentication

Public routes:

- `GET /v1/engine/health`
- `GET /v1/engine/live-voice`
- `GET /v1/engine/ws`

Protected routes require:

```http
Authorization: Bearer <NEURAL_TOKEN>
```

## Agents

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/agents` | `routes::agent::get_agents` | Protected |
| `POST` | `/v1/agents` | `routes::agent::create_agent` | Protected |
| `GET` | `/v1/agents/graph` | `routes::agent::get_swarm_graph_handler` | Protected |
| `GET` | `/v1/agents/{agent_id}/memories` | `routes::memory::get_agent_memory` | Protected; Requires Cargo feature vector-memory; otherwise returns 501. |
| `POST` | `/v1/agents/{agent_id}/memories` | `routes::memory::save_agent_memory` | Protected; Requires Cargo feature vector-memory; otherwise returns 501. |
| `DELETE` | `/v1/agents/{agent_id}/memories/{row_id}` | `routes::memory::delete_agent_memory` | Protected; Requires Cargo feature vector-memory; otherwise returns 501. |
| `PUT` | `/v1/agents/{id}` | `routes::agent::update_agent` | Protected |
| `POST` | `/v1/agents/{id}/mission` | `routes::agent::sync_mission` | Protected |
| `POST` | `/v1/agents/{id}/pause` | `routes::agent::pause_agent` | Protected |
| `POST` | `/v1/agents/{id}/reset` | `routes::agent::reset_agent` | Protected |
| `POST` | `/v1/agents/{id}/resume` | `routes::agent::resume_agent` | Protected |
| `POST` | `/v1/agents/{id}/tasks` | `routes::agent::send_task` | Protected |

## Benchmarks

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/benchmarks` | `routes::benchmarks::get_benchmarks` | Protected |
| `POST` | `/v1/benchmarks` | `routes::benchmarks::create_benchmark` | Protected |
| `POST` | `/v1/benchmarks/run/{test_id}` | `routes::benchmarks::trigger_benchmark` | Protected |
| `GET` | `/v1/benchmarks/{test_id}` | `routes::benchmarks::get_benchmark_history` | Protected |

## Continuity

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/continuity/jobs` | `routes::continuity::list_jobs_handler` | Protected |
| `POST` | `/v1/continuity/jobs` | `routes::continuity::create_job_handler` | Protected |
| `DELETE` | `/v1/continuity/jobs/{id}` | `routes::continuity::delete_job_handler` | Protected |
| `GET` | `/v1/continuity/jobs/{id}` | `routes::continuity::get_job_handler` | Protected |
| `PUT` | `/v1/continuity/jobs/{id}` | `routes::continuity::update_job_handler` | Protected |
| `POST` | `/v1/continuity/jobs/{id}/disable` | `routes::continuity::disable_job_handler` | Protected |
| `POST` | `/v1/continuity/jobs/{id}/enable` | `routes::continuity::enable_job_handler` | Protected |
| `GET` | `/v1/continuity/jobs/{id}/runs` | `routes::continuity::list_job_runs_handler` | Protected |
| `GET` | `/v1/continuity/workflows` | `routes::continuity::list_workflows_handler` | Protected |
| `POST` | `/v1/continuity/workflows` | `routes::continuity::create_workflow_handler` | Protected |
| `DELETE` | `/v1/continuity/workflows/{id}` | `routes::continuity::delete_workflow_handler` | Protected |
| `POST` | `/v1/continuity/workflows/{id}/steps` | `routes::continuity::add_workflow_step_handler` | Protected |

## Docs

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/docs/knowledge` | `routes::docs::list_knowledge_docs` | Protected |
| `GET` | `/v1/docs/knowledge/{category}/{name}` | `routes::docs::get_knowledge_doc` | Protected |
| `GET` | `/v1/docs/operations-manual` | `routes::docs::get_operations_manual` | Protected |

## Engine

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `POST` | `/v1/engine/deploy` | `routes::deploy::trigger_deploy` | Protected |
| `GET` | `/v1/engine/health` | `routes::health::health_check` | Public |
| `POST` | `/v1/engine/kill` | `routes::engine_control::kill_agents` | Protected |
| `GET` | `/v1/engine/live-voice` | `routes::ws::live_voice_handler` | Public |
| `POST` | `/v1/engine/shutdown` | `routes::engine_control::shutdown_engine` | Protected |
| `POST` | `/v1/engine/speak` | `routes::audio::text_to_speech` | Protected |
| `POST` | `/v1/engine/templates/install` | `routes::templates::install_template` | Protected |
| `POST` | `/v1/engine/transcribe` | `routes::audio::transcribe_audio` | Protected |
| `GET` | `/v1/engine/ws` | `routes::ws::ws_handler` | Public |

## Governance

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/governance/blueprints` | `routes::governance::list_blueprints` | Protected |
| `POST` | `/v1/governance/blueprints` | `routes::governance::save_blueprint` | Protected |
| `DELETE` | `/v1/governance/blueprints/{id}` | `routes::governance::delete_blueprint` | Protected |
| `GET` | `/v1/governance/manifest` | `routes::governance::get_sovereign_manifest` | Protected |

## Infra

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/infra/nodes` | `routes::nodes::get_nodes` | Protected |
| `POST` | `/v1/infra/nodes/discover` | `routes::nodes::discover_nodes` | Protected |

## Mcp

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `POST` | `/v1/mcp/message` | `crate::agent::mcp::transport::mcp_message_handler` | Protected |
| `GET` | `/v1/mcp/sse` | `crate::agent::mcp::transport::mcp_sse_handler` | Protected |

## Memory

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/search/memory` | `routes::memory::global_search` | Protected; Requires Cargo feature vector-memory; otherwise returns 501. |

## Model Manager

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `POST` | `/v1/api/pull` | `routes::model_manager::ollama_proxy_pull` | Protected |
| `GET` | `/v1/model-manager/model-store/catalog` | `routes::model_manager::get_model_catalog` | Protected |
| `POST` | `/v1/model-manager/model-store/pull` | `routes::model_manager::pull_model` | Protected |
| `GET` | `/v1/model-manager/models` | `routes::model_manager::get_models` | Protected |
| `DELETE` | `/v1/model-manager/models/{id}` | `routes::model_manager::delete_model` | Protected |
| `PUT` | `/v1/model-manager/models/{id}` | `routes::model_manager::update_model` | Protected |
| `GET` | `/v1/model-manager/providers` | `routes::model_manager::get_providers` | Protected |
| `DELETE` | `/v1/model-manager/providers/{id}` | `routes::model_manager::delete_provider` | Protected |
| `PUT` | `/v1/model-manager/providers/{id}` | `routes::model_manager::update_provider` | Protected |
| `POST` | `/v1/model-manager/providers/{id}/sync` | `routes::model_manager::sync_provider_models` | Protected |
| `POST` | `/v1/model-manager/providers/{id}/test` | `routes::model_manager::test_provider` | Protected |

## Oversight

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/oversight/ledger` | `routes::oversight::get_ledger` | Protected |
| `GET` | `/v1/oversight/pending` | `routes::oversight::get_pending` | Protected |
| `GET` | `/v1/oversight/security/audit-trail` | `routes::oversight::get_audit_trail` | Protected |
| `GET` | `/v1/oversight/security/health` | `routes::oversight::get_agent_health` | Protected |
| `GET` | `/v1/oversight/security/integrity` | `routes::oversight::get_integrity_status` | Protected |
| `GET` | `/v1/oversight/security/missions/quotas` | `routes::oversight::get_mission_quotas` | Protected |
| `PUT` | `/v1/oversight/security/missions/{id}/quota` | `routes::oversight::update_mission_quota` | Protected |
| `GET` | `/v1/oversight/security/policies` | `routes::oversight::get_policies` | Protected |
| `PUT` | `/v1/oversight/security/policies` | `routes::oversight::update_policy` | Protected |
| `GET` | `/v1/oversight/security/quotas` | `routes::oversight::get_security_quotas` | Protected |
| `PUT` | `/v1/oversight/security/quotas/{entity_id}` | `routes::oversight::update_agent_quota` | Protected |
| `PUT` | `/v1/oversight/settings` | `routes::oversight::update_settings` | Protected |
| `POST` | `/v1/oversight/{id}/decide` | `routes::oversight::decide_oversight` | Protected |

## Skills

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/skills` | `routes::skills::list_all_skills` | Protected |
| `DELETE` | `/v1/skills/hooks/{name}` | `routes::skills::delete_hook` | Protected |
| `PUT` | `/v1/skills/hooks/{name}` | `routes::skills::post_hook` | Protected |
| `POST` | `/v1/skills/import` | `routes::skills::import_capability` | Protected |
| `GET` | `/v1/skills/manifests` | `routes::skills::list_manifests` | Protected |
| `GET` | `/v1/skills/manifests/{name}` | `routes::skills::get_manifest` | Protected |
| `GET` | `/v1/skills/mcp-tools` | `routes::mcp::list_mcp_tools` | Protected |
| `POST` | `/v1/skills/mcp-tools/{name}/execute` | `routes::mcp::execute_mcp_tool` | Protected |
| `POST` | `/v1/skills/promote` | `routes::skills::promote_artifact` | Protected |
| `GET` | `/v1/skills/proposals` | `routes::skills::list_capability_proposals` | Protected |
| `POST` | `/v1/skills/proposals/{id}/resolve` | `routes::skills::resolve_capability_proposal` | Protected |
| `POST` | `/v1/skills/register` | `routes::skills::register_capability` | Protected |
| `POST` | `/v1/skills/scan` | `routes::skills::scan_workspace_skills` | Protected |
| `DELETE` | `/v1/skills/scripts/{name}` | `routes::skills::delete_script` | Protected |
| `PUT` | `/v1/skills/scripts/{name}` | `routes::skills::post_script` | Protected |
| `DELETE` | `/v1/skills/workflows/{name}` | `routes::skills::delete_workflow` | Protected |
| `PUT` | `/v1/skills/workflows/{name}` | `routes::skills::post_workflow` | Protected |

## Sovereign

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/sovereign/missions/{mission_id}/leaves` | `routes::sovereign_state::get_mission_leaves` | Protected |
| `POST` | `/v1/sovereign/missions/{mission_id}/nodes` | `routes::sovereign_state::append_session_node` | Protected |
| `GET` | `/v1/sovereign/missions/{mission_id}/nodes/{leaf_id}/history` | `routes::sovereign_state::get_session_history` | Protected |
| `POST` | `/v1/sovereign/missions/{mission_id}/nodes/{node_id}/revert` | `routes::sovereign_state::revert_to_node` | Protected |

## System

| Method | Path | Handler | Notes |
| --- | --- | --- | --- |
| `GET` | `/v1/env-schema` | `routes::env_schema::get_env_schema` | Protected |
| `GET` | `/v1/system/compute-profile` | `routes::system::get_compute_profile` | Protected |

[//]: # (Metadata: [API_REFERENCE])
