// Tauri command handlers — bridge between frontend and Rust backend

use crate::AppState;
use crate::asr::engine_trait::{AsrConfig, AsrEngineType};
use crate::audio::vad::{VadConfig, VoiceActivityDetector};
use crate::resources;
use tauri::{AppHandle, Emitter, Manager, State};

/// Parse engine type string to enum
fn parse_engine_type(engine: &str) -> AsrEngineType {
    match engine {
        "openai_whisper" => AsrEngineType::OpenaiWhisper,
        "deepgram" => AsrEngineType::Deepgram,
        "whisper_cpp" => AsrEngineType::WhisperCpp,
        "funasr" => AsrEngineType::Funasr,
        _ => AsrEngineType::OpenaiWhisper,
    }
}

/// Start audio recording and ASR processing
/// M4 fix: Accept engine/language/apiKey/endpoint parameters
#[tauri::command]
pub async fn start_recording(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    engine: Option<String>,
    language: Option<String>,
    apiKey: Option<String>,
    endpoint: Option<String>,
) -> Result<(), String> {
    log::info!("Starting recording...");

    let engine_str = engine.as_deref().unwrap_or("openai_whisper");
    let engine_type = parse_engine_type(engine_str);

    // Automation: For local engines, auto-start the server if needed
    let mut resolved_endpoint = endpoint.clone();
    let is_local = matches!(engine_type, AsrEngineType::WhisperCpp | AsrEngineType::Funasr);
    if is_local {
        let local_engine = match engine_type {
            AsrEngineType::WhisperCpp => Some(resources::ResourceEngine::WhisperCpp),
            AsrEngineType::Funasr => Some(resources::ResourceEngine::FunASR),
            _ => None,
        };
        if let Some(local_engine) = local_engine {
            // Check if server is running; if not, auto-start it
            let server_running = {
                let guard = state.server_manager.lock().map_err(|e| e.to_string())?;
                guard.as_ref().map(|sm| sm.is_running(local_engine)).unwrap_or(false)
            };

            if !server_running {
                // Extract paths from resource_manager in a separate scope to release the lock
                let (server_binary, model_path, vad_model_path) = {
                    let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
                    let manager = guard.as_ref().ok_or("Resource manager not initialized")?;
                    let server_binary = manager.server_binary_path(local_engine)
                        .ok_or_else(|| format!("{} 服务器程序未找到", local_engine.display_name()))?;
                    let model_path = manager.model_path(local_engine)
                        .ok_or_else(|| format!("{} 模型文件未安装，请先在设置中安装模型", local_engine.display_name()))?;
                    let vad_model_path = manager.vad_model_path(local_engine);
                    (server_binary, model_path, vad_model_path)
                };

                let port = match local_engine {
                    resources::ResourceEngine::WhisperCpp => 8080,
                    resources::ResourceEngine::FunASR => 9880,
                };
                let threads = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4);
                let lang = language.clone().unwrap_or_else(|| "auto".to_string());

                // Start server in a separate scope (resource_manager lock already released - no deadlock)
                let start_result = {
                    let guard = state.server_manager.lock().map_err(|e| e.to_string())?;
                    let manager = guard.as_ref().ok_or("Server manager not initialized")?;
                    manager.start_server(
                        local_engine,
                        server_binary,
                        model_path,
                        vad_model_path,
                        &lang,
                        port,
                        threads,
                    )
                };

                match start_result {
                    Ok(ep) => {
                        resolved_endpoint = Some(ep.clone());
                        let _ = app_handle.emit("asr:status", format!("本地 {} 服务器已启动", local_engine.display_name()));
                    }
                    Err(e) => {
                        let _ = app_handle.emit("asr:error", e.to_string());
                        return Err(e.to_string());
                    }
                }
            } else {
                // Server already running - use its endpoint
                resolved_endpoint = {
                    let guard = state.server_manager.lock().map_err(|e| e.to_string())?;
                    guard.as_ref()
                        .and_then(|sm| sm.get_endpoint(local_engine))
                        .or(Some(local_engine.default_endpoint().to_string()))
                };
            }
        }
    }

    // M4 fix: Initialize ASR engine with config
    // Clone the manager out, drop the lock, then await initialization
    let mut asr_clone = {
        let guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    let asr_initialized = if let Some(ref mut asr) = asr_clone {
        let config = AsrConfig {
            engine_type: engine_type,
            api_key: apiKey.clone(),
            endpoint: resolved_endpoint,
            language: language.clone().unwrap_or_else(|| "auto".to_string()),
            sample_rate: 16000,
        };
        match asr.initialize(config).await {
            Ok(()) => true,
            Err(e) => {
                // Emit error event to frontend
                let _ = app_handle.emit("asr:error", format!("ASR 初始化失败: {}", e));
                false
            }
        }
    } else {
        false
    };
    // Put the initialized manager back
    {
        let mut guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        *guard = asr_clone;
    }

    // If ASR failed to initialize, still start audio capture but warn user
    if !asr_initialized {
        let _ = app_handle.emit("asr:status", "ASR 引擎初始化失败，请检查配置");
    }

    // C2 fix: Start audio capture and get PCM sample receiver
    let mut audio_rx = {
        let mut audio_guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
        if let Some(ref mut audio) = *audio_guard {
            audio.start_recording().map_err(|e| e.to_string())?
        } else {
            return Err("Audio capture not initialized".to_string());
        }
    };

    // C3 fix: Spawn bridge task that forwards audio → ASR → frontend events
    let asr_for_bridge = {
        let guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    let app_handle_clone = app_handle.clone();

    // M14 fix: Initialize VAD for speech endpoint detection
    let mut vad = VoiceActivityDetector::new(VadConfig::default());

    tokio::spawn(async move {
        loop {
            // Forward audio samples from capture to ASR
            match audio_rx.recv().await {
                Some(samples) => {
                    // M14 fix: Process audio through VAD for speech detection
                    let vad_state = vad.process_frame(&samples);
                    if vad_state == crate::audio::vad::VadState::SpeechEnd {
                        // Speech ended — flush ASR for final result
                        if let Some(ref asr) = asr_for_bridge {
                            if let Ok(Some(result)) = asr.flush().await {
                                let _ = app_handle_clone.emit("asr:result", serde_json::json!({
                                    "text": result.text,
                                    "is_final": true,
                                    "language": result.language,
                                    "confidence": result.confidence,
                                }));
                            }
                        }
                    }

                    if let Some(ref asr) = asr_for_bridge {
                        let _ = asr.send_audio(&samples).await;
                    }
                }
                None => {
                    // Channel closed, audio capture stopped
                    break;
                }
            }

            // C3 fix: Poll for ASR results and emit to frontend
            if let Some(ref asr) = asr_for_bridge {
                if let Ok(Some(result)) = asr.receive_result().await {
                    let _ = app_handle_clone.emit("asr:result", serde_json::json!({
                        "text": result.text,
                        "is_final": result.is_final,
                        "language": result.language,
                        "confidence": result.confidence,
                    }));
                }
            }
        }
    });

    // Emit event to frontend
    let _ = app_handle.emit("recording:started", ());
    Ok(())
}

/// Stop audio recording and flush ASR results
#[tauri::command]
pub async fn stop_recording(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    log::info!("Stopping recording...");

    if let Some(ref audio) = *state.audio_capture.lock().map_err(|e| e.to_string())? {
        audio.stop_recording();
    }

    // Flush ASR engine for final result — clone to release lock before await
    let asr_clone = {
        let guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    if let Some(asr) = asr_clone {
        match asr.flush().await {
            Ok(Some(result)) => {
                // C3 fix: Emit final result to frontend
                let _ = app_handle.emit("asr:result", serde_json::json!({
                    "text": result.text,
                    "is_final": result.is_final,
                    "language": result.language,
                    "confidence": result.confidence,
                }));
            }
            Ok(None) => {}
            Err(e) => log::error!("ASR flush error: {}", e),
        }
    }

    let _ = app_handle.emit("recording:stopped", ());
    Ok(())
}

/// Get application settings from database
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    if let Some(ref db) = *state.db.lock().map_err(|e| e.to_string())? {
        db.get_all_settings().map_err(|e| e.to_string())
    } else {
        Ok(serde_json::Value::Object(serde_json::Map::new()))
    }
}

/// Save application settings
#[tauri::command]
pub fn save_settings(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(ref db) = *state.db.lock().map_err(|e| e.to_string())? {
        db.set_setting(&key, &value).map_err(|e| e.to_string())
    } else {
        Err("Database not initialized".to_string())
    }
}

/// Get recognition history
#[tauri::command]
pub fn get_history(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    if let Some(ref db) = *state.db.lock().map_err(|e| e.to_string())? {
        db.get_history(limit.unwrap_or(50)).map_err(|e| e.to_string())
    } else {
        Ok(Vec::new())
    }
}

/// Clear recognition history
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(ref db) = *state.db.lock().map_err(|e| e.to_string())? {
        db.clear_history().map_err(|e| e.to_string())
    } else {
        Err("Database not initialized".to_string())
    }
}

