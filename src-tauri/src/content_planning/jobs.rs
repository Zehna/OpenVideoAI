use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::model::{InputType, PlanningRequest, PlanningSource};
use crate::content_planning::prompt::PLANNER_PROFILE_VERSION;
use crate::jobs::manager::JobManager;
use crate::jobs::model::{Job, JobSubmission};

pub const CONTENT_PLAN_JOB_TYPE: &str = "content.plan";
pub const DEFAULT_CONTENT_PLAN_MAX_RETRIES: i32 = 3;
pub const DEFAULT_CONTENT_PLAN_PRIORITY: i32 = 10;
pub const DEFAULT_CONTENT_PLAN_PAYLOAD_VERSION: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningOperation {
    Generate,
    RegeneratePlan,
    RegenerateScene,
    RegenerateNarration,
}

impl PlanningOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::RegeneratePlan => "regenerate_plan",
            Self::RegenerateScene => "regenerate_scene",
            Self::RegenerateNarration => "regenerate_narration",
        }
    }
}

impl fmt::Display for PlanningOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPlanJobPayload {
    pub schema_version: i32,
    pub project_id: String,
    pub operation: PlanningOperation,
    pub planning_request: PlanningRequest,
    pub source_context: Vec<PlanningSource>,
    pub base_plan_id: Option<String>,
    pub base_revision: Option<i64>,
    pub scene_id: Option<String>,
    pub user_instruction: Option<String>,
    pub provider: String,
    pub provider_config_ref: String,
    pub planner_profile_version: i32,
}

impl ContentPlanJobPayload {
    /// Returns the effective sources for snapshot and generation
    pub fn effective_sources(&self) -> &[PlanningSource] {
        &self.source_context
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegenerationJobParams {
    pub project_id: String,
    pub plan_id: String,
    pub base_revision: i64,
    pub operation: PlanningOperation,
    pub scene_id: Option<String>,
    pub user_instruction: Option<String>,
    pub planning_request: PlanningRequest,
    pub source_context: Vec<PlanningSource>,
    pub provider: Option<String>,
    pub provider_config_ref: Option<String>,
}

pub fn compute_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn compute_input_hash(req: &PlanningRequest) -> String {
    let canonical = format!(
        "{}:{}:{}:{}:{}",
        match req.input_type {
            InputType::Topic => "topic",
            InputType::Script => "script",
        },
        req.input_text.trim(),
        req.target_duration_ms,
        req.language.trim(),
        req.tone.trim()
    );
    compute_sha256(canonical.as_bytes())
}

pub fn compute_context_hash(sources: &[PlanningSource]) -> String {
    let mut sorted_sources = sources.to_vec();
    sorted_sources.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    let json = serde_json::to_string(&sorted_sources).unwrap_or_default();
    compute_sha256(json.as_bytes())
}

pub fn compute_instruction_hash(instruction: Option<&str>) -> String {
    let trimmed = instruction.map(str::trim).unwrap_or("");
    compute_sha256(trimmed.as_bytes())
}

pub fn generate_idempotency_key(
    project_id: &str,
    input_hash: &str,
    context_hash: &str,
    planner_profile_version: i32,
) -> String {
    format!(
        "content.plan:generate:{}:{}:{}:{}",
        project_id, input_hash, context_hash, planner_profile_version
    )
}

pub fn regenerate_plan_idempotency_key(
    plan_id: &str,
    base_revision: i64,
    instruction_hash: &str,
    planner_profile_version: i32,
) -> String {
    format!(
        "content.plan:regenerate_plan:{}:{}:{}:{}",
        plan_id, base_revision, instruction_hash, planner_profile_version
    )
}

pub fn regenerate_scene_idempotency_key(
    plan_id: &str,
    base_revision: i64,
    scene_id: &str,
    instruction_hash: &str,
    planner_profile_version: i32,
) -> String {
    format!(
        "content.plan:regenerate_scene:{}:{}:{}:{}:{}",
        plan_id, base_revision, scene_id, instruction_hash, planner_profile_version
    )
}

pub fn regenerate_narration_idempotency_key(
    plan_id: &str,
    base_revision: i64,
    scene_id: &str,
    instruction_hash: &str,
    planner_profile_version: i32,
) -> String {
    format!(
        "content.plan:regenerate_narration:{}:{}:{}:{}:{}",
        plan_id, base_revision, scene_id, instruction_hash, planner_profile_version
    )
}

pub fn submit_generate_job(
    job_manager: &JobManager,
    project_id: &str,
    mut request: PlanningRequest,
    provider: Option<String>,
    provider_config_ref: Option<String>,
) -> Result<Job, ContentPlanningError> {
    if request.input_text.trim().is_empty() {
        return Err(ContentPlanningError::InvalidPlanningRequest(
            "input text cannot be empty".to_string(),
        ));
    }
    request.project_id = project_id.to_string();
    let source_context = request.source_context.clone();
    let input_hash = compute_input_hash(&request);
    let context_hash = compute_context_hash(&source_context);
    let planner_profile_version = PLANNER_PROFILE_VERSION;

    let idempotency_key = generate_idempotency_key(
        project_id,
        &input_hash,
        &context_hash,
        planner_profile_version,
    );

    let payload = ContentPlanJobPayload {
        schema_version: DEFAULT_CONTENT_PLAN_PAYLOAD_VERSION,
        project_id: project_id.to_string(),
        operation: PlanningOperation::Generate,
        planning_request: request,
        source_context,
        base_plan_id: None,
        base_revision: None,
        scene_id: None,
        user_instruction: None,
        provider: provider.unwrap_or_else(|| "gemini".to_string()),
        provider_config_ref: provider_config_ref.unwrap_or_else(|| "gemini-default".to_string()),
        planner_profile_version,
    };

    let payload_json = serde_json::to_string(&payload)
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

    let submission = JobSubmission {
        job_type: CONTENT_PLAN_JOB_TYPE.to_string(),
        payload: payload_json,
        payload_version: Some(DEFAULT_CONTENT_PLAN_PAYLOAD_VERSION),
        priority: Some(DEFAULT_CONTENT_PLAN_PRIORITY),
        idempotency_key: Some(idempotency_key),
        project_id: Some(project_id.to_string()),
        max_retries: Some(DEFAULT_CONTENT_PLAN_MAX_RETRIES),
    };

    job_manager
        .submit_job(submission)
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))
}

