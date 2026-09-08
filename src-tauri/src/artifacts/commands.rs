use super::importer::ImportedArtifact;
use crate::AppState;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub fn import_artifact(
    project_id: String,
    source_path: String,
    state: State<'_, AppState>,
) -> Result<ImportedArtifact, String> {
    let path = Path::new(&source_path);
    state
        .artifact_importer
        .import(&project_id, path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_project_artifacts(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ImportedArtifact>, String> {
    state
        .artifact_importer
        .list_for_project(&project_id)
        .map_err(|e| e.to_string())
}
