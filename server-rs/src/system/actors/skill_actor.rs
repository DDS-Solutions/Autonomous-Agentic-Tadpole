//! @docs ARCHITECTURE:SovereignKernel
//!
//! ### SkillScanner Actor
//! Autonomously crawls the local workspace to ingest and catalog README-based skills.
//! Implements a safe, non-recursive (or depth-limited) scan to prevent I/O saturation.

use crate::error::AppError;
use crate::system::actors::SystemMessage;
use crate::state::AppState;
use crate::agent::script_skills::parse_skill_md;
use tokio::sync::mpsc;
use std::sync::Arc;
use walkdir::WalkDir;
// use std::path::Path;

pub struct SkillScannerActor {
    app_state: Arc<AppState>,
    receiver: mpsc::Receiver<SystemMessage>,
}

impl SkillScannerActor {
    pub fn new(app_state: Arc<AppState>, receiver: mpsc::Receiver<SystemMessage>) -> Self {
        Self { app_state, receiver }
    }

    pub async fn run(mut self) {
        tracing::info!("[SkillScanner] Actor lifecycle initialized.");
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                SystemMessage::SkillScan { path, resp } => {
                    let result = self.handle_scan(&path).await;
                    let _ = resp.send(result);
                }
                SystemMessage::Shutdown => {
                    tracing::info!("[SkillScanner] Shutdown signal received. Draining...");
                    break;
                }
                _ => {
                    tracing::warn!("[SkillScanner] Received unexpected message type.");
                }
            }
        }
    }

    async fn handle_scan(&self, root_path: &str) -> Result<usize, AppError> {
        tracing::info!("[SkillScanner] Starting autonomous scan of: {}", root_path);
        
        let root_path_string = root_path.to_string();
        let app_state = self.app_state.clone();

        // Offload blocking I/O to a dedicated thread pool
        let count = tokio::task::spawn_blocking(move || {
            let mut count = 0;
            // Configuration for the scan
            let max_depth = 5;
            let ignored_dirs = ["node_modules", ".git", "target", "dist", "server-rs", ".next", ".gemini", ".brain"];

            for entry in WalkDir::new(&root_path_string)
                .max_depth(max_depth)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                
                // Skip ignored directories
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        if ignored_dirs.contains(&name) {
                            continue;
                        }
                    }
                }

                // Look for SKILL.md or README.md with frontmatter
                if path.is_file() {
                    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if file_name == "SKILL.md" || file_name == "README.md" {
                        // Using standard fs::read_to_string since we are in spawn_blocking
                        if let Ok(content) = std::fs::read_to_string(path) {
                            if let Some(skill) = parse_skill_md(&content) {
                                tracing::info!("[SkillScanner] Discovered capability: {} at {:?}", skill.name, path);
                                
                                // Register the skill (This involves async DB calls, so we need a runtime handle)
                                let data = serde_json::to_value(&skill).unwrap_or(serde_json::Value::Null);
                                if !data.is_null() {
                                    let rt = tokio::runtime::Handle::current();
                                    let state = app_state.clone();
                                    match rt.block_on(state.registry.skills.register_capability("skill", data, "workspace_discovered")) {
                                        Ok(_) => count += 1,
                                        Err(e) => tracing::error!("[SkillScanner] Failed to register {}: {}", skill.name, e),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            count
        }).await.map_err(|e| AppError::InternalServerError(format!("Skill scan task panicked: {}", e)))?;

        tracing::info!("[SkillScanner] Scan complete. Ingested {} capabilities.", count);
        Ok(count)
    }
}

// Metadata: [skill_actor]
