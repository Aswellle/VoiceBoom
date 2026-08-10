// VoiceBoom AI — Tauri 2.0 library entry point
// Contains the application setup and run function

mod audio;
mod asr;
mod commands;
mod db;
mod resources;
mod shortcut;
mod tray;

use audio::capture::AudioCapture;
use asr::streaming::AsrManager;
use db::Database;
use resources::ResourceManager;
use resources::server::ServerManager;
use shortcut::GlobalShortcutManager;
use tauri::Manager;

/// Shared application state
pub struct AppState {
    pub audio_capture: std::sync::Mutex<Option<AudioCapture>>,
    pub asr_manager: std::sync::Mutex<Option<AsrManager>>,
    pub db: std::sync::Mutex<Option<Database>>,
    pub shortcut_manager: std::sync::Mutex<Option<GlobalShortcutManager>>,
    pub resource_manager: std::sync::Mutex<Option<ResourceManager>>,
    pub server_manager: std::sync::Mutex<Option<ServerManager>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            audio_capture: std::sync::Mutex::new(None),
            asr_manager: std::sync::Mutex::new(None),
            db: std::sync::Mutex::new(None),
            shortcut_manager: std::sync::Mutex::new(None),
            resource_manager: std::sync::Mutex::new(None),
            server_manager: std::sync::Mutex::new(None),
        }
    }
}

/// Run the VoiceBoom application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // m11 fix: Use try_init to avoid panic if logger already initialized
    let _ = env_logger::try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_recording,
            commands::stop_recording,
            commands::get_settings,
            commands::save_settings,
            commands::get_history,
            commands::clear_history,
            commands::register_shortcut,
            commands::unregister_shortcut,
            commands::get_audio_devices,
            commands::open_settings,
            commands::get_resource_status,
            commands::get_resource_endpoint,
            commands::start_local_server,
            commands::stop_local_server,
            commands::is_server_running,
            commands::install_model,
            commands::switch_engine,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Initialize database
            let app_dir = handle
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir).ok();
            let db_path = app_dir.join("voiceboom.db");
            let db = Database::new(&db_path).expect("failed to initialize database");
            *app.state::<AppState>().db.lock().unwrap() = Some(db);

            // Initialize ASR manager
            let asr = AsrManager::new();
            *app.state::<AppState>().asr_manager.lock().unwrap() = Some(asr);

            // Initialize audio capture
            let audio = AudioCapture::new();
            *app.state::<AppState>().audio_capture.lock().unwrap() = Some(audio);

            // C6 fix: Initialize shortcut manager
            let shortcut_manager = GlobalShortcutManager::new(handle.clone());
            *app.state::<AppState>().shortcut_manager.lock().unwrap() = Some(shortcut_manager);

            // Initialize resource manager
            let resource_dir = resources::default_resource_dir(&app_dir);
            std::fs::create_dir_all(&resource_dir).ok();

            // Extract bundled resources on first launch
            if let Err(e) = resources::ensure_bundled_resources(&handle) {
                log::warn!("Failed to extract bundled resources: {}", e);
            }

            let resource_manager = ResourceManager::new(resource_dir);
            *app.state::<AppState>().resource_manager.lock().unwrap() = Some(resource_manager);
            log::info!("Resource manager initialized at: {:?}", resources::default_resource_dir(&app_dir));

            // Initialize server manager
            let server_manager = ServerManager::new();
            *app.state::<AppState>().server_manager.lock().unwrap() = Some(server_manager);
            log::info!("Server manager initialized");

            // Initialize system tray
            match tray::create_tray(&handle) {
                Ok(_) => log::info!("System tray created successfully"),
                Err(e) => log::warn!("Failed to create system tray: {}", e),
            }

            log::info!("VoiceBoom initialized successfully");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running VoiceBoom application");
}