/// Register a global shortcut
#[tauri::command]
pub fn register_shortcut(
    shortcut: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(ref mut manager) = *state.shortcut_manager.lock().map_err(|e| e.to_string())? {
        manager.register(&shortcut).map_err(|e| e.to_string())
    } else {
        Err("Shortcut manager not initialized".to_string())
    }
}

/// Unregister the current global shortcut
#[tauri::command]
pub fn unregister_shortcut(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(ref mut manager) = *state.shortcut_manager.lock().map_err(|e| e.to_string())? {
        manager.unregister().map_err(|e| e.to_string())
    } else {
        Err("Shortcut manager not initialized".to_string())
    }
}

/// Get list of available audio input devices
#[tauri::command]
pub fn get_audio_devices(state: State<'_, AppState>) -> Result<Vec<(String, String)>, String> {
    if let Some(ref audio) = *state.audio_capture.lock().map_err(|e| e.to_string())? {
        Ok(audio.list_devices())
    } else {
        Err("Audio capture not initialized".to_string())
    }
}

/// Get resource package status for all local engines
#[tauri::command]
pub fn get_resource_status(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
    if let Some(manager) = guard.as_ref() {
        let packages = manager.get_all_packages();
        let result: Vec<serde_json::Value> = packages
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "engine": p.engine.as_str(),
                    "engine_name": p.engine.display_name(),
                    "version": p.version,
                    "channel": p.channel.as_str(),
                    "channel_name": p.channel.display_name(),
                    "is_bundled": p.is_bundled,
                    "is_ready": p.is_ready,
                    "server_binary_exists": p.server_binary_exists,
                    "model_file_exists": p.model_file_exists,
                    "vad_model_exists": p.vad_model_exists,
                    "size_bytes": p.size_bytes,
                    "updated_at": p.updated_at,
                    "path": p.path.to_string_lossy(),
                    "default_model_filename": p.engine.default_model_filename(),
                    "endpoint": p.engine.default_endpoint(),
                })
            })
            .collect();
        Ok(result)
    } else {
        Ok(Vec::new())
    }
}

