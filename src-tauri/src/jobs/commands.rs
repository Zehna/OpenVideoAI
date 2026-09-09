use super::model::{Job, JobCheckpoint, JobSubmission};
use crate::events::model::DomainEvent;
use crate::AppState;
use serde_json::json;
use tauri::State;

#[tauri::command]
pub fn submit_job(
    state: State<'_, AppState>,
    job_type: String,
    payload: String,
    priority: Option<i32>,
    idempotency_key: Option<String>,
    project_id: Option<String>,
    max_retries: Option<i32>,
) -> Result<Job, String> {
    let submission = JobSubmission {
        job_type,
        payload,
        priority,
        idempotency_key,
        project_id,
        max_retries,
    };

    let job = state
        .job_manager
        .submit_job(submission)
        .map_err(|e| e.to_string())?;

    let event = DomainEvent::new(
        "JobQueued".to_string(),
        job.project_id.clone(),
        Some(job.id.clone()),
        json!({
            "jobType": job.job_type,
            "priority": job.priority,
            "status": job.status.as_str(),
        }),
    );
    let _ = state.event_bus.publish(event);

    Ok(job)
}

#[tauri::command]
pub fn get_job(state: State<'_, AppState>, job_id: String) -> Result<Option<Job>, String> {
    state
        .job_manager
        .get_job(&job_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) -> Result<Job, String> {
    let job = state
        .job_manager
        .cancel_job(&job_id)
        .map_err(|e| e.to_string())?;

    let event = DomainEvent::new(
        "JobCancelled".to_string(),
        job.project_id.clone(),
        Some(job.id.clone()),
        json!({
            "status": job.status.as_str(),
        }),
    );
    let _ = state.event_bus.publish(event);

    Ok(job)
}

#[tauri::command]
pub fn list_jobs(
    state: State<'_, AppState>,
    project_id: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<Job>, String> {
    state
        .job_manager
        .list_jobs(
            project_id.as_deref(),
            status.as_deref(),
            limit.unwrap_or(50),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_job_checkpoints(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Vec<JobCheckpoint>, String> {
    state
        .job_manager
        .get_checkpoints(&job_id)
        .map_err(|e| e.to_string())
}
