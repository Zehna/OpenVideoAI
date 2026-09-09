pub mod artifacts;
pub mod db;
pub mod events;
pub mod health;
pub mod jobs;
pub mod logging;
pub mod projects;
pub mod storage;
pub mod tools;

use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub health_service: health::HealthService,
    pub project_service: Arc<projects::service::ProjectService>,
    pub artifact_importer: Arc<artifacts::importer::ArtifactImporter>,
    pub artifact_resolver: Arc<artifacts::resolver::ArtifactResolver>,
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

            let artifacts_dir = app_data_dir.join("artifacts");
            let staging_dir = app_data_dir.join("staging");
            let _ = storage::recovery::sweep_staging_directory(&staging_dir);
            let local_store = Arc::new(storage::local_store::LocalStore::new(artifacts_dir));

            let health_service = health::HealthService::new(db_manager.clone());
            let project_service =
                Arc::new(projects::service::ProjectService::new(db_manager.clone()));
            let artifact_importer = Arc::new(artifacts::importer::ArtifactImporter::new(
                db_manager.clone(),
                local_store.clone(),
                staging_dir,
            ));
            let artifact_resolver = Arc::new(artifacts::resolver::ArtifactResolver::new(
                db_manager,
                local_store,
            ));

            app.manage(AppState {
                health_service,
                project_service,
                artifact_importer,
                artifact_resolver,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_health_snapshot,
            projects::commands::create_project,
            projects::commands::list_projects,
            projects::commands::get_project,
            projects::commands::update_project,
            artifacts::commands::import_artifact,
            artifacts::commands::list_project_artifacts
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
