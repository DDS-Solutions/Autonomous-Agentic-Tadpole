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
use tracing::{info, error, debug};

pub struct MemoryActor {
    receiver: mpsc::UnboundedReceiver<SystemMessage>,
    vector_memory: VectorMemory,
}

impl MemoryActor {
    /// Connects to the LanceDB storage and returns a new MemoryActor.
    pub async fn new(
        receiver: mpsc::UnboundedReceiver<SystemMessage>,
        base_dir: &std::path::Path,
    ) -> Result<Self, AppError> {
        let memory_path = base_dir.join("institutional_memory");
        let path_str = memory_path.to_string_lossy().to_string();
        
        info!("🧠 [MemoryActor] Connecting to vector store at: {}", path_str);
        
        let vector_memory = VectorMemory::connect(&path_str, "knowledge_base").await?;
        vector_memory.ensure_table().await?;

        Ok(Self {
            receiver,
            vector_memory,
        })
    }

    /// Primary execution loop for the MemoryActor.
    pub async fn run(mut self) {
        info!("🧠 [MemoryActor] Logic loop active. Vector store secured.");

        while let Some(msg) = self.receiver.recv().await {
            match msg {
                SystemMessage::MemoryQuery { query: _, limit: _, resp } => {
                    // TODO: Implement embedding retrieval for queries
                    let _ = resp.send(Err(AppError::InternalServerError("MemoryQuery via Actor not yet fully implemented (requires embedding provider access)".to_string())));
                }
                SystemMessage::MemorySave { content, resp } => {
                    let result = self.handle_save(content).await;
                    let _ = resp.send(result);
                }
                SystemMessage::Shutdown => {
                    info!("🧠 [MemoryActor] Shutdown received. Draining knowledge buffers...");
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
}
