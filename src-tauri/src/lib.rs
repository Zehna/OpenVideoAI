pub mod artifacts;
pub mod db;
pub mod events;
pub mod health;
pub mod jobs;
pub mod logging;
pub mod projects;
pub mod storage;
pub mod timeline;
pub mod tools;

use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub health_service: health::HealthService,
    pub db_manager: Arc<db::DbManager>,
    pub project_service: Arc<projects::service::ProjectService>,
    pub artifact_importer: Arc<artifacts::importer::ArtifactImporter>,
    pub artifact_resolver: Arc<artifacts::resolver::ArtifactResolver>,
    pub job_manager: Arc<jobs::manager::JobManager>,
    pub event_bus: Arc<events::bus::EventBus>,
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
        .register_asynchronous_uri_scheme_protocol(
            "openvideoai-media",
            move |ctx, req, responder| {
                let app_handle = ctx.app_handle();
                let state = app_handle.state::<AppState>();
                let uri_str = req.uri().to_string();
                let artifact_id = uri_str
                    .strip_prefix("openvideoai-media://artifact/")
                    .or_else(|| uri_str.strip_prefix("openvideoai-media://localhost/artifact/"))
                    .unwrap_or("")
                    .trim_matches('/');
                let range = req.headers().get("range").and_then(|v| v.to_str().ok());

                let resp = match state.db_manager.conn() {
                    Ok(conn) => match timeline::preview::handle_preview_range_request(
                        artifact_id,
                        range,
                        &state.artifact_resolver,
                        &conn,
                    ) {
                        Ok(pr) => pr.into_http_response().unwrap_or_else(|e| {
                            tauri::http::Response::builder()
                                .status(500)
                                .body(e.into_bytes())
                                .unwrap()
                        }),
                        Err(e) => tauri::http::Response::builder()
                            .status(500)
                            .body(e.into_bytes())
                            .unwrap(),
                    },
                    Err(e) => tauri::http::Response::builder()
                        .status(500)
                        .body(e.to_string().into_bytes())
                        .unwrap(),
                };
                responder.respond(resp);
            },
        )
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
                db_manager.clone(),
                local_store,
            ));
            let event_bus = Arc::new(events::bus::EventBus::new(db_manager.clone()));
            let job_manager = Arc::new(jobs::manager::JobManager::with_event_bus(
                db_manager.clone(),
                event_bus.clone(),
            ));
            let _ = job_manager.reap_expired_leases();

            // Bridge domain events to frontend webview
            let app_handle = app.handle().clone();
            event_bus.subscribe(Arc::new(move |evt| {
                use tauri::Emitter;
                let _ = app_handle.emit("domain-event", evt);
            }));

            app.manage(AppState {
                health_service,
                db_manager,
                project_service,
                artifact_importer,
                artifact_resolver,
                job_manager,
                event_bus,
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
            artifacts::commands::list_project_artifacts,
            jobs::commands::submit_job,
            jobs::commands::get_job,
            jobs::commands::cancel_job,
            jobs::commands::list_jobs,
            jobs::commands::get_job_checkpoints,
            events::commands::list_events
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
