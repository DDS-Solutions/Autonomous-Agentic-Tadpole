//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **Capability Hub**: Unified endpoint for all agent abilities, including
//! **Tool Manifests** (JSON schemas), **Executable Scripts**, and
//! **Multi-step Workflows**. Features a **Multipart Capability Importer**
//! that autonomously parses Markdown frontmatter and script extensions
//! (ps1, sh, py) to register new agentic skills.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[skills]` in tracing logs.
//! - **Failure Path**: 400 Bad Request on invalid multipart boundaries,
//!   500 Internal Server Error during script persistence to disk, or
//!   silent failure of YAML frontmatter parsing in imported skills.
//! - **Trace Scope**: `server-rs::routes::skills`

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::agent::script_skills::{HookDefinition, SkillDefinition, WorkflowDefinition};
use crate::agent::skill_manifest::SkillManifest;
use crate::error::AppError;
use crate::state::AppState;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityType {
    Script,
    Workflow,
    Unknown,
}

impl From<&std::path::Path> for CapabilityType {
    fn from(path: &std::path::Path) -> Self {
        match path.extension().and_then(|s| s.to_str()) {
            Some("ps1") | Some("sh") | Some("py") | Some("bat") | Some("exe") => Self::Script,
            Some("yaml") | Some("yml") => Self::Workflow,
            Some("md") => Self::Unknown, // Requires content parsing
            _ => Self::Unknown,
        }
    }
}

/// GET /v1/skills
///
/// Unified endpoint for all agent abilities:
/// - Manifests: JSON schemas for LLM tool-calling.
/// - Scripts: Executable file-based skills.
/// - Workflows: Multi-step passive sequences.
#[tracing::instrument(skip(state), name = "capability_registry::get_all")]
pub async fn list_all_skills(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mut manifests = Vec::new();
    let mut scripts = Vec::new();
    let mut workflows = Vec::new();
    let mut hooks = Vec::new();

    let snapshot = state.registry.skills.snapshot();

    // 1. Collect Manifests
    for kv in state.registry.skill_registry.manifests.iter() {
        manifests.push(kv.value().clone());
    }

    // 2. Collect Script Skills
    for entry in snapshot.skills.iter() {
        scripts.push(entry.value().clone());
    }

    // 3. Collect Workflows
    for entry in snapshot.workflows.iter() {
        workflows.push(entry.value().clone());
    }

    // 4. Collect Hooks
    for entry in snapshot.hooks.iter() {
        hooks.push(entry.value().clone());
    }

    Ok(Json(json!({
        "status": "success",
        "manifests": manifests,
        "scripts": scripts,
        "workflows": workflows,
        "hooks": hooks
    })))
}

/// GET /v1/skills/manifests
/// Legacy/Narrow endpoint for just manifests (compatible with old MissionApiService GET /v1/skills)
#[tracing::instrument(skip(state), name = "capability_registry::list_manifests")]
pub async fn list_manifests(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mut manifests: Vec<SkillManifest> = state
        .registry
        .skill_registry
        .manifests
        .iter()
        .map(|kv| kv.value().clone())
        .collect();
    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(manifests))
}

/// GET /v1/skills/manifests/:name
#[tracing::instrument(skip(state), fields(name = %name), name = "capability_registry::get_manifest")]
pub async fn get_manifest(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(manifest) = state.registry.skill_registry.get(&name) {
        Ok(Json(manifest.clone()))
    } else {
        Err(AppError::NotFound(format!(
            "Skill manifest '{}' not found",
            name
        )))
    }
}

/// POST /v1/skills/scan
///
/// Triggers an autonomous workspace scan to discover new capabilities (README.md, SKILL.md).
#[tracing::instrument(skip(state), name = "capability_hub::scan")]
pub async fn scan_workspace_skills(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let count = state.scan_workspace_skills_sovereign().await?;
    
    state.broadcast_sys(
        &format!("Autonomous scan complete. Ingested {} new capabilities.", count),
        "success",
        None,
    );

    Ok(Json(json!({
        "status": "success",
        "ingested_count": count
    })))
}

