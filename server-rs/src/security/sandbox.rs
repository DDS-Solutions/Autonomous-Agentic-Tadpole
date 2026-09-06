//! @docs ARCHITECTURE:AgentExecutionRuntime
//! @docs ARCHITECTURE:SecurityModel
//!
//! ### AI Assist Note
//! **Isolated Skill Execution Sandbox**: Implements secure execution environments
//! for promoted AI agent skills using Docker containerization or WebAssembly (Wasmtime).
//! Gated by **Resource Limits** (CPU, memory) and filesystem isolation to mitigate
//! Remote Code Execution (RCE) on the host machine.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Docker daemon unresponsive, Wasm file missing or malformed,
//!   resource exhaustion (OOM), or execution timeouts.
//! - **Telemetry Link**: Search `[sandbox]` in logs.

use crate::error::AppError;
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub use_docker: bool,
    pub use_wasm: bool,
    pub allow_host_fallback: bool,
    pub cpu_limit: Option<f32>,
    pub memory_limit_mb: Option<usize>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        let use_docker = std::env::var("USE_SANDBOX_DOCKER")
            .map(|v| v == "true")
            .unwrap_or(false);
        let use_wasm = std::env::var("USE_SANDBOX_WASM")
            .map(|v| v == "true")
            .unwrap_or(false);
        let allow_host_fallback = std::env::var("ALLOW_HOST_SKILL_EXECUTION")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        let cpu_limit = std::env::var("SANDBOX_CPU_LIMIT")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        let memory_limit_mb = std::env::var("SANDBOX_MEMORY_LIMIT_MB")
            .ok()
            .and_then(|v| v.parse::<usize>().ok());

        Self {
            use_docker,
            use_wasm,
            allow_host_fallback,
            cpu_limit,
            memory_limit_mb,
        }
    }
}

/// Executes a skill command within the configured sandbox.
pub async fn execute_sandboxed(
    command_str: &str,
    args_json: &str,
    workspace_root: &Path,
    config: &SandboxConfig,
) -> Result<String, AppError> {
    let mut parts = command_str.split_whitespace();
    let program = parts.next().ok_or_else(|| AppError::BadRequest("Empty command".to_string()))?;
    let args: Vec<&str> = parts.collect();

    if config.use_docker {
        tracing::info!("[Sandbox] Spawning Docker container for execution: {}", command_str);
        
        if which::which("docker").is_err() {
            return Err(AppError::InfrastructureError {
                provider_id: "docker".to_string(),
                detail: "Docker CLI binary is not installed or not found on PATH.".to_string(),
                help_link: Some("https://docs.docker.com/get-docker/".to_string()),
            });
        }

        let workspace_str = workspace_root.to_string_lossy().to_string();
        let mut docker_cmd = Command::new("docker");
        docker_cmd.arg("run")
            .arg("--rm")
            .arg("--cap-drop=ALL")
            .arg("--network=none")
            .arg("-v")
            .arg(format!("{}:/workspace", workspace_str))
            .arg("-w")
            .arg("/workspace")
            .arg("-e")
            .arg(format!("TADPOLE_SKILL_ARGS={}", args_json));

        if let Some(mem) = config.memory_limit_mb {
            docker_cmd.arg("-m").arg(format!("{}m", mem));
        }

        if let Some(cpu) = config.cpu_limit {
            docker_cmd.arg("--cpus").arg(cpu.to_string());
        }

        docker_cmd.arg("python:3.10-slim");
        
        // Wrap command args
        docker_cmd.arg(program);
        for arg in args {
            docker_cmd.arg(arg);
        }

        let output = tokio::time::timeout(std::time::Duration::from_secs(60), docker_cmd.output()).await
            .map_err(|_| AppError::InfrastructureError {
                provider_id: "sandboxed_python".to_string(),
                detail: "Docker execution timed out after 60s".to_string(),
                help_link: None,
            })?
            .map_err(AppError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "sandboxed_python".to_string(),
                detail: format!("Docker execution failed: {}\nstdout: {}", stderr, stdout),
                help_link: None,
            })
        }
    } else if config.use_wasm || program.ends_with(".wasm") {
        tracing::info!("[Sandbox] Spawning Wasmtime sandbox for execution: {}", command_str);

        if which::which("wasmtime").is_err() {
            return Err(AppError::InfrastructureError {
                provider_id: "wasmtime".to_string(),
                detail: "Wasmtime executable was not found on PATH. Install wasmtime or verify system PATH to execute WebAssembly skills.".to_string(),
                help_link: Some("https://wasmtime.dev/".to_string()),
            });
        }

        let mut wasm_cmd = Command::new("wasmtime");
        wasm_cmd.arg("run")
            .arg("--dir")
            .arg(workspace_root)
            .arg(program);
            
        for arg in args {
            wasm_cmd.arg(arg);
        }

        wasm_cmd.env("TADPOLE_SKILL_ARGS", args_json);
        wasm_cmd.current_dir(workspace_root);

        let output = tokio::time::timeout(std::time::Duration::from_secs(60), wasm_cmd.output()).await
            .map_err(|_| AppError::InfrastructureError {
                provider_id: "wasmtime".to_string(),
                detail: "Wasm execution timed out after 60s".to_string(),
                help_link: None,
            })?
            .map_err(AppError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "wasmtime".to_string(),
                detail: format!("Wasm execution failed: {}\nstdout: {}", stderr, stdout),
                help_link: None,
            })
        }
    } else {
        if !config.allow_host_fallback {
            tracing::error!(
                "[Sandbox] Unsandboxed host execution denied for command: {}. Sandboxing is enforced: enable Docker (USE_SANDBOX_DOCKER=true), Wasm (USE_SANDBOX_WASM=true), or explicitly permit host execution (ALLOW_HOST_SKILL_EXECUTION=true).",
                command_str
            );
            return Err(AppError::Forbidden(
                "Unsandboxed skill execution on host is disabled by default. Configure USE_SANDBOX_DOCKER=true, USE_SANDBOX_WASM=true, or explicitly set ALLOW_HOST_SKILL_EXECUTION=true.".to_string()
            ));
        }

        tracing::warn!("[Sandbox] Falling back to standard execution for command: {}", command_str);
        
        let mut cmd = Command::new(program);
        for arg in args {
            cmd.arg(arg);
        }
        cmd.env("TADPOLE_SKILL_ARGS", args_json);
        cmd.current_dir(workspace_root);

        let output = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output()).await
            .map_err(|_| AppError::InfrastructureError {
                provider_id: "legacy_skill".to_string(),
                detail: "Legacy execution timed out after 60s".to_string(),
                help_link: None,
            })?
            .map_err(AppError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(AppError::InfrastructureError {
                provider_id: "legacy_skill".to_string(),
                detail: format!("Legacy execution failed: {}\nstdout: {}", stderr, stdout),
                help_link: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unsandboxed_execution_blocked_by_default() {
        let config = SandboxConfig {
            use_docker: false,
            use_wasm: false,
            allow_host_fallback: false,
            cpu_limit: None,
            memory_limit_mb: None,
        };
        let res = execute_sandboxed("echo hello", "{}", Path::new("."), &config).await;
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::Forbidden(msg) => {
                assert!(msg.contains("Unsandboxed skill execution"));
            }
            other => panic!("Expected AppError::Forbidden, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_empty_command_rejected() {
        let config = SandboxConfig::default();
        let res = execute_sandboxed("", "{}", Path::new("."), &config).await;
        assert!(res.is_err());
    }
}

// Metadata: [sandbox]
