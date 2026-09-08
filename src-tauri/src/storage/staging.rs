use super::StorageError;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StagedFile {
    pub temp_path: PathBuf,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Streams source file contents into a newly created staging file while computing SHA-256 in 64KB buffers.
/// Does not load entire media files into memory. Cleans up temporary files on error.
pub fn stream_to_staging(
    source_path: &Path,
    staging_dir: &Path,
) -> Result<StagedFile, StorageError> {
    if !source_path.exists() {
        return Err(StorageError::SourceNotFound(source_path.to_path_buf()));
    }
    if !source_path.is_file() {
        return Err(StorageError::SourceNotFile(source_path.to_path_buf()));
    }

    if !staging_dir.exists() {
        fs::create_dir_all(staging_dir)?;
    }

    let temp_filename = format!("stage_{}.tmp", Uuid::new_v4());
    let temp_path = staging_dir.join(temp_filename);

    let mut source = File::open(source_path)
        .map_err(|e| StorageError::SourceUnreadable(format!("{}: {}", source_path.display(), e)))?;
    let mut temp_file = File::create(&temp_path)?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536]; // 64KB chunks
    let mut total_bytes = 0u64;

    loop {
        match source.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                hasher.update(&buffer[..n]);
                if let Err(e) = temp_file.write_all(&buffer[..n]) {
                    let _ = fs::remove_file(&temp_path);
                    return Err(StorageError::Io(e));
                }
                total_bytes += n as u64;
            }
            Err(e) => {
                let _ = fs::remove_file(&temp_path);
                return Err(StorageError::SourceUnreadable(format!(
                    "{}: {}",
                    source_path.display(),
                    e
                )));
            }
        }
    }

    if let Err(e) = temp_file.flush() {
        let _ = fs::remove_file(&temp_path);
        return Err(StorageError::Io(e));
    }

    let sha256 = format!("{:x}", hasher.finalize());

    tracing::debug!(
        source = %source_path.display(),
        temp = %temp_path.display(),
        sha256 = %sha256,
        size_bytes = total_bytes,
        "Streamed source file into staging"
    );

    Ok(StagedFile {
        temp_path,
        sha256,
        size_bytes: total_bytes,
    })
}
