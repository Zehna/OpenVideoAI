pub mod logging;

pub fn run() {
    // Initialize structured logging on application launch
    let _ = logging::init_logging();
    tracing::info!("Starting OpenVideoAI desktop application");

    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
