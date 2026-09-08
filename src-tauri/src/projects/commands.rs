use super::model::Project;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub fn create_project(
    name: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Project, String> {
    state
        .project_service
        .create_project(name, description)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<Project>, String> {
    state
        .project_service
        .list_projects()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_project(project_id: String, state: State<'_, AppState>) -> Result<Project, String> {
    state
        .project_service
        .get_project(&project_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_project(
    project_id: String,
    name: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Project, String> {
    state
        .project_service
        .update_project(&project_id, name, description)
        .map_err(|e| e.to_string())
}
