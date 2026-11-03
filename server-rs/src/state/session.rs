//! @docs ARCHITECTURE:State
//!
//! ### AI Assist Note
//! **State Session Subsystem**: Manages recursive branch traversal, sovereign tree node appending,
//! and atomic time-travel history reversion across SQLite mission states.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[session]` in tracing logs.
//! - **Trace Scope**: `server-rs::state::session`

use crate::error::AppError;
use crate::state::AppState;

impl AppState {
    /// ### 🧠 State: Traverse Session History (traverse_session_history_sovereign)
    /// Reconstructs the linear history for a specific branch tip.
    pub async fn traverse_session_history_sovereign(
        &self,
        leaf_id: &str,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let history = sqlx::query(
            r#"
            WITH RECURSIVE branch AS (
                SELECT id, mission_id, parent_id, role, content, metadata, created_at
                FROM mission_nodes
                WHERE id = ?1
                UNION ALL
                SELECT m.id, m.mission_id, m.parent_id, m.role, m.content, m.metadata, m.created_at
                FROM mission_nodes m
                JOIN branch b ON m.id = b.parent_id
            )
            SELECT * FROM branch ORDER BY created_at ASC
            "#,
        )
        .bind(leaf_id)
        .fetch_all(&self.resources.pool)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to traverse session history: {}", e))
        })?;

        let result: Vec<_> = history
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                serde_json::json!({
                    "id": row.try_get::<String, _>("id").unwrap_or_default(),
                    "role": row.try_get::<String, _>("role").unwrap_or_default(),
                    "content": row.try_get::<String, _>("content").unwrap_or_default(),
                    "metadata": row.try_get::<Option<String>, _>("metadata").ok().flatten().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                    "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok()
                })
            })
            .collect();

        Ok(result)
    }

    /// ### 🧠 State: Append Session Node (append_session_node_sovereign)
    /// Appends a new node to the session tree and atomically updates `mission_history.active_node_id`
    /// within a single SQLite transaction.
    pub async fn append_session_node_sovereign(
        &self,
        mission_id: &str,
        parent_id: Option<String>,
        role: &str,
        content: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<String, AppError> {
        let node_id = uuid::Uuid::new_v4().to_string();
        let metadata_str = metadata.map(|m| m.to_string());

        let mut tx = self.resources.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO mission_nodes (id, mission_id, parent_id, role, content, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&node_id)
        .bind(mission_id)
        .bind(parent_id)
        .bind(role)
        .bind(content)
        .bind(metadata_str)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to append session node: {}", e))
        })?;

        // Update active leaf in mission_history within the same transaction
        sqlx::query("UPDATE mission_history SET active_node_id = ?1 WHERE id = ?2")
            .bind(&node_id)
            .bind(mission_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!(
                    "Failed to update mission active_node_id: {}",
                    e
                ))
            })?;

        tx.commit().await?;

        Ok(node_id)
    }

    /// ### 🧠 State: Revert to Node (revert_to_node_sovereign)
    /// Sets the active head of the mission to a previous node (Time Travel).
    pub async fn revert_to_node_sovereign(
        &self,
        mission_id: &str,
        node_id: &str,
    ) -> Result<(), AppError> {
        // Verify node exists and belongs to mission
        let exists = sqlx::query("SELECT 1 FROM mission_nodes WHERE id = ?1 AND mission_id = ?2")
            .bind(node_id)
            .bind(mission_id)
            .fetch_optional(&self.resources.pool)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Validation failed: {}", e)))?;

        if exists.is_none() {
            return Err(AppError::BadRequest(
                "Target node does not exist in this mission context".to_string(),
            ));
        }

        sqlx::query("UPDATE mission_history SET active_node_id = ?1 WHERE id = ?2")
            .bind(node_id)
            .bind(mission_id)
            .execute(&self.resources.pool)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Reversion failed: {}", e)))?;

        tracing::info!(
            "⏳ [TimeTravel] Mission {} reverted to node {}",
            mission_id,
            node_id
        );
        Ok(())
    }
}

// Metadata: [session]


