use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ContentPlanningError {
    #[error("Content plan not found: {0}")]
    ContentPlanNotFound(String),

    #[error("Content plan revision not found: plan {plan_id}, revision {revision}")]
    ContentPlanRevisionNotFound { plan_id: String, revision: i64 },

    #[error("Content plan revision conflict for plan {plan_id}: expected {expected}, actual {actual}")]
    ContentPlanRevisionConflict {
        plan_id: String,
        expected: i64,
        actual: i64,
    },

    #[error("Content plan is archived: {0}")]
    ContentPlanArchived(String),

    #[error("Invalid planning request: {0}")]
    InvalidPlanningRequest(String),

    #[error("Planning context too large: {0}")]
    PlanningContextTooLarge(String),

    #[error("Content plan validation failed: {0}")]
    ContentPlanValidationFailed(String),

    #[error("Source grounding invalid: {0}")]
    SourceGroundingInvalid(String),

    #[error("Provider auth missing: {0}")]
    ProviderAuthMissing(String),

    #[error("Provider auth failed: {0}")]
    ProviderAuthFailed(String),

    #[error("Provider rate limited: {0}")]
    ProviderRateLimited(String),

    #[error("Provider timeout: {0}")]
    ProviderTimeout(String),

    #[error("Provider unavailable: {0}")]
    ProviderUnavailable(String),

    #[error("Structured output invalid: {0}")]
    StructuredOutputInvalid(String),

    #[error("Planning cancelled: {0}")]
    PlanningCancelled(String),

    #[error("Stale lease: {0}")]
    StaleLease(String),

    #[error("Content plan persist failed: {0}")]
    ContentPlanPersistFailed(String),
}

impl ContentPlanningError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ContentPlanNotFound(_) => "content_plan_not_found",
            Self::ContentPlanRevisionNotFound { .. } => "content_plan_revision_not_found",
            Self::ContentPlanRevisionConflict { .. } => "content_plan_revision_conflict",
            Self::ContentPlanArchived(_) => "content_plan_archived",
            Self::InvalidPlanningRequest(_) => "invalid_planning_request",
            Self::PlanningContextTooLarge(_) => "planning_context_too_large",
            Self::ContentPlanValidationFailed(_) => "content_plan_validation_failed",
            Self::SourceGroundingInvalid(_) => "source_grounding_invalid",
            Self::ProviderAuthMissing(_) => "provider_auth_missing",
            Self::ProviderAuthFailed(_) => "provider_auth_failed",
            Self::ProviderRateLimited(_) => "provider_rate_limited",
            Self::ProviderTimeout(_) => "provider_timeout",
            Self::ProviderUnavailable(_) => "provider_unavailable",
            Self::StructuredOutputInvalid(_) => "structured_output_invalid",
            Self::PlanningCancelled(_) => "planning_cancelled",
            Self::StaleLease(_) => "stale_lease",
            Self::ContentPlanPersistFailed(_) => "content_plan_persist_failed",
        }
    }
}
