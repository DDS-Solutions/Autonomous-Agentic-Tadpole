//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### MemoryActor
//! Asynchronous sink for institutional memory and vector knowledge.
//! Centralizes LanceDB/Arrow operations to prevent lock contention
//! and ensure consistency across the swarm.

use crate::system::actors::SystemMessage;
use crate::agent::memory::VectorMemory;
use crate::error::AppError;
use tokio::sync::mpsc;
use tracing::{info, debug};
use sqlx::SqlitePool;

pub struct MemoryActor {
    receiver: mpsc::Receiver<SystemMessage>,
    vector_memory: VectorMemory,
    pool: SqlitePool,
}

impl MemoryActor {
    /// Connects to the LanceDB storage and returns a new MemoryActor.
    pub async fn new(
        receiver: mpsc::Receiver<SystemMessage>,
        base_dir: &std::path::Path,
        pool: SqlitePool,
    ) -> Result<Self, AppError> {
        let memory_path = base_dir.join("institutional_memory");
        let path_str = memory_path.to_string_lossy().to_string();
        
        info!("🧠 [MemoryActor] Connecting to vector store at: {}", path_str);
        
        let vector_memory = VectorMemory::connect(&path_str, "knowledge_base").await?;
        vector_memory.ensure_table().await?;

        Ok(Self {
            receiver,
            vector_memory,
            pool,
        })
    }

    /// Primary execution loop for the MemoryActor.
    pub async fn run(mut self) {
        info!("🧠 [MemoryActor] Logic loop active. Multiversal Session Tree online.");

        while let Some(msg) = self.receiver.recv().await {
            match msg {
                SystemMessage::MemoryQuery { query: _, limit: _, resp } => {
                    let _ = resp.send(Err(AppError::InternalServerError("MemoryQuery via Actor not yet fully implemented".to_string())));
                }
                SystemMessage::MemorySave { content, resp } => {
                    let result = self.handle_save(content).await;
                    let _ = resp.send(result);
                }
                SystemMessage::MemoryAppend { mission_id, parent_id, role, content, metadata, resp } => {
                    let result = self.handle_append(mission_id, parent_id, role, content, metadata).await;
                    let _ = resp.send(result);
                }
                SystemMessage::MemoryTraverse { leaf_id, resp } => {
                    let result = self.handle_traverse(leaf_id).await;
                    let _ = resp.send(result);
                }
                SystemMessage::MemoryBranch { parent_id, resp } => {
                    // For now, branching is implicit in Append, but we return the parent_id as the new tip
                    let _ = resp.send(Ok(parent_id));
                }
                SystemMessage::Shutdown => {
                    info!("🧠 [MemoryActor] Shutdown received. Flushing knowledge buffers...");
                    break;
                }
                _ => {
                    debug!("🧠 [MemoryActor] Ignoring non-memory message type.");
                }
            }
        }
    }