/// Start a local ASR server (auto-detects binary and model paths)
#[tauri::command]
pub fn start_local_server(
    state: State<'_, AppState>,
    engine: String,
    language: Option<String>,
) -> Result<String, String> {
    let server_guard = state.server_manager.lock().map_err(|e| e.to_string())?;
    let resource_guard = state.resource_manager.lock().map_err(|e| e.to_string())?;

    let server_manager = server_guard.as_ref().ok_or("Server manager not initialized")?;
    let resource_manager = resource_guard.as_ref().ok_or("Resource manager not initialized")?;

    let engine_type = resources::ResourceEngine::from_str(&engine)
        .ok_or_else(|| format!("Unknown engine: {}", engine))?;

    // Auto-detect server binary
    let server_binary = resource_manager.server_binary_path(engine_type)
        .ok_or_else(|| format!("Server binary not found for {}. Please ensure resources are extracted.", engine))?;

    // Auto-detect model path
    let model_path = resource_manager.model_path(engine_type)
        .ok_or_else(|| format!("Model file not found for {}. Please download the model file to the models directory.", engine))?;

    // Auto-detect VAD model path
    let vad_model_path = resource_manager.vad_model_path(engine_type);

    let port = match engine_type {
        resources::ResourceEngine::WhisperCpp => 8080,
        resources::ResourceEngine::FunASR => 9880,
    };

    let threads = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);

    server_manager.start_server(
        engine_type,
        server_binary,
        model_path,
        vad_model_path,
        &language.unwrap_or_else(|| "auto".to_string()),
        port,
        threads,
    ).map_err(|e| e.to_string())
}

