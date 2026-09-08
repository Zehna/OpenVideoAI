use super::StorageError;
use std::fs;
use std::path::Path;

/// Sweeps leftover staging files (e.g. stage_*.tmp) inside the staging directory.
/// Safely runs on application startup to ensure interrupted imports do not accumulate.
pub fn sweep_staging_directory(staging_dir: &Path) -> Result<usize, StorageError> {
    if !staging_dir.exists() {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in fs::read_dir(staging_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if filename.starts_with("stage_") && filename.ends_with(".tmp") {
                if let Err(e) = fs::remove_file(&path) {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to remove stale staging file");
                } else {
                    removed += 1;
                }
            }
        }
    }

    if removed > 0 {
        tracing::info!(count = removed, dir = %staging_dir.display(), "Swept stale staging files");
    }

    Ok(removed)
}
