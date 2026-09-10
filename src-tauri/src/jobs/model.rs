use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    WaitingForCapacity,
    Retryable,
    Completed,
    Failed,
    Cancelled,
    DeadLetter,
}

impl JobState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingForCapacity => "waiting_for_capacity",
            Self::Retryable => "retryable",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::DeadLetter => "dead_letter",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::DeadLetter
        )
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for JobState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting_for_capacity" => Ok(Self::WaitingForCapacity),
            "retryable" => Ok(Self::Retryable),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "dead_letter" => Ok(Self::DeadLetter),
            other => Err(format!("Unknown job state: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub job_type: String,
    pub status: JobState,
    pub priority: i32,
    pub idempotency_key: Option<String>,
    pub project_id: Option<String>,
    pub payload: String,
    #[serde(default = "default_payload_version")]
    pub payload_version: i32,
    pub result: Option<String>,
    pub error: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub leased_by: Option<String>,
    pub lease_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

fn default_payload_version() -> i32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobCheckpoint {
    pub id: String,
    pub job_id: String,
    pub stage: String,
    pub checkpoint_data: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobSubmission {
    pub job_type: String,
    pub payload: String,
    pub payload_version: Option<i32>,
    pub priority: Option<i32>,
    pub idempotency_key: Option<String>,
    pub project_id: Option<String>,
    pub max_retries: Option<i32>,
}
