use super::metadata::{probe_media, MediaMetadata, MetadataRepository};
use super::model::{guess_mime_type, Artifact, ArtifactKind, ArtifactRef};
use super::repository::ArtifactRepository;
use crate::db::{DbError, DbManager};
use crate::projects::repository::ProjectRepository;
use crate::storage::local_store::LocalStore;
use crate::storage::staging::stream_to_staging;
use crate::storage::StorageError;
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ImporterError {
    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Database error: {0}")]
    Database(#[from] DbError),

    #[error("Import failed: {0}")]
    General(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedArtifact {
    pub artifact: ArtifactRef,
    pub original_filename: String,
    pub added_at: String,
    pub metadata: Option<MediaMetadata>,
    pub is_deduplicated: bool,
}

pub struct ArtifactImporter {
    db: Arc<DbManager>,
    store: Arc<LocalStore>,
    staging_dir: PathBuf,
}

impl ArtifactImporter {
    pub fn new(db: Arc<DbManager>, store: Arc<LocalStore>, staging_dir: PathBuf) -> Self {
        Self {
            db,
            store,
            staging_dir,
        }
    }

    pub fn import(
        &self,
        project_id: &str,
        source_path: &Path,
    ) -> Result<ImportedArtifact, ImporterError> {
        let conn = self.db.conn()?;

        // 1. Verify project exists
        let project = ProjectRepository::get_by_id(&conn, project_id)?
            .ok_or_else(|| ImporterError::ProjectNotFound(project_id.to_string()))?;

        // 2. Stream source into staging while computing SHA-256
        let staged = stream_to_staging(source_path, &self.staging_dir)?;

        let original_filename = source_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed_artifact".to_string());

        let ext = source_path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();

        // 3. Check if an artifact with this SHA-256 already exists in the registry
        let existing = ArtifactRepository::get_by_sha256(&conn, &staged.sha256)?;

        let (artifact, is_deduplicated) = match existing {
            Some(existing_artifact) => {
                // Content already stored! Discard staging file
                let _ = fs::remove_file(&staged.temp_path);
                tracing::info!(
                    sha256 = %staged.sha256,
                    artifact_id = %existing_artifact.id,
                    "Reusing existing content-addressed artifact"
                );
                (existing_artifact, true)
            }
            None => {
                // New artifact: promote staging file to canonical store
                let storage_key = self.store.store_artifact(&staged)?;
                let kind = ArtifactKind::from_extension(&ext);
                let mime_type = guess_mime_type(&ext);
                let now = Utc::now().to_rfc3339();

                let new_artifact = Artifact {
                    id: Uuid::new_v4().to_string(),
                    sha256: staged.sha256.clone(),
                    kind,
                    mime_type,
                    size_bytes: staged.size_bytes,
                    storage_key,
                    original_filename: original_filename.clone(),
                    created_at: now,
                };

                // Insert artifact record
                ArtifactRepository::insert(&conn, &new_artifact)?;

                // Probe media metadata if applicable (video/audio/image)
                let canonical_path = self.store.resolve_path(&new_artifact.storage_key)?;
                let meta = probe_media(&new_artifact.id, &canonical_path, None);
                let _ = MetadataRepository::insert(&conn, &meta);

                tracing::info!(
                    artifact_id = %new_artifact.id,
                    sha256 = %new_artifact.sha256,
                    "Registered new artifact in registry"
                );

                (new_artifact, false)
            }
        };

        // 4. Link artifact to project
        let added_at = Utc::now().to_rfc3339();
        ArtifactRepository::link_to_project(&conn, &project.id, &artifact.id, &added_at)?;

        // Touch project updated_at
        let _ = conn.execute(
            "UPDATE projects SET updated_at = ?1 WHERE id = ?2;",
            params![added_at, project.id],
        );

        // 5. Retrieve metadata for return
        let metadata = MetadataRepository::get_by_artifact_id(&conn, &artifact.id)?;

        Ok(ImportedArtifact {
            artifact: artifact.to_ref(),
            original_filename,
            added_at,
            metadata,
            is_deduplicated,
        })
    }

    pub fn list_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<ImportedArtifact>, ImporterError> {
        let conn = self.db.conn()?;

        // Verify project exists
        let _ = ProjectRepository::get_by_id(&conn, project_id)?
            .ok_or_else(|| ImporterError::ProjectNotFound(project_id.to_string()))?;

        let artifacts_with_added = ArtifactRepository::list_for_project(&conn, project_id)?;
        let mut results = Vec::with_capacity(artifacts_with_added.len());

        for (artifact, added_at) in artifacts_with_added {
            let metadata = MetadataRepository::get_by_artifact_id(&conn, &artifact.id)?;
            results.push(ImportedArtifact {
                artifact: artifact.to_ref(),
                original_filename: artifact.original_filename,
                added_at,
                metadata,
                is_deduplicated: false,
            });
        }

        Ok(results)
    }
}
