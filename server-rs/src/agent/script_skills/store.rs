/*
@docs ARCHITECTURE:SovereignKernel

### AI Assist Note
**🛡️ Tadpole OS: Store**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

//! Storage persistence, atomic file rotation, and verified deletion primitives.

use std::path::Path;
use tokio::fs;
use crate::error::AppError;
use super::model::Capability;
use super::parse::{collision_safe_skill_filename, validate_capability_name};
use super::loader::read_file_bounded;

/// Atomically writes content to the target file using temporary file rotation and fsync.
pub async fn atomic_write(path: &Path, content: &[u8]) -> Result<(), AppError> {
    let unique_id = uuid::Uuid::new_v4();
    let tmp_path = path.with_extension(format!("tmp.{}", unique_id));
    let write_res: Result<(), AppError> = async {
        let mut f = fs::File::create(&tmp_path).await.map_err(AppError::Io)?;
        tokio::io::AsyncWriteExt::write_all(&mut f, content).await.map_err(AppError::Io)?;
        f.sync_all().await.map_err(AppError::Io)?;
        drop(f);
        fs::rename(&tmp_path, path).await.map_err(AppError::Io)?;
        if let Some(parent) = path.parent() {
            if let Ok(parent_file) = fs::File::open(parent).await {
                let _ = parent_file.sync_all().await;
            }
        }
        Ok(())
    }.await;

    if write_res.is_err() {
        let _ = fs::remove_file(&tmp_path).await;
    }
    write_res
}

/// Cleans up any orphaned temporary files created during prior incomplete writes (older than 60s).
pub async fn cleanup_stale_temp_files(dir: &Path) {
    if let Ok(mut entries) = fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.contains(".tmp.") {
                    if let Ok(meta) = fs::metadata(&path).await {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed.as_secs() > 60 {
                                    let _ = fs::remove_file(&path).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Persists a capability to disk with size check, collision protection, and atomic write.
pub async fn persist_capability<T: Capability>(dir: &Path, capability: &T) -> Result<(), AppError> {
    validate_capability_name(capability.name())?;
    capability.validate()?;

    let filename = collision_safe_skill_filename(capability.name(), T::EXT);
    let path = crate::utils::security::validate_path(dir, &filename)
        .map_err(|e| AppError::BadRequest(format!("Invalid capability name or path: {e}")))?;

    // Guard against overwriting an existing file that belongs to a different capability
    if fs::try_exists(&path).await.unwrap_or(false) {
        if let Ok(existing_content) = read_file_bounded(&path, T::MAX_BYTES).await {
            let existing_stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if let Some(existing_name) = T::extract_name(existing_content.as_bytes(), &existing_stem) {
                if existing_name != capability.name() {
                    return Err(AppError::Conflict(format!(
                        "Filename collision: file '{}' already belongs to capability '{}'",
                        filename, existing_name
                    )));
                }
            }
        }
    }

    let content = capability.serialize_content()?;
    atomic_write(&path, &content).await
}

/// Performs verified deletion of a capability from a directory.
/// Probes canonical hashed filename, legacy sanitized filename, raw filename, and full directory scan.
/// Unlinks a file ONLY if the embedded name matches `expected_name`.
pub async fn verified_remove<T: Capability>(dir: &Path, expected_name: &str) -> Result<bool, AppError> {
    if !fs::try_exists(dir).await.unwrap_or(false) {
        return Ok(false);
    }

    let mut removed = false;

    // Helper closure to verify embedded name and delete
    async fn try_remove_file<T: Capability>(path: &Path, expected_name: &str) -> bool {
        if let Ok(meta) = fs::symlink_metadata(path).await {
            if meta.file_type().is_symlink() {
                return false;
            }
            if let Ok(content) = read_file_bounded(path, T::MAX_BYTES).await {
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if let Some(name) = T::extract_name(content.as_bytes(), &stem) {
                    if name == expected_name {
                        return fs::remove_file(path).await.is_ok();
                    }
                }
            }
        }
        false
    }

    // 1. Probe canonical hashed filename
    let canonical = collision_safe_skill_filename(expected_name, T::EXT);
    if let Ok(path) = crate::utils::security::validate_path(dir, &canonical) {
        if try_remove_file::<T>(&path, expected_name).await {
            removed = true;
        }
    }

    // 2. Probe legacy sanitized filename
    let legacy = format!("{}.{}", crate::utils::security::sanitize_id(expected_name), T::EXT);
    if legacy != canonical {
        if let Ok(path) = crate::utils::security::validate_path(dir, &legacy) {
            if try_remove_file::<T>(&path, expected_name).await {
                removed = true;
            }
        }
    }

    // 3. Probe raw filename
    let raw = format!("{expected_name}.{}", T::EXT);
    if raw != canonical && raw != legacy {
        if let Ok(path) = crate::utils::security::validate_path(dir, &raw) {
            if try_remove_file::<T>(&path, expected_name).await {
                removed = true;
            }
        }
    }

    // 4. Directory scan for embedded capability name
    if let Ok(mut entries) = fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some(T::EXT) {
                if try_remove_file::<T>(&path, expected_name).await {
                    removed = true;
                }
            }
        }
    }

    Ok(removed)
}
