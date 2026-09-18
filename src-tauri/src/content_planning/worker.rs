use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::content_planning::error::ContentPlanningError;
use crate::content_planning::jobs::{
    ContentPlanJobPayload, PlanningOperation, CONTENT_PLAN_JOB_TYPE,
};
use crate::content_planning::model::ContentPlan;
use crate::content_planning::planner::ContentPlanner;
use crate::content_planning::provider::AIProvider;
use crate::content_planning::service::ContentPlanService;
use crate::jobs::manager::JobManager;
use crate::jobs::model::{Job, JobState};

pub struct LocalContentPlanWorkerHandle {
    shutdown_tx: watch::Sender<bool>,
    join_handle: JoinHandle<()>,
}

impl LocalContentPlanWorkerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.join_handle.await;
    }
}

pub const DEFAULT_LEASE_DURATION_SECS: i64 = 60;
pub const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
pub const DEFAULT_CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(500);
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);

pub const BASE_RETRY_BACKOFF_MS: u64 = 2000;
pub const MAX_RETRY_BACKOFF_MS: u64 = 10000;

pub trait RetrySleeper: Send + Sync {
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Clone, Default)]
pub struct RealRetrySleeper;

impl RetrySleeper for RealRetrySleeper {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[derive(Debug, Clone, Default)]
pub struct NoopRetrySleeper;

impl RetrySleeper for NoopRetrySleeper {
    fn sleep(&self, _duration: Duration) {}
}

#[derive(Debug, Default)]
pub struct RecordingRetrySleeper {
    pub sleeps: std::sync::Mutex<Vec<Duration>>,
}

impl RecordingRetrySleeper {
    pub fn new() -> Self {
        Self {
            sleeps: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn recorded_sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }

    pub fn total_slept(&self) -> Duration {
        self.sleeps.lock().unwrap().iter().sum()
    }
}

impl RetrySleeper for RecordingRetrySleeper {
    fn sleep(&self, duration: Duration) {
        self.sleeps.lock().unwrap().push(duration);
    }
}

pub fn calculate_retry_backoff(job_id: &str, retry_count: i64) -> Duration {
    if retry_count <= 0 {
        return Duration::ZERO;
    }

    let exponent = (retry_count - 1).min(2) as u32;
    let base_ms = BASE_RETRY_BACKOFF_MS.saturating_mul(1 << exponent);
    let jitter_window = base_ms / 4;

    let mut hasher = Sha256::new();
    hasher.update(job_id.as_bytes());
    hasher.update(b":");
    hasher.update(retry_count.to_string().as_bytes());
    let hash = hasher.finalize();
    let seed = u64::from_be_bytes(hash[0..8].try_into().unwrap());
    let jitter = seed % (jitter_window + 1);

    let total_ms = (base_ms + jitter).min(MAX_RETRY_BACKOFF_MS);
    Duration::from_millis(total_ms)
}

use crate::content_planning::provider::{FALLBACK_GEMINI_MODEL, PRIMARY_GEMINI_MODEL};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeminiModelRoute {
    Primary,
    Fallback,
}

impl GeminiModelRoute {
    pub fn model_name(&self) -> &'static str {
        match self {
            Self::Primary => PRIMARY_GEMINI_MODEL,
            Self::Fallback => FALLBACK_GEMINI_MODEL,
        }
    }

    pub fn is_fallback(&self) -> bool {
        matches!(self, Self::Fallback)
    }
}

pub fn extract_error_code(error: Option<&str>) -> Option<&str> {
    let err = error?.trim();
    if err.is_empty() {
        return None;
    }
    if let Some((code, _)) = err.split_once(':') {
        let trimmed = code.trim();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    Some(err)
}

pub fn is_transient_capacity_error(code: &str) -> bool {
    let normalized = code.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "provider_unavailable" | "transient_unavailable"
    )
}

pub fn select_model_for_attempt(
    retry_count: i32,
    max_retries: i32,
    previous_error_code: Option<&str>,
) -> GeminiModelRoute {
    // Pure model routing decision:
    // Only route to Fallback when:
    // 1. We are on the final eligible attempt: retry_count >= max_retries
    //    (e.g., retry_count == 3 when max_retries == 3).
    // 2. The previous error was specifically provider unavailability (HTTP 503 high demand / capacity exhaustion).
    // All initial attempts (retry_count == 0) and intermediate retries (retry_count < max_retries)
    // use Primary.
    if retry_count <= 0 || retry_count < max_retries {
        return GeminiModelRoute::Primary;
    }

    if let Some(err_str) = previous_error_code {
        if let Some(code) = extract_error_code(Some(err_str)) {
            if is_transient_capacity_error(code) {
                return GeminiModelRoute::Fallback;
            }
        }
    }

    GeminiModelRoute::Primary
}

pub struct LocalContentPlanWorker<P: AIProvider + Clone + 'static = Arc<dyn AIProvider>> {
    pub worker_id: String,
    pub job_manager: Arc<JobManager>,
    pub planner: Arc<ContentPlanner<P>>,
    pub plan_service: Arc<ContentPlanService>,
    pub heartbeat_interval: Duration,
    pub sleeper: Arc<dyn RetrySleeper>,
}