pub fn submit_regeneration_job(
    job_manager: &JobManager,
    mut params: RegenerationJobParams,
) -> Result<Job, ContentPlanningError> {
    if params.operation == PlanningOperation::Generate {
        return Err(ContentPlanningError::InvalidPlanningRequest(
            "Cannot submit Generate operation as a regeneration job".to_string(),
        ));
    }
    if params.base_revision <= 0 {
        return Err(ContentPlanningError::InvalidPlanningRequest(
            "base_revision must be positive".to_string(),
        ));
    }
    match params.operation {
        PlanningOperation::RegenerateScene | PlanningOperation::RegenerateNarration => {
            let scene_id = params
                .scene_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ContentPlanningError::InvalidPlanningRequest(
                        "scene_id is required for scene and narration regeneration".to_string(),
                    )
                })?;
            params.scene_id = Some(scene_id.to_string());
        }
        PlanningOperation::RegeneratePlan => {
            params.scene_id = None;
        }
        PlanningOperation::Generate => unreachable!(),
    }

    let instruction_hash = compute_instruction_hash(params.user_instruction.as_deref());
    let planner_profile_version = PLANNER_PROFILE_VERSION;

    let idempotency_key = match params.operation {
        PlanningOperation::RegeneratePlan => regenerate_plan_idempotency_key(
            &params.plan_id,
            params.base_revision,
            &instruction_hash,
            planner_profile_version,
        ),
        PlanningOperation::RegenerateScene => regenerate_scene_idempotency_key(
            &params.plan_id,
            params.base_revision,
            params.scene_id.as_deref().unwrap(),
            &instruction_hash,
            planner_profile_version,
        ),
        PlanningOperation::RegenerateNarration => regenerate_narration_idempotency_key(
            &params.plan_id,
            params.base_revision,
            params.scene_id.as_deref().unwrap(),
            &instruction_hash,
            planner_profile_version,
        ),
        PlanningOperation::Generate => unreachable!(),
    };

    // Synchronize source_context between planning_request and top-level source_context
    params.planning_request.source_context = params.source_context.clone();
    params.planning_request.project_id = params.project_id.clone();

    let payload = ContentPlanJobPayload {
        schema_version: DEFAULT_CONTENT_PLAN_PAYLOAD_VERSION,
        project_id: params.project_id.clone(),
        operation: params.operation,
        planning_request: params.planning_request,
        source_context: params.source_context,
        base_plan_id: Some(params.plan_id),
        base_revision: Some(params.base_revision),
        scene_id: params.scene_id,
        user_instruction: params.user_instruction,
        provider: params.provider.unwrap_or_else(|| "gemini".to_string()),
        provider_config_ref: params
            .provider_config_ref
            .unwrap_or_else(|| "gemini-default".to_string()),
        planner_profile_version,
    };

    let payload_json = serde_json::to_string(&payload)
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

    let submission = JobSubmission {
        job_type: CONTENT_PLAN_JOB_TYPE.to_string(),
        payload: payload_json,
        payload_version: Some(DEFAULT_CONTENT_PLAN_PAYLOAD_VERSION),
        priority: Some(DEFAULT_CONTENT_PLAN_PRIORITY),
        idempotency_key: Some(idempotency_key),
        project_id: Some(params.project_id),
        max_retries: Some(DEFAULT_CONTENT_PLAN_MAX_RETRIES),
    };

    job_manager
        .submit_job(submission)
        .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))
}