/// PUT /v1/skills/scripts/{name}
#[tracing::instrument(skip(state, payload), fields(name = %name), name = "capability_registry::save_script")]
pub async fn post_script(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(mut payload): Json<SkillDefinition>,
) -> Result<impl IntoResponse, AppError> {
    crate::agent::script_skills::parse::validate_capability_name(&name)?;
    if !payload.name.is_empty() && payload.name != name {
        return Err(AppError::BadRequest(format!(
            "Path script name '{}' does not match payload name '{}'",
            name, payload.name
        )));
    }
    payload.name = name;
    state.registry.skills.save_skill(payload.clone()).await?;
    state.broadcast_sys(
        &format!("Updated skill script: {}", payload.name),
        "success",
        None,
    );
    Ok((StatusCode::OK, Json(json!({ "status": "saved" }))))
}

/// DELETE /v1/skills/scripts/:name
#[tracing::instrument(skip(state), fields(name = %name), name = "capability_registry::delete_script")]
pub async fn delete_script(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    crate::agent::script_skills::parse::validate_capability_name(&name)?;
    state.registry.skills.delete_skill(&name).await?;
    state.broadcast_sys(&format!("Deleted script: {}", name), "warning", None);
    Ok((StatusCode::OK, Json(json!({ "status": "deleted" }))))
}

/// PUT /v1/skills/workflows/{name}
#[tracing::instrument(skip(state, payload), fields(name = %name), name = "capability_registry::save_workflow")]
pub async fn post_workflow(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(mut payload): Json<WorkflowDefinition>,
) -> Result<impl IntoResponse, AppError> {
    crate::agent::script_skills::parse::validate_capability_name(&name)?;
    if !payload.name.is_empty() && payload.name != name {
        return Err(AppError::BadRequest(format!(
            "Path workflow name '{}' does not match payload name '{}'",
            name, payload.name
        )));
    }
    payload.name = name;
    state.registry.skills.save_workflow(payload.clone()).await?;
    state.broadcast_sys(
        &format!("Updated workflow: {}", payload.name),
        "success",
        None,
    );
    Ok((StatusCode::OK, Json(json!({ "status": "saved" }))))
}

/// DELETE /v1/skills/workflows/:name
#[tracing::instrument(skip(state), fields(name = %name), name = "capability_registry::delete_workflow")]
pub async fn delete_workflow(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    crate::agent::script_skills::parse::validate_capability_name(&name)?;
    state.registry.skills.delete_workflow(&name).await?;
    state.broadcast_sys(&format!("Deleted workflow: {}", name), "warning", None);
    Ok((StatusCode::OK, Json(json!({ "status": "deleted" }))))
}

/// PUT /v1/skills/hooks/{name}
#[tracing::instrument(skip(state, payload), fields(name = %name), name = "capability_registry::save_hook")]
pub async fn post_hook(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(mut payload): Json<HookDefinition>,
) -> Result<impl IntoResponse, AppError> {
    crate::agent::script_skills::parse::validate_capability_name(&name)?;
    if !payload.name.is_empty() && payload.name != name {
        return Err(AppError::BadRequest(format!(
            "Path hook name '{}' does not match payload name '{}'",
            name, payload.name
        )));
    }
    payload.name = name;
    state.registry.skills.save_hook(payload.clone()).await?;
    state.broadcast_sys(
        &format!("Updated lifecycle hook: {}", payload.name),
        "success",
        None,
    );
    Ok((StatusCode::OK, Json(json!({ "status": "saved" }))))
}

