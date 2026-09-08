pub mod db;
pub mod health;
pub mod logging;
pub mod tools;

use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub health_service: health::HealthService,
}

#[tauri::command]
fn get_health_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<health::HealthSnapshot, String> {
    Ok(state.health_service.get_snapshot())
}

pub fn run() {
    // Initialize structured logging on application launch
    let _ = logging::init_logging();
    tracing::info!("Starting OpenVideoAI desktop application");

    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve application data directory");

            let db_path = app_data_dir.join("openvideoai.db");
            let db_manager =
                Arc::new(db::DbManager::new(db_path).expect("failed to initialize database"));
            db_manager
                .run_migrations()
                .expect("failed to execute migrations");

            let health_service = health::HealthService::new(db_manager);
            app.manage(AppState { health_service });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_health_snapshot])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