/// Stop a local ASR server
#[tauri::command]
pub fn stop_local_server(state: State<'_, AppState>, engine: String) -> Result<(), String> {
    let server_guard = state.server_manager.lock().map_err(|e| e.to_string())?;
    let server_manager = server_guard.as_ref().ok_or("Server manager not initialized")?;

    let engine_type = resources::ResourceEngine::from_str(&engine)
        .ok_or_else(|| format!("Unknown engine: {}", engine))?;

    server_manager.stop_server(engine_type).map_err(|e| e.to_string())
}

/// Check if a local server is running
#[tauri::command]
pub fn is_server_running(state: State<'_, AppState>, engine: String) -> Result<bool, String> {
    let server_guard = state.server_manager.lock().map_err(|e| e.to_string())?;
    let server_manager = server_guard.as_ref().ok_or("Server manager not initialized")?;

    let engine_type = resources::ResourceEngine::from_str(&engine)
        .ok_or_else(|| format!("Unknown engine: {}", engine))?;

    Ok(server_manager.is_running(engine_type))
}

/// Switch ASR engine and auto-configure local servers
/// Returns the engine status for the UI to display
#[tauri::command]
pub fn switch_engine(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    engine: String,
) -> Result<serde_json::Value, String> {
    log::info!("Switching engine to: {}", engine);

    let engine_type = parse_engine_type(&engine);
    let is_local = matches!(engine_type, AsrEngineType::WhisperCpp | AsrEngineType::Funasr);

    let mut result = serde_json::json!({
        "engine": engine,
        "is_local": is_local,
        "server_running": false,
        "model_installed": false,
        "server_binary_installed": false,
        "endpoint": "",
        "status": "ok",
    });

    if is_local {
        let local_engine = match engine_type {
            AsrEngineType::WhisperCpp => resources::ResourceEngine::WhisperCpp,
            AsrEngineType::Funasr => resources::ResourceEngine::FunASR,
            _ => unreachable!(),
        };

        // Extract status info in separate scopes to avoid holding multiple locks
        let (server_binary_installed, model_installed) = {
            let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
            let manager = guard.as_ref().ok_or("Resource manager not initialized")?;
            (
                manager.server_binary_path(local_engine).is_some(),
                manager.model_path(local_engine).is_some(),
            )
        };

        let server_running = {
            let guard = state.server_manager.lock().map_err(|e| e.to_string())?;
            guard.as_ref().map(|sm| sm.is_running(local_engine)).unwrap_or(false)
        };

        result["server_binary_installed"] = serde_json::Value::Bool(server_binary_installed);
        result["model_installed"] = serde_json::Value::Bool(model_installed);
        result["server_running"] = serde_json::Value::Bool(server_running);
        result["endpoint"] = serde_json::Value::String(local_engine.default_endpoint().to_string());

        // Auto-start the server if model is available but server isn't running
        if model_installed && !server_running {
            // Extract paths in separate scope
            let (server_binary, model_path, vad_model_path) = {
                let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
                let manager = guard.as_ref().ok_or("Resource manager not initialized")?;
                let server_binary = manager.server_binary_path(local_engine)
                    .ok_or_else(|| "服务器程序未找到".to_string())?;
                let model_path = manager.model_path(local_engine)
                    .ok_or_else(|| "模型文件未找到".to_string())?;
                let vad_model_path = manager.vad_model_path(local_engine);
                (server_binary, model_path, vad_model_path)
            };

            let port = match local_engine {
                resources::ResourceEngine::WhisperCpp => 8080,
                resources::ResourceEngine::FunASR => 9880,
            };
            let threads = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4);

            // Start server (locks already released - no deadlock)
            let start_result = {
                let guard = state.server_manager.lock().map_err(|e| e.to_string())?;
                let manager = guard.as_ref().ok_or("Server manager not initialized")?;
                manager.start_server(
                    local_engine,
                    server_binary,
                    model_path,
                    vad_model_path,
                    "auto",
                    port,
                    threads,
                )
            };

            match start_result {
                Ok(endpoint) => {
                    result["server_running"] = serde_json::Value::Bool(true);
                    result["endpoint"] = serde_json::Value::String(endpoint);
                    result["status"] = "server_started".into();
                    let _ = app_handle.emit("asr:status", format!("本地 {} 服务器已启动", local_engine.display_name()));
                }
                Err(e) => {
                    result["status"] = serde_json::Value::String(format!("start_failed: {}", e));
                }
            }
        } else if !model_installed {
            result["status"] = "model_missing".into();
        }
    }

    Ok(result)
}

