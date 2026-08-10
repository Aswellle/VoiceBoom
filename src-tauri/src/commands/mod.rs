// Tauri command handlers — bridge between frontend and Rust backend

use crate::AppState;
use crate::asr::engine_trait::{AsrConfig, AsrEngineType};
use crate::audio::vad::{VadConfig, VoiceActivityDetector};
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

    // M4 fix: Initialize ASR engine with config
    // Clone the manager out, drop the lock, then await initialization
    let mut asr_clone = {
        let guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    let asr_initialized = if let Some(ref mut asr) = asr_clone {
        let config = AsrConfig {
            engine_type: parse_engine_type(engine.as_deref().unwrap_or("openai_whisper")),
            api_key: apiKey.clone(),
            endpoint: endpoint.clone(),
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
        let _ = app_handle.emit("asr:status", "未配置 API Key 或连接失败，请在设置中配置");
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

/// M10 fix: Open the settings window
#[tauri::command]
pub fn open_settings(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("settings") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Settings window not found".to_string())
    }
}