impl<P: AIProvider + Clone + 'static> LocalContentPlanWorker<P> {
    pub fn new(
        worker_id: String,
        job_manager: Arc<JobManager>,
        planner: Arc<ContentPlanner<P>>,
        plan_service: Arc<ContentPlanService>,
    ) -> Self {
        Self {
            worker_id,
            job_manager,
            planner,
            plan_service,
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
            sleeper: Arc::new(RealRetrySleeper),
        }
    }

    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    pub fn with_sleeper(mut self, sleeper: Arc<dyn RetrySleeper>) -> Self {
        self.sleeper = sleeper;
        self
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    pub fn acquire_job(&self) -> Result<Option<Job>, ContentPlanningError> {
        self.job_manager
            .acquire_next_job_for_type(CONTENT_PLAN_JOB_TYPE, DEFAULT_LEASE_DURATION_SECS)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))
    }

    /// Verifies lease and cancellation fence.
    fn ensure_fenced_and_active(&self, job: &Job) -> Result<(), ContentPlanningError> {
        let current = self
            .job_manager
            .get_job(&job.id)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?
            .ok_or_else(|| {
                ContentPlanningError::ContentPlanNotFound(format!("Job {} not found", job.id))
            })?;

        if current.status == JobState::Cancelled {
            return Err(ContentPlanningError::PlanningCancelled(format!(
                "Job {} was cancelled",
                job.id
            )));
        }

        if current.status != JobState::Running {
            return Err(ContentPlanningError::StaleLease(format!(
                "Job {} is no longer in running state (current state: {})",
                job.id, current.status
            )));
        }

        if current.lease_token != job.lease_token {
            return Err(ContentPlanningError::StaleLease(format!(
                "Job {} lease token mismatch (lost lease fence)",
                job.id
            )));
        }

        Ok(())
    }

    /// Waits with bounded exponential backoff and deterministic jitter on retries.
    /// Checks cancellation and lease fence in <= 500ms chunks.
    fn wait_for_retry_backoff(&self, job: &Job) -> Result<(), ContentPlanningError> {
        let total_delay = calculate_retry_backoff(&job.id, job.retry_count as i64);
        if total_delay.is_zero() {
            return Ok(());
        }

        tracing::info!(
            job_id = %job.id,
            retry_count = job.retry_count,
            delay_ms = total_delay.as_millis(),
            "Delaying retryable content planning job"
        );

        let chunk_size = Duration::from_millis(500);
        let mut remaining = total_delay;

        while remaining > Duration::ZERO {
            let chunk = remaining.min(chunk_size);
            self.sleeper.sleep(chunk);
            remaining = remaining.saturating_sub(chunk);
            self.ensure_fenced_and_active(job)?;
        }

        Ok(())
    }

    /// Executes closure with cooperative heartbeat loop and cancellation polling.
    fn execute_with_heartbeat<F, R>(&self, job: &Job, f: F) -> Result<R, ContentPlanningError>
    where
        F: FnOnce() -> Result<R, ContentPlanningError> + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res = f();
            let _ = tx.send(res);
        });

        let mut last_heartbeat = std::time::Instant::now();

        loop {
            match rx.recv_timeout(DEFAULT_CANCELLATION_POLL_INTERVAL) {
                Ok(result) => return result,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Poll cancellation and lease status
                    if let Ok(Some(j)) = self.job_manager.get_job(&job.id) {
                        if j.status == JobState::Cancelled {
                            return Err(ContentPlanningError::PlanningCancelled(
                                "Job was cancelled during provider execution".to_string(),
                            ));
                        }
                        if j.status != JobState::Running {
                            return Err(ContentPlanningError::StaleLease(format!(
                                "Job {} is no longer in running state",
                                job.id
                            )));
                        }
                        if j.lease_token != job.lease_token {
                            return Err(ContentPlanningError::StaleLease(format!(
                                "Job {} lease token changed during provider execution",
                                job.id
                            )));
                        }
                    }

                    // Only send heartbeat every heartbeat_interval (default: 15s)
                    if last_heartbeat.elapsed() >= self.heartbeat_interval {
                        match self.job_manager.heartbeat_with_token(
                            &job.id,
                            job.lease_token.as_deref(),
                            DEFAULT_LEASE_DURATION_SECS,
                        ) {
                            Ok(true) => {
                                last_heartbeat = std::time::Instant::now();
                            }
                            _ => {
                                return Err(ContentPlanningError::StaleLease(
                                    "Lost job lease fence during provider execution".to_string(),
                                ));
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ContentPlanningError::ContentPlanPersistFailed(
                        "Worker task thread terminated unexpectedly".to_string(),
                    ));
                }
            }
        }
    }

    pub fn process_one_job(&self, job: &Job) -> Result<ContentPlan, ContentPlanningError> {
        match self.process_one_job_inner(job) {
            Ok(plan) => Ok(plan),
            Err(err) => {
                // If the job was cancelled, do not attempt to overwrite cancellation with Failed
                if !matches!(err, ContentPlanningError::PlanningCancelled(_)) {
                    let allow_retry = matches!(
                        err,
                        ContentPlanningError::ProviderRateLimited(_)
                            | ContentPlanningError::ProviderTimeout(_)
                            | ContentPlanningError::ProviderUnavailable(_)
                    );
                    let error_msg = format!("{}: {}", err.code(), err);
                    let _ = self.job_manager.fail_job_with_token(
                        &job.id,
                        job.lease_token.as_deref(),
                        &error_msg,
                        allow_retry,
                    );
                }
                Err(err)
            }
        }
    }

    fn process_one_job_inner(&self, job: &Job) -> Result<ContentPlan, ContentPlanningError> {
        self.ensure_fenced_and_active(job)?;

        if job.retry_count > 0 {
            self.wait_for_retry_backoff(job)?;
        }

        let previous_error_code = extract_error_code(job.error.as_deref());
        let route = select_model_for_attempt(job.retry_count, job.max_retries, previous_error_code);

        let active_planner = match route {
            GeminiModelRoute::Primary => self.planner.clone(),
            GeminiModelRoute::Fallback => {
                tracing::warn!(
                    job_id = %job.id,
                    retry_count = %job.retry_count,
                    max_retries = %job.max_retries,
                    fallback_model = %FALLBACK_GEMINI_MODEL,
                    "Primary model capacity exhausted after transient errors; executing attempt on fallback model"
                );
                Arc::new(self.planner.with_model(FALLBACK_GEMINI_MODEL))
            }
        };

        let payload: ContentPlanJobPayload = serde_json::from_str(&job.payload)
            .map_err(|e| ContentPlanningError::InvalidPlanningRequest(e.to_string()))?;

        match payload.operation {
            PlanningOperation::Generate => self.process_generate_job(job, payload, &active_planner),
            PlanningOperation::RegeneratePlan => {
                self.process_regenerate_plan_job(job, payload, &active_planner)
            }
            PlanningOperation::RegenerateScene => {
                self.process_regenerate_scene_job(job, payload, &active_planner)
            }
            PlanningOperation::RegenerateNarration => {
                self.process_regenerate_narration_job(job, payload, &active_planner)
            }
        }
    }

    fn process_generate_job(
        &self,
        job: &Job,
        payload: ContentPlanJobPayload,
        active_planner: &Arc<ContentPlanner<P>>,
    ) -> Result<ContentPlan, ContentPlanningError> {
        // 1. validate_request
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_request",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        if payload.planning_request.input_text.trim().is_empty() {
            return Err(ContentPlanningError::InvalidPlanningRequest(
                "input text cannot be empty".to_string(),
            ));
        }

        // 2. prepare_context
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "prepare_context",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        // 3. Check fence before draft call
        self.ensure_fenced_and_active(job)?;

        // 4. generate_outline
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "generate_outline",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let planner = active_planner.clone();
        let req = payload.planning_request.clone();
        let draft = self.execute_with_heartbeat(job, {
            let planner = planner.clone();
            let req = req.clone();
            move || planner.execute_draft_stage(&req)
        })?;

        // 5. validate_outline
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_outline",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        // 6. generate_content
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "generate_content",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.ensure_fenced_and_active(job)?;

        let plan = self.execute_with_heartbeat(job, {
            let planner = planner.clone();
            let req = req.clone();
            let draft = draft.clone();
            move || planner.execute_plan_expansion_stage(&req, &draft)
        })?;

        // 7. validate_content
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_content",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        // 5. Check fence before persist
        self.ensure_fenced_and_active(job)?;

        // 6. persist_revision
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "persist_revision",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let plan_id = if plan.id.is_empty() {
            format!("plan-{}", uuid::Uuid::new_v4())
        } else {
            plan.id.clone()
        };

        let published = self.plan_service.publish_generated_revision(
            &plan_id,
            None,
            plan,
            payload.source_context,
            "generate",
            "system-planner",
            Some(&job.id),
        )?;

        // 7. finalize
        self.job_manager
            .record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let result_json = serde_json::to_string(&published)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        Ok(published)
    }

    fn process_regenerate_plan_job(
        &self,
        job: &Job,
        payload: ContentPlanJobPayload,
        active_planner: &Arc<ContentPlanner<P>>,
    ) -> Result<ContentPlan, ContentPlanningError> {
        let plan_id = payload.base_plan_id.as_deref().ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("basePlanId required".to_string())
        })?;
        let base_revision = payload.base_revision.ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("baseRevision required".to_string())
        })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_request",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "load_base_revision",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let base_plan = self
            .plan_service
            .get_plan_revision(plan_id, base_revision)?
            .ok_or_else(|| ContentPlanningError::ContentPlanRevisionNotFound {
                plan_id: plan_id.to_string(),
                revision: base_revision,
            })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "prepare_context",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "generate_content",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.ensure_fenced_and_active(job)?;

        let planner = active_planner.clone();
        let instruction = payload.user_instruction.clone().unwrap_or_default();
        let sources = payload.source_context.clone();
        let new_plan = self.execute_with_heartbeat(job, move || {
            planner.regenerate_plan(&base_plan, &instruction, &sources)
        })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_content",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.ensure_fenced_and_active(job)?;

        // Base revision conflict check before publication
        let current = self
            .plan_service
            .get_plan(plan_id)?
            .ok_or_else(|| ContentPlanningError::ContentPlanNotFound(plan_id.to_string()))?;
        if current.revision != base_revision {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan_id.to_string(),
                expected: base_revision,
                actual: current.revision,
            });
        }

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "persist_revision",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let published = self.plan_service.publish_generated_revision(
            plan_id,
            Some(base_revision),
            new_plan,
            payload.source_context,
            "regenerate_plan",
            "system-planner",
            Some(&job.id),
        )?;

        self.job_manager
            .record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let result_json = serde_json::to_string(&published)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        Ok(published)
    }

    fn process_regenerate_scene_job(
        &self,
        job: &Job,
        payload: ContentPlanJobPayload,
        active_planner: &Arc<ContentPlanner<P>>,
    ) -> Result<ContentPlan, ContentPlanningError> {
        let plan_id = payload.base_plan_id.as_deref().ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("basePlanId required".to_string())
        })?;
        let base_revision = payload.base_revision.ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("baseRevision required".to_string())
        })?;
        let scene_id = payload.scene_id.clone().ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("sceneId required".to_string())
        })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_request",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "load_base_revision",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let base_plan = self
            .plan_service
            .get_plan_revision(plan_id, base_revision)?
            .ok_or_else(|| ContentPlanningError::ContentPlanRevisionNotFound {
                plan_id: plan_id.to_string(),
                revision: base_revision,
            })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "prepare_scene_context",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "generate_scene",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.ensure_fenced_and_active(job)?;

        let planner = active_planner.clone();
        let instruction = payload.user_instruction.clone().unwrap_or_default();
        let sources = payload.source_context.clone();
        let s_id = scene_id.clone();
        let new_plan = self.execute_with_heartbeat(job, move || {
            planner.regenerate_scene(&base_plan, &s_id, &instruction, &sources)
        })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_scene",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.ensure_fenced_and_active(job)?;

        // Base revision conflict check before publication
        let current = self
            .plan_service
            .get_plan(plan_id)?
            .ok_or_else(|| ContentPlanningError::ContentPlanNotFound(plan_id.to_string()))?;
        if current.revision != base_revision {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan_id.to_string(),
                expected: base_revision,
                actual: current.revision,
            });
        }

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "persist_revision",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let published = self.plan_service.publish_generated_revision(
            plan_id,
            Some(base_revision),
            new_plan,
            payload.source_context,
            "regenerate_scene",
            "system-planner",
            Some(&job.id),
        )?;

        self.job_manager
            .record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let result_json = serde_json::to_string(&published)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        Ok(published)
    }

    fn process_regenerate_narration_job(
        &self,
        job: &Job,
        payload: ContentPlanJobPayload,
        active_planner: &Arc<ContentPlanner<P>>,
    ) -> Result<ContentPlan, ContentPlanningError> {
        let plan_id = payload.base_plan_id.as_deref().ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("basePlanId required".to_string())
        })?;
        let base_revision = payload.base_revision.ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("baseRevision required".to_string())
        })?;
        let scene_id = payload.scene_id.clone().ok_or_else(|| {
            ContentPlanningError::InvalidPlanningRequest("sceneId required".to_string())
        })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_request",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "load_base_revision",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let base_plan = self
            .plan_service
            .get_plan_revision(plan_id, base_revision)?
            .ok_or_else(|| ContentPlanningError::ContentPlanRevisionNotFound {
                plan_id: plan_id.to_string(),
                revision: base_revision,
            })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "prepare_narration_context",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "generate_narration",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.ensure_fenced_and_active(job)?;

        let planner = active_planner.clone();
        let instruction = payload.user_instruction.clone().unwrap_or_default();
        let sources = payload.source_context.clone();
        let s_id = scene_id.clone();
        let new_plan = self.execute_with_heartbeat(job, move || {
            planner.regenerate_narration(&base_plan, &s_id, &instruction, &sources)
        })?;

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_narration",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.ensure_fenced_and_active(job)?;

        // Base revision conflict check before publication
        let current = self
            .plan_service
            .get_plan(plan_id)?
            .ok_or_else(|| ContentPlanningError::ContentPlanNotFound(plan_id.to_string()))?;
        if current.revision != base_revision {
            return Err(ContentPlanningError::ContentPlanRevisionConflict {
                plan_id: plan_id.to_string(),
                expected: base_revision,
                actual: current.revision,
            });
        }

        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "persist_revision",
                "{}",
            )
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let published = self.plan_service.publish_generated_revision(
            plan_id,
            Some(base_revision),
            new_plan,
            payload.source_context,
            "regenerate_narration",
            "system-planner",
            Some(&job.id),
        )?;

        self.job_manager
            .record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        let result_json = serde_json::to_string(&published)
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        self.job_manager
            .complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
            .map_err(|e| ContentPlanningError::ContentPlanPersistFailed(e.to_string()))?;

        Ok(published)
    }
}

pub fn start_local_content_plan_worker<P: AIProvider + Clone + 'static>(
    worker_id: String,
    job_manager: Arc<JobManager>,
    planner: Arc<ContentPlanner<P>>,
    plan_service: Arc<ContentPlanService>,
    poll_interval: Duration,
) -> LocalContentPlanWorkerHandle {
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let worker = Arc::new(LocalContentPlanWorker::new(
        worker_id,
        job_manager.clone(),
        planner,
        plan_service,
    ));

    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

    let rt_handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => RUNTIME
            .get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("failed to initialize background tokio runtime for content plan worker")
            })
            .handle()
            .clone(),
    };

    let join_handle = rt_handle.spawn(async move {
        loop {
            if *shutdown_rx.borrow() {
                break;
            }

            match job_manager
                .acquire_next_job_for_type(CONTENT_PLAN_JOB_TYPE, DEFAULT_LEASE_DURATION_SECS)
            {
                Ok(Some(job)) => {
                    let w = worker.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = w.process_one_job(&job);
                    })
                    .await;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Content plan worker failed to acquire job: {}", e);
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });

    LocalContentPlanWorkerHandle {
        shutdown_tx,
        join_handle,
    }
}