/// Install a model file into the models directory for a local engine
#[tauri::command]
pub fn install_model(
    state: State<'_, AppState>,
    engine: String,
    model_path: String,
) -> Result<serde_json::Value, String> {
    let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
    let manager = guard.as_ref().ok_or("Resource manager not initialized")?;

    let engine_type = resources::ResourceEngine::from_str(&engine)
        .ok_or_else(|| format!("Unknown engine: {}", engine))?;

    let source = std::path::PathBuf::from(&model_path);
    if !source.exists() {
        return Err(format!("路径不存在: {}", model_path));
    }

    // Determine target filename
    let target_name = if source.is_dir() {
        // If it's a directory, look for the default model filename
        let default_name = engine_type.default_model_filename();
        let full_path = source.join(default_name);
        if !full_path.exists() {
            return Err(format!(
                "目录中没有找到模型文件 {}，请指定模型文件的完整路径",
                default_name
            ));
        }
        default_name.to_string()
    } else {
        // If it's a file, use its name or rename to default
        source.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| "无效的文件路径".to_string())?
    };

    // Copy to models directory
    let models_dir = manager.ensure_models_dir(engine_type);
    let dest = models_dir.join(&target_name);
    std::fs::copy(&source, &dest).map_err(|e| format!("复制模型文件失败: {}", e))?;

    log::info!("Installed model '{}' for {}", target_name, engine_type.display_name());

    Ok(serde_json::json!({
        "engine": engine_type.as_str(),
        "model_file": target_name,
        "model_exists": manager.model_path(engine_type).is_some(),
    }))
}

/// Get the default endpoint for a local engine
#[tauri::command]
pub fn get_resource_endpoint(engine: String) -> Result<String, String> {
    let engine_type = resources::ResourceEngine::from_str(&engine)
        .ok_or_else(|| format!("Unknown engine: {}", engine))?;
    Ok(engine_type.default_endpoint().to_string())
}

/// Open the settings window (creates it programmatically on demand)
#[tauri::command]
pub fn open_settings<R: tauri::Runtime>(app_handle: AppHandle<R>) -> Result<(), String> {
    use tauri::WebviewWindowBuilder;

    // Check if settings window already exists
    if let Some(window) = app_handle.get_webview_window("settings") {
        // Window exists - just show and focus it
        window.show().map_err(|e| format!("Failed to show settings: {}", e))?;
        window.set_focus().map_err(|e| format!("Failed to focus settings: {}", e))?;
        // Also bring to front on Windows
        window.set_always_on_top(true).ok();
        window.set_always_on_top(false).ok();
        return Ok(());
    }

    // Create the settings window programmatically
    let settings_window = WebviewWindowBuilder::new(
        &app_handle,
        "settings",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("VoiceBoom Settings")
    .inner_size(720.0, 560.0)
    .min_inner_size(600.0, 480.0)
    .center()
    .resizable(true)
    .decorations(true)
    .build()
    .map_err(|e| format!("Failed to create settings window: {}", e))?;

    settings_window.show().map_err(|e| format!("Failed to show settings: {}", e))?;
    settings_window.set_focus().map_err(|e| format!("Failed to focus settings: {}", e))?;

    Ok(())
}
