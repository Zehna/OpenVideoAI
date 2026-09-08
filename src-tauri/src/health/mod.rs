pub mod models;

use crate::db::DbManager;
use crate::tools::detect_tool;
pub use models::HealthSnapshot;
use std::sync::Arc;

pub struct HealthService {
    db: Arc<DbManager>,
}

impl HealthService {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self { db }
    }

    pub fn get_snapshot(&self) -> HealthSnapshot {
        let database = self.db.check_health().unwrap_or_else(|e| {
            tracing::error!(error = %e, "Database health check failed");
            crate::db::DatabaseStatus {
                ok: false,
                path: self.db.path().to_string_lossy().to_string(),
            }
        });

        let ffmpeg = detect_tool("ffmpeg", None);
        let ffprobe = detect_tool("ffprobe", None);

        HealthSnapshot {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            database,
            ffmpeg,
            ffprobe,
        }
    }
}
