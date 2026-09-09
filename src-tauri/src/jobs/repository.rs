use super::model::{Job, JobCheckpoint, JobState, JobSubmission};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JobError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Job not found: {0}")]
    NotFound(String),

    #[error("Job already in terminal state: {0}")]
    TerminalState(String),
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
        result: row.get(7)?,
        error: row.get(8)?,
        retry_count: row.get(9)?,
        max_retries: row.get(10)?,
        leased_by: row.get(11)?,
        lease_expires_at: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        started_at: row.get(15)?,
        completed_at: row.get(16)?,
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

const SELECT_JOB_FIELDS: &str = "id, job_type, status, priority, idempotency_key, project_id, payload, result, error, retry_count, max_retries, leased_by, lease_expires_at, created_at, updated_at, started_at, completed_at";

impl JobRepository {
    /// Enqueue a job, respecting idempotency if specified.
    pub fn enqueue(conn: &Connection, submission: JobSubmission) -> Result<Job, JobError> {
        if let Some(ref key) = submission.idempotency_key {
            let sql = format!(
                "SELECT {} FROM jobs WHERE idempotency_key = ?1",
                SELECT_JOB_FIELDS
            );
            let existing: Option<Job> =
                conn.query_row(&sql, params![key], row_to_job).optional()?;

            if let Some(job) = existing {
                tracing::info!(job_id = %job.id, idempotency_key = %key, "Reused existing idempotent job");
                return Ok(job);
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let priority = submission.priority.unwrap_or(0);
        let max_retries = submission.max_retries.unwrap_or(3);
        let status = JobState::Queued;

        conn.execute(
            "INSERT INTO jobs (id, job_type, status, priority, idempotency_key, project_id, payload, retry_count, max_retries, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?9)",
            params![
                id,
                submission.job_type,
                status.as_str(),
                priority,
                submission.idempotency_key,
                submission.project_id,
                submission.payload,
                max_retries,
                now,
            ],
        )?;

        tracing::info!(job_id = %id, job_type = %submission.job_type, priority = priority, "Enqueued new job");

        Self::get_by_id(conn, &id)?.ok_or_else(|| JobError::NotFound(id))
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
    pub fn acquire_lease(
        conn: &Connection,
        worker_id: &str,
        lease_duration_secs: i64,
    ) -> Result<Option<Job>, JobError> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let lease_expires_at =
            (now_dt + chrono::Duration::seconds(lease_duration_secs)).to_rfc3339();

        let candidate_id: Option<String> = conn
            .query_row(
                "SELECT id FROM jobs 
                 WHERE status = 'queued' 
                    OR (status = 'running' AND lease_expires_at < ?1) 
                    OR (status = 'retryable' AND updated_at <= ?1)
                 ORDER BY priority DESC, created_at ASC 
                 LIMIT 1",
                params![now],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(job_id) = candidate_id {
            conn.execute(
                "UPDATE jobs 
                 SET status = 'running', leased_by = ?1, lease_expires_at = ?2, started_at = COALESCE(started_at, ?3), updated_at = ?3 
                 WHERE id = ?4",
                params![worker_id, lease_expires_at, now, job_id],
            )?;

            tracing::info!(job_id = %job_id, worker = %worker_id, lease_expires = %lease_expires_at, "Acquired job lease");
            let job = Self::get_by_id(conn, &job_id)?.ok_or_else(|| JobError::NotFound(job_id))?;
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    /// Extend lease for active worker while work is in progress.
    pub fn heartbeat(
        conn: &Connection,
        job_id: &str,
        worker_id: &str,
        lease_duration_secs: i64,
    ) -> Result<bool, JobError> {
        let now_dt = Utc::now();
        let now = now_dt.to_rfc3339();
        let lease_expires_at =
            (now_dt + chrono::Duration::seconds(lease_duration_secs)).to_rfc3339();

        let updated_rows = conn.execute(
            "UPDATE jobs 
             SET lease_expires_at = ?1, updated_at = ?2 
             WHERE id = ?3 AND leased_by = ?4 AND status = 'running'",
            params![lease_expires_at, now, job_id, worker_id],
        )?;

        Ok(updated_rows > 0)
    }

    /// Record checkpoint data for an intermediate pipeline stage.
    pub fn record_checkpoint(
        conn: &Connection,
        job_id: &str,
        stage: &str,
        checkpoint_data: &str,
    ) -> Result<JobCheckpoint, JobError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

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
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE jobs 
             SET status = 'completed', result = ?1, leased_by = NULL, lease_expires_at = NULL, completed_at = ?2, updated_at = ?2 
             WHERE id = ?3",
            params![result_json, now, job_id],
        )?;

        tracing::info!(job_id = %job_id, "Completed job");
        Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))
    }

    /// Mark job as failed. If allow_retry is true and retries are available, transitions to retryable; otherwise dead_letter.
    pub fn fail_job(
        conn: &Connection,
        job_id: &str,
        error_msg: &str,
        allow_retry: bool,
    ) -> Result<Job, JobError> {
        let now = Utc::now().to_rfc3339();
        let job =
            Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))?;

        if allow_retry && job.retry_count < job.max_retries {
            let next_count = job.retry_count + 1;
            conn.execute(
                "UPDATE jobs 
                 SET status = 'retryable', retry_count = ?1, error = ?2, leased_by = NULL, lease_expires_at = NULL, updated_at = ?3 
                 WHERE id = ?4",
                params![next_count, error_msg, now, job_id],
            )?;
            tracing::warn!(job_id = %job_id, retry_count = next_count, max_retries = job.max_retries, error = %error_msg, "Job failed, scheduled for retry");
        } else {
            conn.execute(
                "UPDATE jobs 
                 SET status = 'dead_letter', error = ?1, leased_by = NULL, lease_expires_at = NULL, completed_at = ?2, updated_at = ?2 
                 WHERE id = ?3",
                params![error_msg, now, job_id],
            )?;
            tracing::error!(job_id = %job_id, max_retries = job.max_retries, error = %error_msg, "Job exceeded max retries, moved to dead_letter");
        }

        Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))
    }

    /// Cancel a job if not already in terminal state.
    pub fn cancel_job(conn: &Connection, job_id: &str) -> Result<Job, JobError> {
        let now = Utc::now().to_rfc3339();
        let job =
            Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))?;

        match job.status {
            JobState::Completed | JobState::Failed | JobState::Cancelled | JobState::DeadLetter => {
                Ok(job)
            }
            _ => {
                conn.execute(
                    "UPDATE jobs 
                     SET status = 'cancelled', leased_by = NULL, lease_expires_at = NULL, completed_at = ?1, updated_at = ?1 
                     WHERE id = ?2",
                    params![now, job_id],
                )?;
                tracing::info!(job_id = %job_id, "Cancelled job");
                Self::get_by_id(conn, job_id)?.ok_or_else(|| JobError::NotFound(job_id.to_string()))
            }
        }
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

    /// Fetch all checkpoints for a specific job in chronological order.
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
