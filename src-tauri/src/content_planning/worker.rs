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

pub struct LocalContentPlanWorker<P: AIProvider + 'static = Arc<dyn AIProvider>> {
    pub worker_id: String,
    pub job_manager: Arc<JobManager>,
    pub planner: Arc<ContentPlanner<P>>,
    pub plan_service: Arc<ContentPlanService>,
    pub heartbeat_interval: Duration,
}

impl<P: AIProvider + 'static> LocalContentPlanWorker<P> {
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
        }
    }

    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
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
                    let _ = self.job_manager.fail_job_with_token(
                        &job.id,
                        job.lease_token.as_deref(),
                        &err.to_string(),
                        allow_retry,
                    );
                }
                Err(err)
            }
        }
    }

    fn process_one_job_inner(&self, job: &Job) -> Result<ContentPlan, ContentPlanningError> {
        self.ensure_fenced_and_active(job)?;

        let payload: ContentPlanJobPayload = serde_json::from_str(&job.payload)
            .map_err(|e| ContentPlanningError::InvalidPlanningRequest(e.to_string()))?;

        match payload.operation {
            PlanningOperation::Generate => self.process_generate_job(job, payload),
            PlanningOperation::RegeneratePlan => self.process_regenerate_plan_job(job, payload),
            PlanningOperation::RegenerateScene => self.process_regenerate_scene_job(job, payload),
            PlanningOperation::RegenerateNarration => {
                self.process_regenerate_narration_job(job, payload)
            }
        }
    }

    fn process_generate_job(
        &self,
        job: &Job,
        payload: ContentPlanJobPayload,
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

        let planner = self.planner.clone();
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

        let planner = self.planner.clone();
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

        let planner = self.planner.clone();
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

        let planner = self.planner.clone();
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

pub fn start_local_content_plan_worker<P: AIProvider + 'static>(
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
