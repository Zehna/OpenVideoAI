use super::model::{Job, JobCheckpoint, JobSubmission};
use super::repository::{JobError, JobRepository};
use crate::db::DbManager;
use chrono::Utc;
use std::sync::Arc;

pub struct JobManager {
    db: Arc<DbManager>,
    worker_id: String,
}

impl JobManager {
    pub fn new(db: Arc<DbManager>) -> Self {
        let worker_id = format!("worker-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        Self { db, worker_id }
    }

    pub fn with_worker_id(db: Arc<DbManager>, worker_id: String) -> Self {
        Self { db, worker_id }
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    pub fn submit_job(&self, submission: JobSubmission) -> Result<Job, JobError> {
        let conn = self.db.conn()?;
        JobRepository::enqueue(&conn, submission)
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
        let conn = self.db.conn()?;
        JobRepository::heartbeat(&conn, job_id, &self.worker_id, lease_duration_secs)
    }

    pub fn record_checkpoint(
        &self,
        job_id: &str,
        stage: &str,
        checkpoint_data: &str,
    ) -> Result<JobCheckpoint, JobError> {
        let conn = self.db.conn()?;
        JobRepository::record_checkpoint(&conn, job_id, stage, checkpoint_data)
    }

    pub fn get_checkpoints(&self, job_id: &str) -> Result<Vec<JobCheckpoint>, JobError> {
        let conn = self.db.conn()?;
        JobRepository::get_checkpoints(&conn, job_id)
    }

    pub fn complete_job(&self, job_id: &str, result_json: Option<String>) -> Result<Job, JobError> {
        let conn = self.db.conn()?;
        JobRepository::complete_job(&conn, job_id, result_json)
    }

    pub fn fail_job(
        &self,
        job_id: &str,
        error_msg: &str,
        allow_retry: bool,
    ) -> Result<Job, JobError> {
        let conn = self.db.conn()?;
        JobRepository::fail_job(&conn, job_id, error_msg, allow_retry)
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<Job, JobError> {
        let conn = self.db.conn()?;
        JobRepository::cancel_job(&conn, job_id)
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
    pub fn reap_expired_leases(&self) -> Result<usize, JobError> {
        let now = Utc::now().to_rfc3339();
        let conn = self.db.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, retry_count, max_retries 
             FROM jobs 
             WHERE status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1",
        )?;

        let expired: Vec<(String, i32, i32)> = stmt
            .query_map(rusqlite::params![now], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut reaped_count = 0;
        for (job_id, retry_count, max_retries) in expired {
            if retry_count < max_retries {
                let next_count = retry_count + 1;
                conn.execute(
                    "UPDATE jobs 
                     SET status = 'retryable', retry_count = ?1, error = 'Lease expired without heartbeat (worker crashed or terminated)', leased_by = NULL, lease_expires_at = NULL, updated_at = ?2 
                     WHERE id = ?3",
                    rusqlite::params![next_count, now, job_id],
                )?;
                tracing::warn!(job_id = %job_id, retry_count = next_count, "Reaped expired lease, re-queued as retryable");
            } else {
                conn.execute(
                    "UPDATE jobs 
                     SET status = 'dead_letter', error = 'Lease expired without heartbeat (worker crashed or terminated), exhausted max retries', leased_by = NULL, lease_expires_at = NULL, completed_at = ?1, updated_at = ?1 
                     WHERE id = ?2",
                    rusqlite::params![now, job_id],
                )?;
                tracing::error!(job_id = %job_id, "Reaped expired lease, moved to dead_letter");
            }
            reaped_count += 1;
        }

        Ok(reaped_count)
    }
}