    async fn handle_save(&self, content: serde_json::Value) -> Result<(), AppError> {
        let text = content.get("text").and_then(|v| v.as_str()).unwrap_or_default();
        let mission_id = content.get("mission_id").and_then(|v| v.as_str()).unwrap_or("global");
        let vector = content.get("vector")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|n| n.as_f64().map(|f| f as f32)).collect::<Vec<f32>>())
            .unwrap_or_default();

        if text.is_empty() || vector.is_empty() {
            return Err(AppError::BadRequest("MemorySave requires 'text' and 'vector'".to_string()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        self.vector_memory.add_memory(&id, text, mission_id, vector).await
    }

    async fn handle_append(
        &self,
        mission_id: String,
        parent_id: Option<String>,
        role: String,
        content: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<String, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let metadata_json = metadata.map(|m| m.to_string()).unwrap_or_else(|| "{}".to_string());

        // Jittered Retry for SQLITE_BUSY
        let mut attempts = 0;
        let max_attempts = 5;
        let mut last_err = None;

        while attempts < max_attempts {
            let result = sqlx::query(
                "INSERT INTO mission_nodes (id, mission_id, parent_id, role, content, metadata) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(&mission_id)
            .bind(&parent_id)
            .bind(&role)
            .bind(&content)
            .bind(&metadata_json)
            .execute(&self.pool)
            .await;

            match result {
                Ok(_) => {
                    // Also update the mission's active leaf (non-critical, so we ignore error)
                    sqlx::query("UPDATE mission_history SET active_node_id = ? WHERE id = ?")
                        .bind(&id)
                        .bind(&mission_id)
                        .execute(&self.pool)
                        .await
                        .ok();
                    return Ok(id);
                },
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("database is locked") || err_msg.contains("code 5") || err_msg.contains("code 6") {
                        attempts += 1;
                        let delay = 10 + (rand::random::<u64>() % 50); // Jittered sleep
                        tokio::time::sleep(std::time::Duration::from_millis(delay * attempts)).await;
                        last_err = Some(e);
                    } else {
                        return Err(AppError::InternalServerError(format!("Failed to append node: {}", e)));
                    }
                }
            }
        }

        Err(AppError::InternalServerError(format!(
            "Failed to append node after {} attempts: {}", 
            max_attempts, 
            last_err.map(|e| e.to_string()).unwrap_or_default()
        )))
    }

    async fn handle_traverse(&self, leaf_id: String) -> Result<Vec<serde_json::Value>, AppError> {
        // Recursive CTE to find all ancestors with depth tracking for correct ordering
        let nodes = sqlx::query_as::<_, (String, String, Option<String>, String, String, String)>(
            r#"
            WITH RECURSIVE history AS (
                SELECT id, mission_id, parent_id, role, content, metadata, created_at, 0 as depth
                FROM mission_nodes
                WHERE id = ?
                UNION ALL
                SELECT n.id, n.mission_id, n.parent_id, n.role, n.content, n.metadata, n.created_at, h.depth + 1
                FROM mission_nodes n
                JOIN history h ON n.id = h.parent_id
            )
            SELECT id, mission_id, parent_id, role, content, metadata 
            FROM history 
            ORDER BY depth DESC
            "#
        )
        .bind(leaf_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::InternalServerError(format!("History traversal failed: {}", e)))?;

        let result = nodes.into_iter().map(|(id, mission_id, parent_id, role, content, metadata)| {
            serde_json::json!({
                "id": id,
                "mission_id": mission_id,
                "parent_id": parent_id,
                "role": role,
                "content": content,
                "metadata": serde_json::from_str::<serde_json::Value>(&metadata).unwrap_or_default()
            })
        }).collect();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_multiversal_traversal() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        
        // Run migration manually for the test
        sqlx::query(
            "CREATE TABLE mission_history (id TEXT PRIMARY KEY, active_node_id TEXT)"
        ).execute(&pool).await.unwrap();
        
        sqlx::query(
            "CREATE TABLE mission_nodes (
                id TEXT PRIMARY KEY,
                mission_id TEXT NOT NULL,
                parent_id TEXT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(&pool).await.unwrap();

        let (_tx, rx) = mpsc::channel(10);
        let base_dir = std::env::temp_dir();
        let actor = MemoryActor::new(rx, &base_dir, pool.clone()).await.unwrap();

        let m_id = "test-mission";
        
        // Seed mission
        sqlx::query("INSERT INTO mission_history (id) VALUES (?)").bind(m_id).execute(&pool).await.unwrap();
        
        // 1. Root
        let root_id = actor.handle_append(m_id.to_string(), None, "system".to_string(), "Root".to_string(), None).await.unwrap();
        
        // 2. Branch A
        let a1_id = actor.handle_append(m_id.to_string(), Some(root_id.clone()), "user".to_string(), "A1".to_string(), None).await.unwrap();
        
        // 3. Branch B (Fork from Root)
        let b1_id = actor.handle_append(m_id.to_string(), Some(root_id.clone()), "user".to_string(), "B1".to_string(), None).await.unwrap();

        // Traverse Branch A
        let history_a = actor.handle_traverse(a1_id).await.unwrap();
        assert_eq!(history_a.len(), 2);
        assert_eq!(history_a[0]["content"], "Root");
        assert_eq!(history_a[1]["content"], "A1");

        // Traverse Branch B
        let history_b = actor.handle_traverse(b1_id).await.unwrap();
        assert_eq!(history_b.len(), 2);
        assert_eq!(history_b[0]["content"], "Root");
        assert_eq!(history_b[1]["content"], "B1");
    }
}
