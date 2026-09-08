pub mod local_store;
pub mod staging;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Source file not found: {0}")]
    SourceNotFound(PathBuf),

    #[error("Source path is not a regular file: {0}")]
    SourceNotFile(PathBuf),

    #[error("Source file is unreadable: {0}")]
    SourceUnreadable(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid storage key: {0}")]
    InvalidStorageKey(String),

    #[error("Path traversal detected: {0}")]
    PathTraversal(String),
}