/// DELETE /v1/skills/hooks/:name
#[tracing::instrument(skip(state), fields(name = %name), name = "capability_registry::delete_hook")]
pub async fn delete_hook(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    crate::agent::script_skills::parse::validate_capability_name(&name)?;
    state.registry.skills.delete_hook(&name).await?;
    state.broadcast_sys(
        &format!("Deleted lifecycle hook: {}", name),
        "warning",
        None,
    );
    Ok((StatusCode::OK, Json(json!({ "status": "deleted" }))))
}

/// POST /v1/skills/import
/// Accepts a markdown file or script, parses it, and returns a structured preview.
#[tracing::instrument(skip(_state, multipart), name = "capability_hub::import")]
pub async fn import_capability(
    State(_state): State<Arc<AppState>>,
    mut multipart: axum::extract::Multipart,
) -> Result<impl IntoResponse, AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("file").to_string();
        let file_name = field.file_name().unwrap_or("unknown.md").to_string();
        let content = field
            .text()
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to read field: {}", e)))?;

        if name == "file" {
            let path = std::path::Path::new(&file_name);
            let cap_type = CapabilityType::from(path);

            if file_name.ends_with(".md") {
                // Parse as Skill or Workflow
                if let Some(skill) = crate::agent::script_skills::parse_skill_md(&content) {
                    return Ok(Json(json!({
                        "type": "skill",
                        "data": skill,
                        "preview": content
                    })));
                } else {
                    // Treat as basic Workflow
                    let workflow = WorkflowDefinition {
                        id: None,
                        name: file_name.trim_end_matches(".md").to_string(),
                        content: content.clone(),
                        doc_url: None,
                        tags: None,
                        category: "user".to_string(),
                    };
                    return Ok(Json(json!({
                        "type": "workflow",
                        "data": workflow,
                        "preview": content
                    })));
                }
            } else if cap_type == CapabilityType::Script {
                let hook = HookDefinition {
                    name: file_name.clone(),
                    description: format!("Imported script from {}", file_name),
                    hook_type: "post-tool".to_string(), // Default
                    content: content.clone(),
                    active: true,
                    category: "user".to_string(),
                };
                return Ok(Json(json!({
                    "type": "hook", // Scripts imported as hooks for lifecycle integration
                    "data": hook,
                    "preview": content
                })));
            }
        }
    }

    Err(AppError::BadRequest(
        "No valid file found in multipart request".to_string(),
    ))
}

#[derive(serde::Deserialize, Debug)]
pub struct RegisterPayload {
    pub r#type: String,
    pub data: serde_json::Value,
    pub category: String,
}

