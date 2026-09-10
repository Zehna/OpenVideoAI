use super::model::{Job, JobCheckpoint, JobSubmission};
use super::repository::{JobError, JobRepository};
use crate::db::DbManager;
use crate::events::bus::EventBus;
use crate::events::model::DomainEvent;
use serde_json::json;
use std::sync::Arc;

pub struct JobManager {
    db: Arc<DbManager>,
    event_bus: Arc<EventBus>,
    worker_id: String,
}

impl JobManager {
    pub fn new(db: Arc<DbManager>) -> Self {
        let event_bus = Arc::new(EventBus::new(db.clone()));
        Self::with_event_bus(db, event_bus)
    }

    pub fn with_event_bus(db: Arc<DbManager>, event_bus: Arc<EventBus>) -> Self {
        let worker_id = format!("worker-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        Self {
            db,
            event_bus,
            worker_id,
        }
    }

    pub fn with_worker_id(db: Arc<DbManager>, worker_id: String) -> Self {
        let event_bus = Arc::new(EventBus::new(db.clone()));
        Self::with_worker_and_event_bus(db, event_bus, worker_id)
    }

    pub fn with_worker_and_event_bus(
        db: Arc<DbManager>,
        event_bus: Arc<EventBus>,
        worker_id: String,
    ) -> Self {
        Self {
            db,
            event_bus,
            worker_id,
        }
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// Submits a job with transactional event persistence.
    /// If new, persists JobQueued event in the SAME transaction.
    pub fn submit_job(&self, submission: JobSubmission) -> Result<Job, JobError> {
        let mut conn = self.db.conn()?;
        let tx = conn.transaction()?;

        let (job, is_new) = JobRepository::enqueue_with_status(&tx, submission)?;

        let event = if is_new {
            let evt = DomainEvent::new(
                "JobQueued".to_string(),
                job.project_id.clone(),
                Some(job.id.clone()),
                json!({
                    "jobType": job.job_type,
                    "priority": job.priority,
                    "status": job.status.as_str(),
                    "payloadVersion": job.payload_version,
                }),
            );
            EventBus::record_event_tx(&tx, &evt)?;
            Some(evt)
        } else {
            None
        };

        tx.commit()?;

        // Broadcast to in-memory subscribers AFTER transaction commit
        if let Some(evt) = event {
            self.event_bus.broadcast(&evt);
        }

        Ok(job)
    }

    pub fn get_job(&self, job_id: &str) -> Result<Option<Job>, JobError> {
        let conn = self.db.conn()?;
        JobRepository::get_by_id(&conn, job_id)
    }

    pub fn acquire_next_job(&self, lease_duration_secs: i64) -> Result<Option<Job>, JobError> {
        // Run reaper on acquisition to ensure expired leases are reclaimed
        let _ = self.reap_expired_leases();
        let conn = self.db.conn()?;
        JobRepository::acquire_lease(&conn, &self.worker_id, lease_duration_secs)
    }

    pub fn heartbeat(&self, job_id: &str, lease_duration_secs: i64) -> Result<bool, JobError> {
        self.heartbeat_with_token(job_id, None, lease_duration_secs)
    }

    pub fn heartbeat_with_token(
        &self,
        job_id: &str,
        lease_token: Option<&str>,
        lease_duration_secs: i64,
    ) -> Result<bool, JobError> {
        let conn = self.db.conn()?;
        JobRepository::heartbeat_with_token(
            &conn,
            job_id,
            &self.worker_id,
            lease_token,
            lease_duration_secs,
        )
    }

    pub fn record_checkpoint(
        &self,
        job_id: &str,
        stage: &str,
        checkpoint_data: &str,
    ) -> Result<JobCheckpoint, JobError> {
        self.record_checkpoint_with_token(job_id, None, stage, checkpoint_data)
    }

    pub fn record_checkpoint_with_token(
        &self,
        job_id: &str,
        lease_token: Option<&str>,
        stage: &str,
        checkpoint_data: &str,
    ) -> Result<JobCheckpoint, JobError> {
        let mut conn = self.db.conn()?;
        let tx = conn.transaction()?;

        let cp = JobRepository::record_checkpoint_with_lease(
            &tx,
            job_id,
            Some(&self.worker_id),
            lease_token,
            stage,
            checkpoint_data,
        )?;

        let event = DomainEvent::new(
            "JobCheckpointReached".to_string(),
            None,
            Some(job_id.to_string()),
            json!({
                "stage": cp.stage,
                "checkpointId": cp.id,
            }),
        );
        EventBus::record_event_tx(&tx, &event)?;

        tx.commit()?;

        self.event_bus.broadcast(&event);

        Ok(cp)
    }

    pub fn get_checkpoints(&self, job_id: &str) -> Result<Vec<JobCheckpoint>, JobError> {
        let conn = self.db.conn()?;
        JobRepository::get_checkpoints(&conn, job_id)
    }

    pub fn complete_job(&self, job_id: &str, result_json: Option<String>) -> Result<Job, JobError> {
        self.complete_job_with_token(job_id, None, result_json)
    }

    pub fn complete_job_with_token(
        &self,
        job_id: &str,
        lease_token: Option<&str>,
        result_json: Option<String>,
    ) -> Result<Job, JobError> {
        let mut conn = self.db.conn()?;
        let tx = conn.transaction()?;

        let job = JobRepository::complete_job_with_lease(
            &tx,
            job_id,
            Some(&self.worker_id),
            lease_token,
            result_json,
        )?;

        let event = DomainEvent::new(
            "JobCompleted".to_string(),
            job.project_id.clone(),
            Some(job.id.clone()),
            json!({
                "status": job.status.as_str(),
                "completedAt": job.completed_at,
            }),
        );
        EventBus::record_event_tx(&tx, &event)?;

        tx.commit()?;

        self.event_bus.broadcast(&event);

        Ok(job)
    }

    pub fn fail_job(
        &self,
        job_id: &str,
        error_msg: &str,
        allow_retry: bool,
    ) -> Result<Job, JobError> {
        self.fail_job_with_token(job_id, None, error_msg, allow_retry)
    }

    pub fn fail_job_with_token(
        &self,
        job_id: &str,
        lease_token: Option<&str>,
        error_msg: &str,
        allow_retry: bool,
    ) -> Result<Job, JobError> {
        let mut conn = self.db.conn()?;
        let tx = conn.transaction()?;

        let job = JobRepository::fail_job_with_lease(
            &tx,
            job_id,
            Some(&self.worker_id),
            lease_token,
            error_msg,
            allow_retry,
        )?;

        let event_type = match job.status {
            super::model::JobState::DeadLetter => "JobMovedToDeadLetter",
            _ => "JobFailed",
        };

        let event = DomainEvent::new(
            event_type.to_string(),
            job.project_id.clone(),
            Some(job.id.clone()),
            json!({
                "status": job.status.as_str(),
                "retryCount": job.retry_count,
                "error": job.error,
            }),
        );
        EventBus::record_event_tx(&tx, &event)?;

        tx.commit()?;

        self.event_bus.broadcast(&event);

        Ok(job)
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<Job, JobError> {
        let mut conn = self.db.conn()?;
        let tx = conn.transaction()?;

        let (job, was_cancelled) = JobRepository::cancel_job_with_status(&tx, job_id)?;

        let event = if was_cancelled {
            let evt = DomainEvent::new(
                "JobCancelled".to_string(),
                job.project_id.clone(),
                Some(job.id.clone()),
                json!({
                    "status": job.status.as_str(),
                }),
            );
            EventBus::record_event_tx(&tx, &evt)?;
            Some(evt)
        } else {
            None
        };

        tx.commit()?;

        if let Some(evt) = event {
            self.event_bus.broadcast(&evt);
        }

        Ok(job)
    }

    pub fn list_jobs(
        &self,
        project_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Job>, JobError> {
        let conn = self.db.conn()?;
        JobRepository::list_jobs(&conn, project_id, status, limit)
    }

    /// Reaps expired running jobs whose workers crashed or failed to send heartbeats.
    /// Transactionally persists state transitions and audit events, then broadcasts.
    pub fn reap_expired_leases(&self) -> Result<usize, JobError> {
        let mut conn = self.db.conn()?;
        let tx = conn.transaction()?;

        let (reaped_count, events) = JobRepository::reap_expired_leases_tx(&tx)?;

        for evt in &events {
            EventBus::record_event_tx(&tx, evt)?;
        }

        tx.commit()?;

        for evt in &events {
            self.event_bus.broadcast(evt);
        }

        Ok(reaped_count)
    }
}
