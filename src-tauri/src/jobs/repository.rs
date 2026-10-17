use super::model::{Job, JobCheckpoint, JobState, JobSubmission};
use crate::events::model::DomainEvent;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JobError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Database manager error: {0}")]
    DbManager(#[from] crate::db::DbError),

    #[error("Event error: {0}")]
    Event(#[from] crate::events::bus::EventError),

    #[error("Job not found: {0}")]
    NotFound(String),

    #[error("Job already in terminal state: {0}")]
    TerminalState(String),

    #[error("Stale lease or unauthorized worker: {0}")]
    StaleLease(String),
}

pub struct JobRepository;

fn row_to_job(row: &Row<'_>) -> rusqlite::Result<Job> {
    let status_str: String = row.get(2)?;
    let status = JobState::from_str(&status_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(Job {
        id: row.get(0)?,
        job_type: row.get(1)?,
        status,
        priority: row.get(3)?,
        idempotency_key: row.get(4)?,
        project_id: row.get(5)?,
        payload: row.get(6)?,
        payload_version: row.get(7)?,
        result: row.get(8)?,
        error: row.get(9)?,
        retry_count: row.get(10)?,
        max_retries: row.get(11)?,
        leased_by: row.get(12)?,
        lease_token: row.get(13)?,
        lease_expires_at: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        started_at: row.get(17)?,
        completed_at: row.get(18)?,
    })
}

fn row_to_checkpoint(row: &Row<'_>) -> rusqlite::Result<JobCheckpoint> {
    Ok(JobCheckpoint {
        id: row.get(0)?,
        job_id: row.get(1)?,
        stage: row.get(2)?,
        checkpoint_data: row.get(3)?,
        created_at: row.get(4)?,
    })
}

const SELECT_JOB_FIELDS: &str = "id, job_type, status, priority, idempotency_key, project_id, payload, payload_version, result, error, retry_count, max_retries, leased_by, lease_token, lease_expires_at, created_at, updated_at, started_at, completed_at";

impl JobRepository {
    /// Enqueue a job, returning the Job.
    pub fn enqueue(conn: &Connection, submission: JobSubmission) -> Result<Job, JobError> {
        let (job, _) = Self::enqueue_with_status(conn, submission)?;
        Ok(job)
    }

    /// Enqueue a job, returning (Job, is_new). If an identical non-null idempotency_key
    /// already exists or is concurrently inserted, returns the existing job with is_new = false.
    pub fn enqueue_with_status(
        conn: &Connection,
        submission: JobSubmission,
    ) -> Result<(Job, bool), JobError> {
        if let Some(ref key) = submission.idempotency_key {
            let sql = format!(
                "SELECT {} FROM jobs WHERE idempotency_key = ?1",
                SELECT_JOB_FIELDS
            );
            let existing: Option<Job> =
                conn.query_row(&sql, params![key], row_to_job).optional()?;

            if let Some(job) = existing {
                tracing::info!(job_id = %job.id, idempotency_key = %key, "Reused existing idempotent job");
                return Ok((job, false));
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let priority = submission.priority.unwrap_or(0);
        let max_retries = submission.max_retries.unwrap_or(3);
        let payload_version = submission.payload_version.unwrap_or(1);
        let status = JobState::Queued;

        let insert_res = conn.execute(
            "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, project_id, payload, payload_version, retry_count, max_retries, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?10)",
            params![
                id,
                submission.job_type,
                status.as_str(),
                priority,
                submission.idempotency_key,
                submission.project_id,
                submission.payload,
                payload_version,
                max_retries,
                now,
            ],
        );

        match insert_res {
            Ok(_) => {
                tracing::info!(job_id = %id, job_type = %submission.job_type, "Enqueued new job");
                let job = Self::get_by_id(conn, &id)?.ok_or_else(|| JobError::NotFound(id))?;
                Ok((job, true))
            }
            Err(rusqlite::Error::SqliteFailure(err, Some(ref msg)))
                if err.code == rusqlite::ErrorCode::ConstraintViolation
                    && (msg.contains("idempotency_key") || msg.contains("UNIQUE")) =>
            {
                // Concurrently inserted by another thread; fetch and return existing job
                if let Some(ref key) = submission.idempotency_key {
                    let sql = format!(
                        "SELECT {} FROM jobs WHERE idempotency_key = ?1",
                        SELECT_JOB_FIELDS
                    );
                    let existing: Option<Job> =
                        conn.query_row(&sql, params![key], row_to_job).optional()?;
                    if let Some(job) = existing {
                        tracing::info!(job_id = %job.id, idempotency_key = %key, "Caught concurrent duplicate insert, returning existing job");
                        return Ok((job, false));
                    }
                }
                Err(JobError::Db(rusqlite::Error::SqliteFailure(
                    err,
                    Some(msg.clone()),
                )))
            }
            Err(e) => Err(JobError::Db(e)),
        }
    }

    /// Retrieve single job by ID.
    pub fn get_by_id(conn: &Connection, job_id: &str) -> Result<Option<Job>, JobError> {
        let sql = format!("SELECT {} FROM jobs WHERE id = ?1", SELECT_JOB_FIELDS);
        let job = conn
            .query_row(&sql, params![job_id], row_to_job)
            .optional()?;
        Ok(job)
    }

    /// Atomically select and lease the highest priority runnable job.
    /// Uses an atomic Compare-And-Swap (CAS) update and generates a unique lease_token.
    /// If multiple workers race for the same candidate, exactly one succeeds and the other
    /// re-evaluates the candidate queue.
    pub fn acquire_lease(
        conn: &Connection,
        worker_id: &str,
        lease_duration_secs: i64,
    ) -> Result<Option<Job>, JobError> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let lease_expires_at =
            (now_dt + chrono::Duration::seconds(lease_duration_secs)).to_rfc3339();

        loop {
            let candidate_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM jobs 
                     WHERE status = 'queued' 
                        OR (status = 'retryable' AND updated_at <= ?1)
                     ORDER BY priority DESC, created_at ASC 
                     LIMIT 1",
                    params![now],
                    |row| row.get(0),
                )
                .optional()?;

            let Some(job_id) = candidate_id else {
                return Ok(None);
            };

            let lease_token = uuid::Uuid::new_v4().to_string();

            // Atomic CAS update: only update if the job remains in runnable state (queued or retryable).
            // Expired running jobs must be reclaimed via reap_expired_leases to ensure retry accounting.
            let rows_affected = conn.execute(
                "UPDATE jobs 
                 SET status = 'running', 
                     leased_by = ?1, 
                     lease_token = ?2,
                     lease_expires_at = ?3, 
                     started_at = COALESCE(started_at, ?4), 
                     updated_at = ?4 
                 WHERE id = ?5 AND (
                     status = 'queued' 
                     OR (status = 'retryable' AND updated_at <= ?4)
                 )",
                params![worker_id, lease_token, lease_expires_at, now, job_id],
            )?;

            if rows_affected == 1 {
                tracing::info!(job_id = %job_id, worker = %worker_id, lease_token = %lease_token, "Acquired job lease");
                let job =
                    Self::get_by_id(conn, &job_id)?.ok_or_else(|| JobError::NotFound(job_id))?;
                return Ok(Some(job));
            }

            // Another worker took this candidate between SELECT and UPDATE. Loop to try next candidate.
        }
    }

    /// Acquire next runnable job of a specific job_type.
    pub fn acquire_lease_for_type(
        conn: &Connection,
        worker_id: &str,
        job_type: &str,
        lease_duration_secs: i64,
    ) -> Result<Option<Job>, JobError> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let lease_expires_at =
            (now_dt + chrono::Duration::seconds(lease_duration_secs)).to_rfc3339();

        loop {
            let candidate_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM jobs 
                     WHERE job_type = ?1 
                       AND (status = 'queued' OR (status = 'retryable' AND updated_at <= ?2))
                     ORDER BY priority DESC, created_at ASC 
                     LIMIT 1",
                    params![job_type, now],
                    |row| row.get(0),
                )
                .optional()?;

            let Some(job_id) = candidate_id else {
                return Ok(None);
            };

            let lease_token = uuid::Uuid::new_v4().to_string();

            let rows_affected = conn.execute(
                "UPDATE jobs 
                 SET status = 'running', 
                     leased_by = ?1, 
                     lease_token = ?2,
                     lease_expires_at = ?3, 
                     started_at = COALESCE(started_at, ?4), 
                     updated_at = ?4 
                 WHERE id = ?5 AND job_type = ?6 AND (
                     status = 'queued' 
                     OR (status = 'retryable' AND updated_at <= ?4)
                 )",
                params![
                    worker_id,
                    lease_token,
                    lease_expires_at,
                    now,
                    job_id,
                    job_type
                ],
            )?;

            if rows_affected == 1 {
                tracing::info!(job_id = %job_id, worker = %worker_id, lease_token = %lease_token, job_type = %job_type, "Acquired job lease for type");
                let job =
                    Self::get_by_id(conn, &job_id)?.ok_or_else(|| JobError::NotFound(job_id))?;
                return Ok(Some(job));
            }
        }
    }

    /// Extend lease for active worker while work is in progress.
    pub fn heartbeat(
        conn: &Connection,
        job_id: &str,
        worker_id: &str,
        lease_duration_secs: i64,
    ) -> Result<bool, JobError> {
        Self::heartbeat_with_token(conn, job_id, worker_id, None, lease_duration_secs)
    }

    /// Extend lease with optional lease_token check.
    pub fn heartbeat_with_token(
        conn: &Connection,
        job_id: &str,
        worker_id: &str,
        lease_token: Option<&str>,
        lease_duration_secs: i64,
    ) -> Result<bool, JobError> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let lease_expires_at =
            (now_dt + chrono::Duration::seconds(lease_duration_secs)).to_rfc3339();

        let mut query = "UPDATE jobs SET lease_expires_at = ?1, updated_at = ?2 WHERE id = ?3 AND leased_by = ?4 AND status = 'running' AND lease_expires_at >= ?2".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(lease_expires_at),
            Box::new(now),
            Box::new(job_id.to_string()),
            Box::new(worker_id.to_string()),
        ];

        if let Some(tok) = lease_token {
            query.push_str(" AND lease_token = ?");
            params_vec.push(Box::new(tok.to_string()));
        }

        let params_slice: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let updated_rows = conn.execute(&query, params_slice.as_slice())?;

        Ok(updated_rows > 0)
    }

    /// Record checkpoint data for an intermediate pipeline stage.
    pub fn record_checkpoint(
        conn: &Connection,
        job_id: &str,
        stage: &str,
        checkpoint_data: &str,
    ) -> Result<JobCheckpoint, JobError> {
        Self::record_checkpoint_with_lease(conn, job_id, None, None, stage, checkpoint_data)
    }

    /// Record checkpoint verifying worker lease and attempt identity.
    pub fn record_checkpoint_with_lease(
        conn: &Connection,
        job_id: &str,
        worker_id: Option<&str>,
        lease_token: Option<&str>,
        stage: &str,
        checkpoint_data: &str,
    ) -> Result<JobCheckpoint, JobError> {
        let current =
            Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))?;

        if current.status.is_terminal() {
            return Err(JobError::TerminalState(format!(
                "Cannot record checkpoint for job {}: already in terminal state '{}'",
                job_id, current.status
            )));
        }

        let now = Utc::now().to_rfc3339();

        if worker_id.is_some() || lease_token.is_some() {
            if current.status != JobState::Running {
                return Err(JobError::StaleLease(format!(
                    "Cannot record checkpoint for job {}: current status is '{}', not 'running'",
                    job_id, current.status
                )));
            }

            if let Some(expires_at) = &current.lease_expires_at {
                if expires_at.as_str() < now.as_str() {
                    return Err(JobError::StaleLease(format!(
                        "Cannot record checkpoint for job {}: lease expired at {}",
                        job_id, expires_at
                    )));
                }
            }

            if let Some(w) = worker_id {
                if current.leased_by.as_deref() != Some(w) {
                    return Err(JobError::StaleLease(format!(
                        "Worker {} cannot record checkpoint for job {}: currently leased by {:?}",
                        w, job_id, current.leased_by
                    )));
                }
            }

            if let Some(tok) = lease_token {
                if current.lease_token.as_deref() != Some(tok) {
                    return Err(JobError::StaleLease(format!(
                        "Cannot record checkpoint for job {}: lease token mismatch",
                        job_id
                    )));
                }
            }
        }

        let id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO job_checkpoints (id, job_id, stage, checkpoint_data, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, job_id, stage, checkpoint_data, now],
        )?;

        tracing::info!(job_id = %job_id, stage = %stage, checkpoint_id = %id, "Recorded pipeline stage checkpoint");

        Ok(JobCheckpoint {
            id,
            job_id: job_id.to_string(),
            stage: stage.to_string(),
            checkpoint_data: checkpoint_data.to_string(),
            created_at: now,
        })
    }

    /// Mark job as successfully completed with output JSON.
    pub fn complete_job(
        conn: &Connection,
        job_id: &str,
        result_json: Option<String>,
    ) -> Result<Job, JobError> {
        Self::complete_job_with_lease(conn, job_id, None, None, result_json)
    }

    /// Mark job as successfully completed with worker lease verification.
    pub fn complete_job_with_lease(
        conn: &Connection,
        job_id: &str,
        worker_id: Option<&str>,
        lease_token: Option<&str>,
        result_json: Option<String>,
    ) -> Result<Job, JobError> {
        let now = Utc::now().to_rfc3339();

        let mut query = "UPDATE jobs SET status = 'completed', result = ?1, error = NULL, leased_by = NULL, lease_token = NULL, lease_expires_at = NULL, completed_at = ?2, updated_at = ?2 WHERE id = ?3 AND status NOT IN ('completed', 'failed', 'cancelled', 'dead_letter')".to_string();
        if worker_id.is_some() || lease_token.is_some() {
            query.push_str(" AND status = 'running' AND lease_expires_at >= ?2");
        }

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(result_json),
            Box::new(now.clone()),
            Box::new(job_id.to_string()),
        ];

        if let Some(w) = worker_id {
            query.push_str(" AND leased_by = ?");
            params_vec.push(Box::new(w.to_string()));
        }

        if let Some(tok) = lease_token {
            query.push_str(" AND lease_token = ?");
            params_vec.push(Box::new(tok.to_string()));
        }

        let params_slice: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let rows_affected = conn.execute(&query, params_slice.as_slice())?;

        if rows_affected == 1 {
            tracing::info!(job_id = %job_id, "Completed job");
            return Self::get_by_id(conn, job_id)?
                .ok_or_else(|| JobError::NotFound(job_id.to_string()));
        }

        // rows_affected == 0: diagnose reason
        let current =
            Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))?;
        if current.status.is_terminal() {
            return Err(JobError::TerminalState(format!(
                "Cannot complete job {}: already in terminal state '{}'",
                job_id, current.status
            )));
        }

        if let Some(expires_at) = &current.lease_expires_at {
            if expires_at.as_str() < now.as_str() {
                return Err(JobError::StaleLease(format!(
                    "Cannot complete job {}: lease expired at {}",
                    job_id, expires_at
                )));
            }
        }

        Err(JobError::StaleLease(format!(
            "Cannot complete job {}: status is '{}', leased_by is {:?}, lease_token mismatch",
            job_id, current.status, current.leased_by
        )))
    }

    /// Mark job as failed. If allow_retry is true and retries are available, transitions to retryable; otherwise dead_letter.
    pub fn fail_job(
        conn: &Connection,
        job_id: &str,
        error_msg: &str,
        allow_retry: bool,
    ) -> Result<Job, JobError> {
        Self::fail_job_with_lease(conn, job_id, None, None, error_msg, allow_retry)
    }

    /// Mark job as failed with worker lease verification.
    pub fn fail_job_with_lease(
        conn: &Connection,
        job_id: &str,
        worker_id: Option<&str>,
        lease_token: Option<&str>,
        error_msg: &str,
        allow_retry: bool,
    ) -> Result<Job, JobError> {
        let current =
            Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))?;

        if current.status.is_terminal() {
            return Err(JobError::TerminalState(format!(
                "Cannot fail job {}: already in terminal state '{}'",
                job_id, current.status
            )));
        }

        let now = Utc::now().to_rfc3339();

        if worker_id.is_some() || lease_token.is_some() {
            if current.status != JobState::Running {
                return Err(JobError::StaleLease(format!(
                    "Cannot fail job {}: current status is '{}', not 'running'",
                    job_id, current.status
                )));
            }

            if let Some(expires_at) = &current.lease_expires_at {
                if expires_at.as_str() < now.as_str() {
                    return Err(JobError::StaleLease(format!(
                        "Cannot fail job {}: lease expired at {}",
                        job_id, expires_at
                    )));
                }
            }

            if let Some(w) = worker_id {
                if current.leased_by.as_deref() != Some(w) {
                    return Err(JobError::StaleLease(format!(
                        "Worker {} cannot fail job {}: leased by {:?}",
                        w, job_id, current.leased_by
                    )));
                }
            }

            if let Some(tok) = lease_token {
                if current.lease_token.as_deref() != Some(tok) {
                    return Err(JobError::StaleLease(format!(
                        "Cannot fail job {}: lease token mismatch",
                        job_id
                    )));
                }
            }
        }

        if allow_retry && current.retry_count < current.max_retries {
            let next_count = current.retry_count + 1;
            let rows = conn.execute(
                "UPDATE jobs 
                 SET status = 'retryable', retry_count = ?1, error = ?2, leased_by = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?3 
                 WHERE id = ?4 AND status = 'running'",
                params![next_count, error_msg, now, job_id],
            )?;
            if rows == 0 {
                return Err(JobError::StaleLease(format!(
                    "Job {} state changed concurrently",
                    job_id
                )));
            }
            tracing::warn!(job_id = %job_id, retry_count = next_count, max_retries = current.max_retries, error = %error_msg, "Job failed, scheduled for retry");
        } else if allow_retry {
            let rows = conn.execute(
                "UPDATE jobs 
                 SET status = 'dead_letter', error = ?1, leased_by = NULL, lease_token = NULL, lease_expires_at = NULL, completed_at = ?2, updated_at = ?2 
                 WHERE id = ?3 AND status = 'running'",
                params![error_msg, now, job_id],
            )?;
            if rows == 0 {
                return Err(JobError::StaleLease(format!(
                    "Job {} state changed concurrently",
                    job_id
                )));
            }
            tracing::error!(job_id = %job_id, max_retries = current.max_retries, error = %error_msg, "Job exceeded max retries, moved to dead_letter");
        } else {
            let rows = conn.execute(
                "UPDATE jobs 
                 SET status = 'failed', error = ?1, leased_by = NULL, lease_token = NULL, lease_expires_at = NULL, completed_at = ?2, updated_at = ?2 
                 WHERE id = ?3 AND status = 'running'",
                params![error_msg, now, job_id],
            )?;
            if rows == 0 {
                return Err(JobError::StaleLease(format!(
                    "Job {} state changed concurrently",
                    job_id
                )));
            }
            tracing::error!(job_id = %job_id, error = %error_msg, "Job failed with non-retryable error");
        }

        Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))
    }

    /// Cancel a job if not already in terminal state.
    pub fn cancel_job(conn: &Connection, job_id: &str) -> Result<Job, JobError> {
        let (job, _) = Self::cancel_job_with_status(conn, job_id)?;
        Ok(job)
    }

    /// Cancel a job, returning (Job, was_cancelled).
    pub fn cancel_job_with_status(
        conn: &Connection,
        job_id: &str,
    ) -> Result<(Job, bool), JobError> {
        let job =
            Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))?;

        if job.status.is_terminal() {
            return Ok((job, false));
        }

        let now = Utc::now().to_rfc3339();
        let rows_affected = conn.execute(
            "UPDATE jobs 
             SET status = 'cancelled', leased_by = NULL, lease_token = NULL, lease_expires_at = NULL, completed_at = ?1, updated_at = ?1 
             WHERE id = ?2 AND status NOT IN ('completed', 'failed', 'cancelled', 'dead_letter')",
            params![now, job_id],
        )?;

        if rows_affected == 0 {
            let current = Self::get_by_id(conn, job_id)?
                .ok_or_else(|| JobError::NotFound(job_id.to_string()))?;
            return Ok((current, false));
        }

        tracing::info!(job_id = %job_id, "Cancelled job");
        let updated =
            Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))?;
        Ok((updated, true))
    }

    /// Reaps expired running jobs whose workers crashed or failed to send heartbeats,
    /// returning the reaped count and associated DomainEvents.
    pub fn reap_expired_leases_tx(
        conn: &Connection,
    ) -> Result<(usize, Vec<DomainEvent<serde_json::Value>>), JobError> {
        let now = Utc::now().to_rfc3339();

        let mut stmt = conn.prepare(
            "SELECT id, project_id, retry_count, max_retries 
             FROM jobs 
             WHERE status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at < ?1",
        )?;

        let expired: Vec<(String, Option<String>, i32, i32)> = stmt
            .query_map(rusqlite::params![now], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut reaped_count = 0;
        let mut events = Vec::new();

        for (job_id, project_id, retry_count, max_retries) in expired {
            if retry_count < max_retries {
                let next_count = retry_count + 1;
                conn.execute(
                    "UPDATE jobs 
                     SET status = 'retryable', retry_count = ?1, error = 'Lease expired without heartbeat (worker crashed or terminated)', leased_by = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?2 
                     WHERE id = ?3 AND status = 'running'",
                    rusqlite::params![next_count, now, job_id],
                )?;
                tracing::warn!(job_id = %job_id, retry_count = next_count, "Reaped expired lease, re-queued as retryable");

                events.push(DomainEvent::new(
                    "JobLeaseExpired".to_string(),
                    project_id,
                    Some(job_id),
                    serde_json::json!({
                        "retryCount": next_count,
                        "maxRetries": max_retries,
                    }),
                ));
            } else {
                conn.execute(
                    "UPDATE jobs 
                     SET status = 'dead_letter', error = 'Lease expired without heartbeat (worker crashed or terminated), exhausted max retries', leased_by = NULL, lease_token = NULL, lease_expires_at = NULL, completed_at = ?1, updated_at = ?1 
                     WHERE id = ?2 AND status = 'running'",
                    rusqlite::params![now, job_id],
                )?;
                tracing::error!(job_id = %job_id, "Reaped expired lease, moved to dead_letter");

                events.push(DomainEvent::new(
                    "JobMovedToDeadLetter".to_string(),
                    project_id,
                    Some(job_id),
                    serde_json::json!({
                        "reason": "Exhausted max retries upon lease expiration",
                        "retryCount": retry_count,
                    }),
                ));
            }
            reaped_count += 1;
        }

        Ok((reaped_count, events))
    }

    /// List jobs filtered by optional project_id and status.
    pub fn list_jobs(
        conn: &Connection,
        project_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Job>, JobError> {
        let mut sql = format!("SELECT {} FROM jobs WHERE 1=1", SELECT_JOB_FIELDS);
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(pid) = project_id {
            sql.push_str(" AND project_id = ?");
            params_vec.push(Box::new(pid.to_string()));
        }

        if let Some(st) = status {
            sql.push_str(" AND status = ?");
            params_vec.push(Box::new(st.to_string()));
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        params_vec.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), row_to_job)?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row?);
        }
        Ok(jobs)
    }

    /// Retrieve checkpoints for a job ordered chronologically.
    pub fn get_checkpoints(
        conn: &Connection,
        job_id: &str,
    ) -> Result<Vec<JobCheckpoint>, JobError> {
        let mut stmt = conn.prepare(
            "SELECT id, job_id, stage, checkpoint_data, created_at 
             FROM job_checkpoints 
             WHERE job_id = ?1 
             ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map(params![job_id], row_to_checkpoint)?;
        let mut checkpoints = Vec::new();
        for row in rows {
            checkpoints.push(row?);
        }
        Ok(checkpoints)
    }
}
