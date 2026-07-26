//! @docs ARCHITECTURE:Infrastructure
//! 
//! ### AI Assist Note
//! **Core technical resource for the Tadpole OS Sovereign infrastructure.**
//! This module implements high-fidelity logic for the Sovereign Reality layer.
//! 
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Runtime logic error, state desynchronization, or resource exhaustion.
//! - **Telemetry Link**: Search `[okf_gate]` in tracing logs.

use crate::error::AppError;
use sqlx::SqlitePool;
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OkfNodeInfo {
    pub id: String,
    pub title: String,
    pub concept_type: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OkfValidationResult {
    pub status: String, // "nominal" | "warning" | "critical"
    pub message: Option<String>,
}

/// Query active OKF playbooks associated with a cluster.
pub async fn get_mounted_playbooks(
    pool: &SqlitePool,
    cluster_id: &str,
) -> Result<Vec<OkfNodeInfo>, AppError> {
    let rows = sqlx::query(
        "SELECT id, title, concept_type FROM knowledge_store_meta WHERE cluster_id = ? AND concept_type = 'playbook'"
    )
    .bind(cluster_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[okf_gate] Failed to query playbooks: {}", e)))?;

    let mut playbooks = Vec::new();
    for row in rows {
        use sqlx::Row;
        let id: String = row.try_get("id")?;
        let title_opt: Option<String> = row.try_get("title")?;
        let concept_type: String = row.try_get("concept_type")?;
        let title = title_opt.unwrap_or_else(|| id.clone());
        playbooks.push(OkfNodeInfo { id, title, concept_type });
    }

    Ok(playbooks)
}

/// Validate the host environments against the requirements of the mounted playbooks.
pub async fn validate_environments(
    pool: &SqlitePool,
    cluster_id: &str,
    detected_envs: &[String],
) -> Result<OkfValidationResult, AppError> {
    let playbooks = get_mounted_playbooks(pool, cluster_id).await?;
    if playbooks.is_empty() {
        return Ok(OkfValidationResult {
            status: "nominal".to_string(),
            message: None,
        });
    }

    let rows = sqlx::query(
        "SELECT text, constraints_json FROM knowledge_store_meta WHERE cluster_id = ? AND concept_type = 'playbook'"
    )
    .bind(cluster_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[okf_gate] Failed to query playbook text: {}", e)))?;

    for row in rows {
        use sqlx::Row;
        let text: String = row.try_get("text")?;
        let constraints_opt: Option<String> = row.try_get("constraints_json").ok().flatten();
        let text_lower = text.to_lowercase();

        // Check structured JSON constraints if present
        if let Some(json_str) = constraints_opt {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(req_envs) = parsed.get("required_envs").and_then(|v| v.as_array()) {
                    for env_val in req_envs {
                        if let Some(env_str) = env_val.as_str() {
                            if !detected_envs.contains(&env_str.to_string()) {
                                return Ok(OkfValidationResult {
                                    status: "warning".to_string(),
                                    message: Some(format!("OKF Gate: Mounted playbook requires environment '{}' which was not detected. Awaiting HITL oversight approval before execution.", env_str)),
                                });
                            }
                        }
                    }
                }
            }
        }

        if (text_lower.contains("requires: docker") || text_lower.contains("requires docker"))
            && !detected_envs.contains(&"docker".to_string())
        {
            return Ok(OkfValidationResult {
                status: "warning".to_string(),
                message: Some("Active OKF playbooks require a Docker container sandbox, but no Docker daemon was detected on the host. Non-blocking warning: await HITL prompt.".to_string()),
            });
        }

        if (text_lower.contains("requires: k8s_node") || text_lower.contains("requires k8s"))
            && !detected_envs.contains(&"k8s_node".to_string())
        {
            return Ok(OkfValidationResult {
                status: "critical".to_string(),
                message: Some("Critical: OKF playbooks require a Kubernetes cluster environment, but the application is running outside Kubernetes. Non-blocking warning: human-in-the-loop oversight prompt triggered.".to_string()),
            });
        }

        if (text_lower.contains("requires: jupyter_lab") || text_lower.contains("requires jupyter"))
            && !detected_envs.contains(&"jupyter_lab".to_string())
        {
            return Ok(OkfValidationResult {
                status: "warning".to_string(),
                message: Some("Active OKF playbooks require Jupyter Lab environment for data modeling, but jupyter was not found in PATH.".to_string()),
            });
        }

        if (text_lower.contains("requires: wasm_sandbox") || text_lower.contains("requires wasm"))
            && !detected_envs.contains(&"wasm_sandbox".to_string())
        {
            return Ok(OkfValidationResult {
                status: "warning".to_string(),
                message: Some("Active OKF playbooks require WebAssembly sandbox isolation, but wasm-codec was not detected.".to_string()),
            });
        }
    }

    Ok(OkfValidationResult {
        status: "nominal".to_string(),
        message: None,
    })
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaybookCachedDetail {
    pub id: String,
    pub title: String,
    pub concept_type: String,
    pub text: String,
}

pub async fn mount_playbooks_to_workspace(
    pool: &SqlitePool,
    cluster_id: &str,
    workspace_dir: &std::path::Path,
) -> Result<(), AppError> {
    let rows = sqlx::query(
        "SELECT id, title, concept_type, text FROM knowledge_store_meta WHERE cluster_id = ? AND concept_type = 'playbook'"
    )
    .bind(cluster_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("[okf_gate] Failed to query playbooks for mounting: {}", e)))?;

    let mut playbooks = Vec::new();
    for row in rows {
        use sqlx::Row;
        let id: String = row.try_get("id")?;
        let title_opt: Option<String> = row.try_get("title")?;
        let concept_type: String = row.try_get("concept_type")?;
        let text: String = row.try_get("text")?;
        let title = title_opt.unwrap_or_else(|| id.clone());
        playbooks.push(PlaybookCachedDetail { id, title, concept_type, text });
    }

    if workspace_dir.exists() {
        let json_path = workspace_dir.join("playbooks.json");
        let content = serde_json::to_string_pretty(&playbooks)
            .map_err(|e| AppError::InternalServerError(format!("[okf_gate] Failed to serialize playbooks: {}", e)))?;
        
        tokio::fs::write(&json_path, content).await
            .map_err(|e| AppError::InternalServerError(format!("[okf_gate] Failed to write playbooks.json: {}", e)))?;
        tracing::info!("✅ [okf_gate] Mounted {} playbooks to {}", playbooks.len(), json_path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query(
            r#"CREATE TABLE knowledge_store_meta (
                id TEXT PRIMARY KEY,
                text TEXT NOT NULL DEFAULT '',
                concept_type TEXT NOT NULL DEFAULT 'general',
                cluster_id TEXT,
                title TEXT,
                description TEXT,
                resource_uri TEXT,
                tags TEXT,
                constraints_json TEXT,
                provenance_chain TEXT
            )"#
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_okf_validation_nominal_empty() {
        let pool = setup_test_db().await;
        let res = validate_environments(&pool, "cl-test", &[]).await.unwrap();
        assert_eq!(res.status, "nominal");
        assert!(res.message.is_none());
    }

    #[tokio::test]
    async fn test_okf_validation_missing_docker() {
        let pool = setup_test_db().await;
        
        sqlx::query(
            r#"INSERT INTO knowledge_store_meta (id, text, concept_type, cluster_id, title)
               VALUES ('pb-1', 'This playbook requires docker for isolation.', 'playbook', 'cl-test', 'Docker Playbook')"#
        )
        .execute(&pool)
        .await
        .unwrap();

        let res = validate_environments(&pool, "cl-test", &["vs_code".to_string()]).await.unwrap();
        assert_eq!(res.status, "warning");
        assert!(res.message.unwrap().contains("Docker daemon"));

        let res = validate_environments(&pool, "cl-test", &["docker".to_string()]).await.unwrap();
        assert_eq!(res.status, "nominal");
    }

    #[tokio::test]
    async fn test_okf_validation_missing_k8s() {
        let pool = setup_test_db().await;

        sqlx::query(
            r#"INSERT INTO knowledge_store_meta (id, text, concept_type, cluster_id, title)
               VALUES ('pb-2', 'Operational step: requires k8s_node for orchestrations.', 'playbook', 'cl-test', 'K8s Playbook')"#
        )
        .execute(&pool)
        .await
        .unwrap();

        let res = validate_environments(&pool, "cl-test", &["docker".to_string()]).await.unwrap();
        assert_eq!(res.status, "critical");
        assert!(res.message.unwrap().contains("Kubernetes"));

        let res = validate_environments(&pool, "cl-test", &["k8s_node".to_string()]).await.unwrap();
        assert_eq!(res.status, "nominal");
    }
}
