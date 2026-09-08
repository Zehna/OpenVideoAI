use super::repository::ArtifactRepository;
use crate::db::{DbError, DbManager};
use crate::storage::local_store::LocalStore;
use crate::storage::StorageError;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("Invalid artifact URI scheme: expected 'artifact://', got '{0}'")]
    InvalidScheme(String),

    #[error("Artifact not found: {0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    Database(#[from] DbError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct ArtifactResolver {
    db: Arc<DbManager>,
    store: Arc<LocalStore>,
}

impl ArtifactResolver {
    pub fn new(db: Arc<DbManager>, store: Arc<LocalStore>) -> Self {
        Self { db, store }
    }

    /// Resolves an internal "artifact://<id>" URI into its canonical on-disk absolute path.
    pub fn resolve_uri(&self, uri: &str) -> Result<PathBuf, ResolverError> {
        let artifact_id = uri
            .strip_prefix("artifact://")
            .ok_or_else(|| ResolverError::InvalidScheme(uri.to_string()))?;

        if artifact_id.is_empty()
            || artifact_id.contains('/')
            || artifact_id.contains('\\')
            || artifact_id.contains("..")
        {
            return Err(ResolverError::InvalidScheme(uri.to_string()));
        }

        let conn = self.db.conn()?;
        let artifact = ArtifactRepository::get_by_id(&conn, artifact_id)?
            .ok_or_else(|| ResolverError::NotFound(artifact_id.to_string()))?;

        let canonical_path = self.store.resolve_path(&artifact.storage_key)?;
        Ok(canonical_path)
    }
}
