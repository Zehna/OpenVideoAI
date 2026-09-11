use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TimelineError {
    #[error("Timeline not found: {0}")]
    TimelineNotFound(String),
    #[error("Track not found: {0}")]
    TrackNotFound(String),
    #[error("Clip not found: {0}")]
    ClipNotFound(String),
    #[error("Revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: i64, actual: i64 },
    #[error("Invalid timeline operation: {0}")]
    InvalidOperation(String),
    #[error("Clips cannot overlap on the same track")]
    ClipOverlap,
    #[error("Invalid clip range")]
    InvalidClipRange,
    #[error("Referenced artifact not found: {0}")]
    ArtifactNotFound(String),
    #[error("Source media duration is unknown")]
    MediaDurationUnknown,
    #[error("Preview unavailable: {0}")]
    PreviewUnavailable(String),
    #[error("Render spec is invalid: {0}")]
    RenderSpecInvalid(String),
    #[error("FFmpeg executable unavailable: {0}")]
    FfmpegUnavailable(String),
    #[error("Render failed: {0}")]
    RenderFailed(String),
    #[error("Render output artifact import failed: {0}")]
    RenderOutputImportFailed(String),
    #[error("Database error: {0}")]
    Database(String),
}

impl From<rusqlite::Error> for TimelineError {
    fn from(err: rusqlite::Error) -> Self {
        TimelineError::Database(err.to_string())
    }
}

impl From<crate::db::DbError> for TimelineError {
    fn from(err: crate::db::DbError) -> Self {
        TimelineError::Database(err.to_string())
    }
}
