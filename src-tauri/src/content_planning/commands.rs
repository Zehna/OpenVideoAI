use std::sync::Arc;
use tauri::State;

use crate::content_planning::jobs::{
    submit_generate_job, submit_regeneration_job, RegenerationJobParams,
};
use crate::content_planning::model::{
    ContentPlan, ContentPlanRevisionSummary, ContentPlanSummary, PlannerProviderStatus,
    PlanningRequest,
};
use crate::content_planning::provider::ProviderConfigResolver;
use crate::content_planning::service::ContentPlanService;
use crate::jobs::manager::JobManager;
use crate::jobs::model::Job;
use crate::AppState;

// --- Pure testable helpers (cmd_*) ---

pub fn cmd_submit_content_plan_job(
    job_manager: &Arc<JobManager>,
    project_id: String,
    request: PlanningRequest,
    provider: Option<String>,
    provider_config_ref: Option<String>,
) -> Result<Job, String> {
    submit_generate_job(
        job_manager,
        &project_id,
        request,
        provider,
        provider_config_ref,
    )
    .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_get_content_plan(
    plan_service: &Arc<ContentPlanService>,
    plan_id: String,
) -> Result<Option<ContentPlan>, String> {
    plan_service
        .get_plan(&plan_id)
        .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_list_project_content_plans(
    plan_service: &Arc<ContentPlanService>,
    project_id: String,
    include_archived: Option<bool>,
) -> Result<Vec<ContentPlanSummary>, String> {
    plan_service
        .list_project_plans(&project_id, include_archived.unwrap_or(false))
        .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_save_content_plan_revision(
    plan_service: &Arc<ContentPlanService>,
    plan_id: String,
    expected_revision: i64,
    content_plan: ContentPlan,
) -> Result<ContentPlan, String> {
    plan_service
        .save_manual_revision(&plan_id, expected_revision, content_plan)
        .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_submit_content_plan_regeneration(
    job_manager: &Arc<JobManager>,
    params: RegenerationJobParams,
) -> Result<Job, String> {
    submit_regeneration_job(job_manager, params).map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_approve_content_plan(
    plan_service: &Arc<ContentPlanService>,
    plan_id: String,
    expected_revision: i64,
) -> Result<ContentPlanSummary, String> {
    plan_service
        .approve(&plan_id, expected_revision)
        .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_archive_content_plan(
    plan_service: &Arc<ContentPlanService>,
    plan_id: String,
    expected_revision: i64,
) -> Result<ContentPlanSummary, String> {
    plan_service
        .archive(&plan_id, expected_revision)
        .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_get_content_plan_history(
    plan_service: &Arc<ContentPlanService>,
    plan_id: String,
) -> Result<Vec<ContentPlanRevisionSummary>, String> {
    plan_service
        .list_history(&plan_id)
        .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_get_content_plan_revision(
    plan_service: &Arc<ContentPlanService>,
    plan_id: String,
    revision: i64,
) -> Result<Option<ContentPlan>, String> {
    plan_service
        .get_plan_revision(&plan_id, revision)
        .map_err(|e| format!("{}: {}", e.code(), e))
}

pub fn cmd_get_planner_provider_status() -> Result<PlannerProviderStatus, String> {
    Ok(ProviderConfigResolver::get_status())
}

// --- Tauri Commands (using State<'_, AppState>) ---

#[tauri::command]
pub fn submit_content_plan_job(
    state: State<'_, AppState>,
    project_id: String,
    request: PlanningRequest,
    provider: Option<String>,
    provider_config_ref: Option<String>,
) -> Result<Job, String> {
    cmd_submit_content_plan_job(
        &state.job_manager,
        project_id,
        request,
        provider,
        provider_config_ref,
    )
}

#[tauri::command]
pub fn get_content_plan(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<Option<ContentPlan>, String> {
    cmd_get_content_plan(&state.content_plan_service, plan_id)
}

#[tauri::command]
pub fn list_project_content_plans(
    state: State<'_, AppState>,
    project_id: String,
    include_archived: Option<bool>,
) -> Result<Vec<ContentPlanSummary>, String> {
    cmd_list_project_content_plans(&state.content_plan_service, project_id, include_archived)
}

#[tauri::command]
pub fn save_content_plan_revision(
    state: State<'_, AppState>,
    plan_id: String,
    expected_revision: i64,
    content_plan: ContentPlan,
) -> Result<ContentPlan, String> {
    cmd_save_content_plan_revision(
        &state.content_plan_service,
        plan_id,
        expected_revision,
        content_plan,
    )
}

#[tauri::command]
pub fn submit_content_plan_regeneration(
    state: State<'_, AppState>,
    params: RegenerationJobParams,
) -> Result<Job, String> {
    cmd_submit_content_plan_regeneration(&state.job_manager, params)
}

#[tauri::command]
pub fn approve_content_plan(
    state: State<'_, AppState>,
    plan_id: String,
    expected_revision: i64,
) -> Result<ContentPlanSummary, String> {
    cmd_approve_content_plan(&state.content_plan_service, plan_id, expected_revision)
}

#[tauri::command]
pub fn archive_content_plan(
    state: State<'_, AppState>,
    plan_id: String,
    expected_revision: i64,
) -> Result<ContentPlanSummary, String> {
    cmd_archive_content_plan(&state.content_plan_service, plan_id, expected_revision)
}

#[tauri::command]
pub fn get_content_plan_history(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<Vec<ContentPlanRevisionSummary>, String> {
    cmd_get_content_plan_history(&state.content_plan_service, plan_id)
}

#[tauri::command]
pub fn get_content_plan_revision(
    state: State<'_, AppState>,
    plan_id: String,
    revision: i64,
) -> Result<Option<ContentPlan>, String> {
    cmd_get_content_plan_revision(&state.content_plan_service, plan_id, revision)
}

#[tauri::command]
pub fn get_planner_provider_status(
    _state: State<'_, AppState>,
) -> Result<PlannerProviderStatus, String> {
    cmd_get_planner_provider_status()
}
