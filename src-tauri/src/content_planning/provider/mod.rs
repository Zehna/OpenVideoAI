pub mod config;
pub mod gemini;
pub mod mock;

pub use config::*;
pub use gemini::*;
pub use mock::*;

use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::model::GenerationUsage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    pub system_prompt: String,
    pub user_prompt: String,
    pub schema_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderResponse {
    pub text: String,
    pub usage: Option<GenerationUsage>,
    pub model: String,
    pub provider: String,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum ProviderError {
    #[error("provider authentication missing: {0}")]
    AuthMissing(String),
    #[error("provider authentication failed: {0}")]
    AuthFailed(String),
    #[error("provider rate limited: {0}")]
    RateLimited(String),
    #[error("provider timeout: {0}")]
    Timeout(String),
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("structured output invalid: {0}")]
    StructuredOutputInvalid(String),
    #[error("provider transport error: {0}")]
    Transport(String),
}

impl ProviderError {
    pub fn to_content_planning_error(&self) -> ContentPlanningError {
        match self {
            ProviderError::AuthMissing(m) => ContentPlanningError::ProviderAuthMissing(m.clone()),
            ProviderError::AuthFailed(m) => ContentPlanningError::ProviderAuthFailed(m.clone()),
            ProviderError::RateLimited(m) => ContentPlanningError::ProviderRateLimited(m.clone()),
            ProviderError::Timeout(m) => ContentPlanningError::ProviderTimeout(m.clone()),
            ProviderError::Unavailable(m) => ContentPlanningError::ProviderUnavailable(m.clone()),
            ProviderError::StructuredOutputInvalid(m) => {
                ContentPlanningError::StructuredOutputInvalid(m.clone())
            }
            ProviderError::Transport(m) => ContentPlanningError::ProviderUnavailable(m.clone()),
        }
    }
}

impl From<ProviderError> for ContentPlanningError {
    fn from(err: ProviderError) -> Self {
        err.to_content_planning_error()
    }
}

pub trait AIProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn generate_planning_draft(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError>;
    fn generate_content_plan(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError>;
    fn regenerate_plan(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    fn regenerate_scene(&self, input: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    fn regenerate_narration(
        &self,
        input: &ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError>;
}
