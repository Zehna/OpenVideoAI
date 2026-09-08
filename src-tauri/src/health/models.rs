use crate::db::DatabaseStatus;
use crate::tools::ToolStatus;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthSnapshot {
    pub app_version: String,
    pub database: DatabaseStatus,
    pub ffmpeg: ToolStatus,
    pub ffprobe: ToolStatus,
}
