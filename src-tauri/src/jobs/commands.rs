use super::model::{Job, JobCheckpoint, JobSubmission};
use crate::AppState;
use tauri::State;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn submit_job(
    state: State<'_, AppState>,
    job_type: String,
    payload: String,
    payload_version: Option<i32>,
    priority: Option<i32>,
    idempotency_key: Option<String>,
    project_id: Option<String>,
    max_retries: Option<i32>,
) -> Result<Job, String> {
    let submission = JobSubmission {
        job_type,
        payload,
        payload_version,
        priority,
        idempotency_key,
        project_id,
        max_retries,
    };

    state
        .job_manager
        .submit_job(submission)
        .map_err(|e| e.to_string())
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
    state
        .job_manager
        .cancel_job(&job_id)
        .map_err(|e| e.to_string())
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
