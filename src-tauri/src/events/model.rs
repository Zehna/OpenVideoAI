use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainEvent<T = serde_json::Value> {
    pub event_id: String,
    pub event_type: String,
    pub version: u32,
    pub timestamp: String,
    pub project_id: Option<String>,
    pub job_id: Option<String>,
    pub payload: T,
}

impl<T> DomainEvent<T> {
    pub fn new(
        event_type: String,
        project_id: Option<String>,
        job_id: Option<String>,
        payload: T,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type,
            version: 1,
            timestamp: Utc::now().to_rfc3339(),
            project_id,
            job_id,
            payload,
        }
    }
}
