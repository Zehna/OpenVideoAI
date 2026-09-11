use std::sync::Arc;
use tauri::State;

use super::model::*;
use super::mutation::TimelineMutation;
use super::renderspec::{RenderSpecBuilder, RenderSpecV1};
use super::repository::TimelineRepository;
use super::service::TimelineService;
use crate::db::DbManager;
use crate::jobs::manager::JobManager;
use crate::jobs::model::JobSubmission;
use crate::AppState;

pub fn cmd_create_timeline(
    db: &Arc<DbManager>,
    project_id: &str,
    name: &str,
    width: u32,
    height: u32,
    frame_rate: f64,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let id = format!("timeline-{}", uuid::Uuid::new_v4());
    let canvas = CanvasConfig {
        width,
        height,
        frame_rate,
    };
    TimelineRepository::create(&mut conn, &id, project_id, name, canvas).map_err(|e| e.to_string())
}

pub fn cmd_get_timeline(
    db: &Arc<DbManager>,
    timeline_id: &str,
) -> Result<Option<TimelineDocument>, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    TimelineRepository::get_by_id(&conn, timeline_id).map_err(|e| e.to_string())
}

pub fn cmd_list_project_timelines(
    db: &Arc<DbManager>,
    project_id: &str,
) -> Result<Vec<TimelineDocument>, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    TimelineRepository::list_by_project(&conn, project_id).map_err(|e| e.to_string())
}

pub fn cmd_delete_timeline(db: &Arc<DbManager>, timeline_id: &str) -> Result<bool, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    TimelineRepository::delete(&conn, timeline_id).map_err(|e| e.to_string())
}

pub fn cmd_apply_timeline_mutation(
    db: &Arc<DbManager>,
    timeline_id: &str,
    expected_revision: i64,
    mutation: TimelineMutation,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let service = TimelineService::new();
    service
        .apply_mutation(&mut conn, timeline_id, expected_revision, mutation)
        .map_err(|e| e.to_string())
}

pub fn cmd_undo_timeline(
    db: &Arc<DbManager>,
    timeline_id: &str,
    expected_revision: i64,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let service = TimelineService::new();
    service
        .undo(&mut conn, timeline_id, expected_revision)
        .map_err(|e| e.to_string())
}

pub fn cmd_redo_timeline(
    db: &Arc<DbManager>,
    timeline_id: &str,
    expected_revision: i64,
) -> Result<TimelineDocument, String> {
    let mut conn = db.conn().map_err(|e| e.to_string())?;
    let service = TimelineService::new();
    service
        .redo(&mut conn, timeline_id, expected_revision)
        .map_err(|e| e.to_string())
}

pub fn cmd_build_render_spec(
    db: &Arc<DbManager>,
    timeline_id: &str,
) -> Result<RenderSpecV1, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    let doc = TimelineRepository::get_by_id(&conn, timeline_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Timeline not found".to_string())?;
    RenderSpecBuilder::build_from_timeline(&doc, &*conn).map_err(|e| e.to_string())
}

pub fn cmd_submit_timeline_render(
    db: &Arc<DbManager>,
    job_manager: &Arc<JobManager>,
    timeline_id: &str,
    expected_revision: i64,
) -> Result<String, String> {
    let conn = db.conn().map_err(|e| e.to_string())?;
    let doc = TimelineRepository::get_by_id(&conn, timeline_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Timeline not found".to_string())?;

    if doc.revision != expected_revision {
        return Err(format!(
            "Revision conflict: expected {}, actual {}",
            expected_revision, doc.revision
        ));
    }

    let spec = RenderSpecBuilder::build_from_timeline(&doc, &*conn).map_err(|e| e.to_string())?;
    drop(conn);

    let payload = serde_json::json!({
        "schemaVersion": 1,
        "renderSpec": spec,
        "rendererProfileVersion": 1,
    });

    let idempotency_key = format!("render.timeline:{}:{}:1:1", doc.id, doc.revision);
    let submission = JobSubmission {
        job_type: "render.timeline".to_string(),
        priority: Some(10),
        idempotency_key: Some(idempotency_key),
        project_id: Some(doc.project_id),
        payload: payload.to_string(),
        max_retries: Some(3),
        payload_version: Some(1),
    };

    let job = job_manager
        .submit_job(submission)
        .map_err(|e| e.to_string())?;
    Ok(job.id)
}

// Tauri IPC command handlers
#[tauri::command]
pub fn create_timeline(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
    width: u32,
    height: u32,
    frame_rate: f64,
) -> Result<TimelineDocument, String> {
    cmd_create_timeline(
        &state.db_manager,
        &project_id,
        &name,
        width,
        height,
        frame_rate,
    )
}

#[tauri::command]
pub fn get_timeline(
    state: State<'_, AppState>,
    timeline_id: String,
) -> Result<Option<TimelineDocument>, String> {
    cmd_get_timeline(&state.db_manager, &timeline_id)
}

#[tauri::command]
pub fn list_project_timelines(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<TimelineDocument>, String> {
    cmd_list_project_timelines(&state.db_manager, &project_id)
}

#[tauri::command]
pub fn delete_timeline(state: State<'_, AppState>, timeline_id: String) -> Result<bool, String> {
    cmd_delete_timeline(&state.db_manager, &timeline_id)
}

#[tauri::command]
pub fn apply_timeline_mutation(
    state: State<'_, AppState>,
    timeline_id: String,
    expected_revision: i64,
    mutation: TimelineMutation,
) -> Result<TimelineDocument, String> {
    cmd_apply_timeline_mutation(&state.db_manager, &timeline_id, expected_revision, mutation)
}

#[tauri::command]
pub fn undo_timeline(
    state: State<'_, AppState>,
    timeline_id: String,
    expected_revision: i64,
) -> Result<TimelineDocument, String> {
    cmd_undo_timeline(&state.db_manager, &timeline_id, expected_revision)
}

#[tauri::command]
pub fn redo_timeline(
    state: State<'_, AppState>,
    timeline_id: String,
    expected_revision: i64,
) -> Result<TimelineDocument, String> {
    cmd_redo_timeline(&state.db_manager, &timeline_id, expected_revision)
}

#[tauri::command]
pub fn build_render_spec(
    state: State<'_, AppState>,
    timeline_id: String,
) -> Result<RenderSpecV1, String> {
    cmd_build_render_spec(&state.db_manager, &timeline_id)
}

#[tauri::command]
pub fn submit_timeline_render(
    state: State<'_, AppState>,
    timeline_id: String,
    expected_revision: i64,
) -> Result<String, String> {
    cmd_submit_timeline_render(
        &state.db_manager,
        &state.job_manager,
        &timeline_id,
        expected_revision,
    )
}