/// POST /v1/skills/register
/// Finalizes registration of a parsed capability.
#[tracing::instrument(skip(state, payload), fields(capability_type = %payload.r#type), name = "capability_registry::register")]
pub async fn register_capability(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterPayload>,
) -> Result<impl IntoResponse, AppError> {
    match state
        .registry
        .skills
        .register_capability(&payload.r#type, payload.data, &payload.category)
        .await
    {
        Ok(name) => {
            state.broadcast_sys(
                &format!("Registered new {}: {}", payload.r#type, name),
                "success",
                None,
            );
            Ok((
                StatusCode::OK,
                Json(json!({ "status": "registered", "name": name })),
            ))
        }
        Err(e) => Err(e),
    }
}

/// GET /v1/skills/proposals
///
/// Returns all pending capability proposals (skills, workflows, hooks) submitted by agents.
#[tracing::instrument(skip(state), name = "capability_registry::list_proposals")]
pub async fn list_capability_proposals(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query(
        "SELECT id, mission_id, agent_id, cap_type, name, description, content, created_at, status 
         FROM capability_proposals 
         WHERE status = 'pending' 
         ORDER BY created_at DESC",
    )
    .fetch_all(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    let proposals: Vec<serde_json::Value> = rows.into_iter().map(|row| {
        use sqlx::Row;
        serde_json::json!({
            "id": row.get::<String, _>("id"),
            "mission_id": row.get::<Option<String>, _>("mission_id"),
            "agent_id": row.get::<String, _>("agent_id"),
            "type": row.get::<String, _>("cap_type"),
            "name": row.get::<String, _>("name"),
            "description": row.get::<String, _>("description"),
            "payload": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("content")).unwrap_or_default(),
            "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "status": row.get::<String, _>("status"),
        })
    }).collect();

    Ok(Json(proposals))
}

#[derive(serde::Deserialize, Debug)]
pub struct ResolveProposalPayload {
    pub decision: String, // "approved" or "rejected"
}

/// POST /v1/skills/proposals/:id/resolve
///
/// Finalizes a capability proposal. If approved, it is physically written to the agent_generated directory.
#[tracing::instrument(skip(state, payload), fields(proposal_id = %proposal_id, decision = %payload.decision), name = "capability_registry::resolve_proposal")]
pub async fn resolve_capability_proposal(
    Path(proposal_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ResolveProposalPayload>,
) -> Result<impl IntoResponse, AppError> {
    let approved = match payload.decision.as_str() {
        "approved" => true,
        "rejected" => false,
        other => {
            return Err(AppError::BadRequest(format!(
                "Invalid decision '{}': expected 'approved' or 'rejected'.",
                other
            )));
        }
    };

    // 1. Fetch proposal & verify pending state
    let row: (String, String, String, String, String) = sqlx::query_as(
        "SELECT cap_type, name, content, agent_id, status FROM capability_proposals WHERE id = ?",
    )
    .bind(&proposal_id)
    .fetch_one(&state.resources.pool)
    .await
    .map_err(|_| AppError::NotFound("Proposal not found".to_string()))?;

    let (cap_type, name, payload_str, _agent_id, status) = row;
    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Proposal '{}' is already resolved (status: '{}')",
            proposal_id, status
        )));
    }

    let data: serde_json::Value = serde_json::from_str(&payload_str)
        .map_err(|e| AppError::BadRequest(format!("Invalid proposal payload JSON: {}", e)))?;

    // 2. Claim the transition atomically via OCC (`AND status = 'pending'`) before mutating filesystem
    let update_res = sqlx::query(
        "UPDATE capability_proposals SET status = ? WHERE id = ? AND status = 'pending'",
    )
    .bind(if approved { "approved" } else { "rejected" })
    .bind(&proposal_id)
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;
    if update_res.rows_affected() == 0 {
        return Err(AppError::Conflict(format!(
            "Proposal '{}' was concurrently resolved by another operator",
            proposal_id
        )));
    }

    // 3. If approved, physically register it (reverting DB status to pending if registration fails)
    if approved {
        if let Err(e) = state
            .registry
            .skills
            .register_capability(&cap_type, data, "ai")
            .await
        {
            let _ = sqlx::query("UPDATE capability_proposals SET status = 'pending' WHERE id = ?")
                .bind(&proposal_id)
                .execute(&state.resources.pool)
                .await;
            return Err(e);
        }

        state.broadcast_sys(
            &format!("✅ Autonomous Capability Approved: {} ({})", name, cap_type),
            "success",
            None,
        );
    }

    Ok(Json(
        json!({ "status": "ok", "decision": payload.decision }),
    ))
}

#[derive(serde::Deserialize, Debug)]
pub struct PromotePayload {
    pub name: String,
    pub description: String,
    pub cap_type: String, // 'skill', 'workflow', 'hook'
    pub content: String,
    pub agent_id: String,
    pub mission_id: Option<String>,
}

