use super::model::DomainEvent;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub fn list_events(
    state: State<'_, AppState>,
    event_type: Option<String>,
    job_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<DomainEvent>, String> {
    state
        .event_bus
        .list_events(
            event_type.as_deref(),
            job_id.as_deref(),
            limit.unwrap_or(50),
        )
        .map_err(|e| e.to_string())
}
