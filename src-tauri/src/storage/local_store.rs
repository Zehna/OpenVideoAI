use super::staging::StagedFile;
use super::StorageError;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct LocalStore {
    artifacts_dir: PathBuf,
}

impl LocalStore {
    pub fn new(artifacts_dir: PathBuf) -> Self {
        Self { artifacts_dir }
    }

    /// Computes canonical storage key: "artifacts/<sha256[0:2]>/<sha256>"
    pub fn compute_storage_key(&self, sha256: &str) -> String {
        let prefix = if sha256.len() >= 2 {
            &sha256[..2]
        } else {
            "00"
        };
        format!("artifacts/{}/{}", prefix, sha256)
    }

    /// Promotes staged file into the canonical content-addressed storage location.
    /// If an identical artifact already exists on disk, the staging file is discarded.
    pub fn store_artifact(&self, staged: &StagedFile) -> Result<String, StorageError> {
        let key = self.compute_storage_key(&staged.sha256);
        let target_path = self.resolve_path(&key)?;

        if let Some(parent) = target_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // If target already exists, content is already stored (dedup)
        if target_path.exists() {
            let _ = fs::remove_file(&staged.temp_path);
            tracing::debug!(key = %key, "Content already stored in canonical store");
            return Ok(key);
        }

        // Atomic move into canonical destination (rename, fallback to copy+delete across devices)
        if fs::rename(&staged.temp_path, &target_path).is_err() {
            fs::copy(&staged.temp_path, &target_path)?;
            let _ = fs::remove_file(&staged.temp_path);
        }

        tracing::info!(key = %key, target = %target_path.display(), "Promoted staged file to canonical artifact");
        Ok(key)
    }

    /// Resolves canonical storage key to absolute filesystem path.
    /// Strictly rejects path traversal or foreign prefixes.
    pub fn resolve_path(&self, storage_key: &str) -> Result<PathBuf, StorageError> {
        if storage_key.contains("..") || storage_key.contains(':') {
            return Err(StorageError::PathTraversal(storage_key.to_string()));
        }

        let normalized = storage_key.replace('\\', "/");
        if !normalized.starts_with("artifacts/") {
            return Err(StorageError::InvalidStorageKey(storage_key.to_string()));
        }

        let parts: Vec<&str> = normalized.split('/').collect();
        if parts.len() != 3 || parts[1].len() != 2 || parts[2].len() != 64 {
            return Err(StorageError::InvalidStorageKey(storage_key.to_string()));
        }

        // Parent of prefix and sha256 inside artifacts_dir
        let path = self.artifacts_dir.join(parts[1]).join(parts[2]);
        Ok(path)
    }

    pub fn artifacts_dir(&self) -> &PathBuf {
        &self.artifacts_dir
    }
}