/// POST /v1/skills/promote
///
/// Promotes an artifact (agent-generated code) to a formal capability proposal.
#[tracing::instrument(skip(state, payload), fields(name = %payload.name), name = "capability_registry::promote")]
pub async fn promote_artifact(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PromotePayload>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.is_empty()
        || payload.name.len() > 64
        || !payload.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::BadRequest(
            "Capability name must be 1-64 characters and contain only alphanumeric characters, underscores, or hyphens."
                .to_string(),
        ));
    }

    if !matches!(payload.cap_type.as_str(), "skill" | "workflow" | "hook") {
        return Err(AppError::BadRequest(format!(
            "Unsupported capability type '{}': expected 'skill', 'workflow', or 'hook'.",
            payload.cap_type
        )));
    }

    let proposal_id = uuid::Uuid::new_v4().to_string();

    // Ensure every promoted proposal persists a valid JSON payload consumable by resolve_capability_proposal.
    let final_payload = match payload.cap_type.as_str() {
        "skill" => {
            let skill = SkillDefinition {
                name: payload.name.clone(),
                description: payload.description.clone(),
                execution_command: format!("python agent_generated/{}.py", payload.name),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "args": { "type": "object" }
                    }
                }),
                category: "ai".to_string(),
                oversight_required: true,
                doc_url: None,
                tags: None,
                full_instructions: (!payload.content.trim().is_empty())
                    .then(|| payload.content.clone()),
                negative_constraints: None,
                verification_script: None,
                id: None,
            };
            serde_json::to_string(&skill).map_err(|e| {
                AppError::InternalServerError(format!("Failed to serialize skill proposal: {}", e))
            })?
        }
        "workflow" => {
            let workflow = match serde_json::from_str::<serde_json::Value>(&payload.content) {
                Ok(serde_json::Value::Object(mut obj)) => {
                    obj.insert("name".to_string(), json!(payload.name));
                    obj.entry("category".to_string())
                        .or_insert_with(|| json!("ai"));
                    serde_json::from_value::<WorkflowDefinition>(serde_json::Value::Object(obj))
                        .unwrap_or_else(|_| WorkflowDefinition {
                            id: None,
                            name: payload.name.clone(),
                            content: payload.content.clone(),
                            doc_url: None,
                            tags: None,
                            category: "ai".to_string(),
                        })
                }
                _ => WorkflowDefinition {
                    id: None,
                    name: payload.name.clone(),
                    content: payload.content.clone(),
                    doc_url: None,
                    tags: None,
                    category: "ai".to_string(),
                },
            };
            serde_json::to_string(&workflow).map_err(|e| {
                AppError::InternalServerError(format!("Failed to serialize workflow proposal: {}", e))
            })?
        }
        "hook" => {
            let hook = match serde_json::from_str::<serde_json::Value>(&payload.content) {
                Ok(serde_json::Value::Object(mut obj)) => {
                    obj.insert("name".to_string(), json!(payload.name));
                    obj.entry("description".to_string())
                        .or_insert_with(|| json!(payload.description));
                    obj.entry("hook_type".to_string())
                        .or_insert_with(|| json!("post_analysis"));
                    obj.entry("active".to_string()).or_insert_with(|| json!(true));
                    obj.entry("category".to_string())
                        .or_insert_with(|| json!("ai"));
                    serde_json::from_value::<HookDefinition>(serde_json::Value::Object(obj))
                        .unwrap_or_else(|_| HookDefinition {
                            name: payload.name.clone(),
                            description: payload.description.clone(),
                            hook_type: "post_analysis".to_string(),
                            content: payload.content.clone(),
                            active: true,
                            category: "ai".to_string(),
                        })
                }
                _ => HookDefinition {
                    name: payload.name.clone(),
                    description: payload.description.clone(),
                    hook_type: "post_analysis".to_string(),
                    content: payload.content.clone(),
                    active: true,
                    category: "ai".to_string(),
                },
            };
            serde_json::to_string(&hook).map_err(|e| {
                AppError::InternalServerError(format!("Failed to serialize hook proposal: {}", e))
            })?
        }
        _ => unreachable!(),
    };

    sqlx::query(
        "INSERT INTO capability_proposals (id, name, description, cap_type, content, agent_id, mission_id, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')"
    )
    .bind(&proposal_id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.cap_type)
    .bind(&final_payload)
    .bind(&payload.agent_id)
    .bind(payload.mission_id.as_deref())
    .execute(&state.resources.pool)
    .await
    .map_err(AppError::Sqlx)?;

    state.broadcast_sys(
        &format!("🚀 New Capability Proposal: {} by Agent {}", payload.name, payload.agent_id),
        "info",
        payload.mission_id.clone(),
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({ 
            "status": "proposed", 
            "id": proposal_id,
            "message": "Artifact promoted to proposal. Awaiting human oversight."
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capability_proposal_lifecycle() {
        let state = Arc::new(AppState::new_mock().await);

        // 1. Promote an artifact
        let promote_payload = PromotePayload {
            name: "test_auto_skill".to_string(),
            description: "An automated skill created by an agent".to_string(),
            cap_type: "skill".to_string(),
            content: "print('hello from proposed skill')".to_string(),
            agent_id: "agent-001".to_string(),
            mission_id: Some("mission-alpha".to_string()),
        };

        let promote_res = promote_artifact(
            axum::extract::State(state.clone()),
            Json(promote_payload),
        ).await;
        assert!(promote_res.is_ok(), "Promote artifact must succeed");

        // 2. List pending proposals
        let list_res = list_capability_proposals(
            axum::extract::State(state.clone()),
        ).await;
        assert!(list_res.is_ok(), "List proposals must succeed without SQL or type decode error");

        // 3. Query proposals from DB to verify id is string UUID and content is stored
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT id, name, status FROM capability_proposals WHERE name = 'test_auto_skill'",
        )
        .fetch_optional(&state.resources.pool)
        .await
        .unwrap();

        assert!(row.is_some(), "Proposal must exist in database");
        let (proposal_id, name, status) = row.unwrap();
        assert_eq!(name, "test_auto_skill");
        assert_eq!(status, "pending");

        // 4. Resolve proposal (Approve)
        let resolve_res = resolve_capability_proposal(
            axum::extract::Path(proposal_id.clone()),
            axum::extract::State(state.clone()),
            Json(ResolveProposalPayload {
                decision: "approved".to_string(),
            }),
        ).await;
        assert!(resolve_res.is_ok(), "Resolve capability proposal must succeed");

        // Verify status is approved
        let updated_status: (String,) = sqlx::query_as(
            "SELECT status FROM capability_proposals WHERE id = ?",
        )
        .bind(&proposal_id)
        .fetch_one(&state.resources.pool)
        .await
        .unwrap();

        assert_eq!(updated_status.0, "approved");
    }

    #[tokio::test]
    async fn test_put_script_rejects_path_body_name_mismatch() {
        let state = Arc::new(AppState::new_minimal_mock().await);
        let payload = SkillDefinition {
            id: None,
            name: "other_name".to_string(),
            description: "desc".to_string(),
            execution_command: "echo hi".to_string(),
            schema: serde_json::json!({}),
            oversight_required: true,
            doc_url: None,
            tags: None,
            full_instructions: None,
            negative_constraints: None,
            verification_script: None,
            category: "user".to_string(),
        };

        let res = post_script(
            axum::extract::Path("expected_name".to_string()),
            axum::extract::State(state.clone()),
            Json(payload),
        )
        .await;
        assert!(matches!(res, Err(AppError::BadRequest(_))));

        // Reject newline/frontmatter injection and traversal in Path(name)
        let bad_wf = WorkflowDefinition {
            id: None,
            name: "bad\n---\ninjected".to_string(),
            content: "hello".to_string(),
            doc_url: None,
            tags: None,
            category: "user".to_string(),
        };
        let wf_res = post_workflow(
            axum::extract::Path("bad\n---\ninjected".to_string()),
            axum::extract::State(state.clone()),
            Json(bad_wf),
        )
        .await;
        assert!(matches!(wf_res, Err(AppError::BadRequest(_))));

        // DELETE nonexistent skill returns 404 NotFound (not 200 OK)
        let del_res = delete_script(
            axum::extract::Path("definitely_missing_skill_xyz".to_string()),
            axum::extract::State(state.clone()),
        )
        .await;
        assert!(matches!(del_res, Err(AppError::NotFound(_))));

        // DELETE traversal name returns 400 BadRequest
        let trav_res = delete_script(
            axum::extract::Path("../outside".to_string()),
            axum::extract::State(state),
        )
        .await;
        assert!(matches!(trav_res, Err(AppError::BadRequest(_))));
    }

    #[tokio::test]
    async fn test_promote_and_resolve_hook_and_primitive_json_proposals() {
        let state = Arc::new(AppState::new_mock().await);

        // (a) Plain-text hook proposal -> promote includes default hook_type/active -> resolve succeeds (200) -> second resolve returns 409 Conflict
        let hook_name = "test_pass3_plain_hook";
        let promote_hook = promote_artifact(
            axum::extract::State(state.clone()),
            Json(PromotePayload {
                name: hook_name.to_string(),
                description: "Plain text hook".to_string(),
                cap_type: "hook".to_string(),
                content: "echo post-analysis-ok".to_string(),
                agent_id: "agent-001".to_string(),
                mission_id: Some("mission-p3".to_string()),
            }),
        )
        .await;
        assert!(promote_hook.is_ok(), "Plain-text hook promotion must succeed");

        let (hook_id,): (String,) = sqlx::query_as(
            "SELECT id FROM capability_proposals WHERE name = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(hook_name)
        .fetch_one(&state.resources.pool)
        .await
        .unwrap();

        let first_resolve = resolve_capability_proposal(
            axum::extract::Path(hook_id.clone()),
            axum::extract::State(state.clone()),
            Json(ResolveProposalPayload {
                decision: "approved".to_string(),
            }),
        )
        .await;
        assert!(
            first_resolve.is_ok(),
            "Approving plain-text promoted hook must succeed with 200 OK, got: {:?}",
            first_resolve.err()
        );

        let second_resolve = resolve_capability_proposal(
            axum::extract::Path(hook_id),
            axum::extract::State(state.clone()),
            Json(ResolveProposalPayload {
                decision: "approved".to_string(),
            }),
        )
        .await;
        assert!(
            matches!(second_resolve, Err(AppError::Conflict(_))),
            "Re-resolving an already-resolved proposal must fail with 409 Conflict"
        );

        // (b) Edge case: bare JSON primitive `"123"` promoted as hook or workflow must wrap into valid JSON object and resolve cleanly
        let prim_wf_name = "test_pass3_prim_wf";
        let promote_prim = promote_artifact(
            axum::extract::State(state.clone()),
            Json(PromotePayload {
                name: prim_wf_name.to_string(),
                description: "Primitive JSON literal content".to_string(),
                cap_type: "workflow".to_string(),
                content: "123".to_string(),
                agent_id: "agent-001".to_string(),
                mission_id: None,
            }),
        )
        .await;
        assert!(promote_prim.is_ok());

        let (wf_id,): (String,) = sqlx::query_as(
            "SELECT id FROM capability_proposals WHERE name = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(prim_wf_name)
        .fetch_one(&state.resources.pool)
        .await
        .unwrap();

        let wf_resolve = resolve_capability_proposal(
            axum::extract::Path(wf_id),
            axum::extract::State(state.clone()),
            Json(ResolveProposalPayload {
                decision: "approved".to_string(),
            }),
        )
        .await;
        assert!(
            wf_resolve.is_ok(),
            "Primitive-JSON promoted workflow must resolve cleanly, got: {:?}",
            wf_resolve.err()
        );

        // Clean up generated test artifacts so the repository data/ dir remains pristine
        let _ = delete_hook(axum::extract::Path(hook_name.to_string()), axum::extract::State(state.clone())).await;
        let _ = delete_workflow(axum::extract::Path(prim_wf_name.to_string()), axum::extract::State(state)).await;
    }
}

// Metadata: [skills]
