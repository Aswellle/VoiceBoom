// VoiceBoom AI — Tauri 2.0 library entry point
// Contains the application setup and run function

mod audio;
mod asr;
mod commands;
mod db;
mod inject;
mod resources;
mod shortcut;
mod tray;

use audio::capture::AudioCapture;
use asr::streaming::AsrManager;
use db::Database;
use resources::ResourceManager;
use shortcut::GlobalShortcutManager;
use tauri::Manager;

/// Initialize a file-based logger that writes to the app data directory.
/// This is critical for diagnosing issues in the released GUI app where
/// stderr/console output is invisible.
fn init_file_logger() {
    use std::fs::OpenOptions;
    use std::io::Write;

    let log_path = std::env::temp_dir().join("voiceboom_debug.log");

    // Open once (truncating any previous log) and hold the handle for the whole
    // app lifetime. log() locks it per record instead of reopening the file on
    // every call, which avoided interleaved/lost lines from concurrent writes.
    let mut file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(_) => return, // Nothing else we can do if the log file is unwritable.
    };

    let _ = writeln!(
        file,
        "=== VoiceBoom debug log started at {:?} ===",
        std::time::SystemTime::now()
    );
    let _ = writeln!(file, "Log file: {:?}", log_path);

    let _ = log::set_boxed_logger(Box::new(Logger {
        target: std::sync::Mutex::new(file),
    }))
    .map(|()| log::set_max_level(log::LevelFilter::Debug));

    log::info!("File logger initialized at {:?}", log_path);
}

struct Logger {
    target: std::sync::Mutex<std::fs::File>,
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            use std::io::Write;
            if let Ok(mut file) = self.target.lock() {
                let _ = writeln!(
                    file,
                    "[{}] {}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                );
            }
        }
    }

    fn flush(&self) {}
}

/// Shared application state
pub struct AppState {
    pub audio_capture: std::sync::Mutex<Option<AudioCapture>>,
    pub asr_manager: std::sync::Mutex<Option<AsrManager>>,
    pub db: std::sync::Mutex<Option<Database>>,
    pub shortcut_manager: std::sync::Mutex<Option<GlobalShortcutManager>>,
    pub resource_manager: std::sync::Mutex<Option<ResourceManager>>,
    /// Atomic guard against concurrent start_recording calls
    pub starting: std::sync::atomic::AtomicBool,
    /// Atomic flag indicating a bridge task is actively running.
    /// Arc so the spawned bridge task can hold its own handle.
    pub bridge_active: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            audio_capture: std::sync::Mutex::new(None),
            asr_manager: std::sync::Mutex::new(None),
            db: std::sync::Mutex::new(None),
            shortcut_manager: std::sync::Mutex::new(None),
            resource_manager: std::sync::Mutex::new(None),
            starting: std::sync::atomic::AtomicBool::new(false),
            bridge_active: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

/// Run the VoiceBoom application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize file-based logging so we can diagnose runtime issues in the
    // released GUI app (stderr is invisible there).
    init_file_logger();

    log::info!("=== VoiceBoom starting ===");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
            commands::install_model,
            commands::switch_engine,
            commands::inject_text,
            commands::set_auto_start,
            commands::get_auto_start,
            commands::save_api_key,
            commands::get_api_key,
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

            // Initialize system tray
            match tray::create_tray(&handle) {
                Ok(_) => log::info!("System tray created successfully"),
                Err(e) => log::warn!("Failed to create system tray: {}", e),
            }

            // Settings window behavior: closing it hides it instead of destroying it.
            // This keeps the window alive so open_settings can find it and re-show
            // on the next invocation, instead of hitting "window not found".
            if let Some(settings_win) = handle.get_webview_window("settings") {
                let win_for_hide = settings_win.clone();
                settings_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win_for_hide.hide();
                    }
                });
                log::info!("Settings window close-to-hide configured");
            }

            log::info!("VoiceBoom initialized successfully");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building VoiceBoom application")
        .run(|_app_handle, _event| {
            // No explicit cleanup needed - sherpa-onnx resources are managed in-process
        });
}
