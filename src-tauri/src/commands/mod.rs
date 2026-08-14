// Tauri command handlers — bridge between frontend and Rust backend

use crate::AppState;
use crate::asr::engine_trait::{AsrConfig, AsrEngineType};
use crate::resources;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

/// RAII claim on the "starting a recording" critical section.
/// Acquired atomically at the top of start_recording and released on Drop,
/// which covers every early return in the start sequence.
pub struct RecordingClaim<'a> {
    flag: &'a AtomicBool,
}

impl<'a> RecordingClaim<'a> {
    /// Returns Some(claim) if this caller won the race, None if a start is already in flight.
    pub fn try_acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self { flag })
    }
}

impl Drop for RecordingClaim<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

/// RAII guard for bridge_active flag.
/// Ensures the flag is cleared when the bridge task exits, even on panic.
struct BridgeActiveGuard {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for BridgeActiveGuard {
    fn drop(&mut self) {
        self.flag.store(false, std::sync::atomic::Ordering::Release);
        log::debug!("Bridge task exited, bridge_active cleared");
    }
}

/// Whether a local engine is actually usable right now.
///
/// Process bookkeeping alone was unreliable in both directions — a crashed
/// server still looked "running", and one that survived an app restart looked
/// "stopped" — so the HTTP engine is confirmed by probing its port.

/// Parse engine type string to enum
fn parse_engine_type(engine: &str) -> AsrEngineType {
    match engine {
        "openai_whisper" => AsrEngineType::OpenaiWhisper,
        "deepgram" => AsrEngineType::Deepgram,
        "whisper_cpp" | "funasr" => AsrEngineType::Funasr, // Both map to local SenseVoice now
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

    // Guard: prevent double-start using an atomic claim.
    // The claim is held for the whole start sequence and released on any early
    // return via RecordingClaim's Drop, so two concurrent callers can never both
    // reach audio.start_recording().
    let _claim = RecordingClaim::try_acquire(&state.starting)
        .ok_or_else(|| "已经在录音中，请先停止当前录音".to_string())?;

    // The previous recording's bridge task may still be finishing its final
    // transcription (seconds, for local engines). Wait for it to wind down
    // instead of rejecting the user — push-to-talk is often used in quick
    // succession, and an error toast there reads as the app being broken.
    {
        const WAIT_STEP: std::time::Duration = std::time::Duration::from_millis(50);
        const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
        let started = std::time::Instant::now();
        while state.bridge_active.load(std::sync::atomic::Ordering::Acquire) {
            if started.elapsed() >= MAX_WAIT {
                return Err("上一段语音仍在识别中，请稍候再试".to_string());
            }
            tokio::time::sleep(WAIT_STEP).await;
        }
    }

    {
        let audio_guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
        if let Some(ref audio) = *audio_guard {
            if audio.is_recording() {
                return Err("已经在录音中，请先停止当前录音".to_string());
            }
        }
    }

    let engine_str = engine.as_deref().unwrap_or("openai_whisper");
    let engine_type = parse_engine_type(engine_str);

    // Auto-configure endpoint for local engines (sherpa-onnx)
    let mut resolved_endpoint = endpoint.clone();
    let is_local = matches!(engine_type, AsrEngineType::Funasr);
    log::info!("start_recording: engine={:?}, is_local={}, endpoint={:?}", engine_type, is_local, resolved_endpoint);
    if is_local {
        let local_engine = resources::ResourceEngine::SenseVoice;

        // Check model files and build endpoint
        let model_check = {
            let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
            let manager = guard.as_ref().ok_or("Resource manager not initialized")?;

            log::debug!("Looking for models via ResourceManager");
            let vad_path = manager.vad_model_path(local_engine);
            let model_path = manager.model_path(local_engine);
            let tokens_path = manager.tokens_path(local_engine);

            log::debug!("VAD path: {:?}", vad_path);
            log::debug!("Model path: {:?}", model_path);
            log::debug!("Tokens path: {:?}", tokens_path);

            let vad_path = vad_path.ok_or_else(|| "Silero VAD 模型未安装".to_string())?;
            let model_path = model_path.ok_or_else(|| "SenseVoice ONNX 模型未安装".to_string())?;
            let tokens_path = tokens_path.ok_or_else(|| "SenseVoice tokens 文件未安装".to_string())?;

            log::info!("Models found: vad={:?}, model={:?}, tokens={:?}", vad_path, model_path, tokens_path);

            // Build sherpa-onnx endpoint: vad\x1Emodel\x1Etokens
            Ok::<String, String>(format!(
                "{}\x1E{}\x1E{}",
                vad_path.display(),
                model_path.display(),
                tokens_path.display()
            ))
        };

        match model_check {
            Ok(ep) => {
                log::info!("Endpoint resolved: {}", ep);
                resolved_endpoint = Some(ep);
                let _ = app_handle.emit("asr:status", "SenseVoice 本地引擎已就绪");
            }
            Err(e) => {
                log::error!("Model check failed: {}", e);
                let _ = app_handle.emit("asr:error", e.clone());
                return Err(e);
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
            endpoint: resolved_endpoint.clone(),
            language: language.clone().unwrap_or_else(|| "auto".to_string()),
            sample_rate: 16000,
        };
        log::info!("Initializing ASR with endpoint={:?}", resolved_endpoint);
        match asr.initialize(config).await {
            Ok(()) => {
                log::info!("ASR initialized successfully");
                true
            }
            Err(e) => {
                log::error!("ASR initialization failed: {}", e);
                let _ = app_handle.emit("asr:error", format!("ASR 初始化失败: {}", e));
                false
            }
        }
    } else {
        log::error!("ASR manager is None!");
        false
    };
    // Put the initialized manager back
    {
        let mut guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        *guard = asr_clone;
    }

    // If ASR failed to initialize, do NOT start a dead pipeline: the bridge
    // would capture audio into an engine that can't produce results, then blame
    // the microphone on stop. The specific error was already emitted above.
    if !asr_initialized {
        return Err("ASR 引擎初始化失败，请检查配置".to_string());
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

    // Spawn bridge task that forwards audio -> ASR -> frontend events.
    //
    // Endpoint detection lives entirely inside the ASR adapter now (sherpa-onnx's
    // Silero VAD for local engines, or the cloud provider's own segmentation).
    // This task used to run a second, independent energy-based VAD here and
    // decide on its own when to call flush() — with local engines also running
    // Silero VAD internally, the two endpoint detectors raced: one could flush
    // mid-segment while the other was still accumulating, which is what produced
    // truncated/duplicated results and stuck "still processing" states. The
    // bridge's only job now is: push samples in, and forward whatever the
    // adapter reports out — is_final tells the frontend which event it got.
    let asr_for_bridge = {
        let guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    let app_handle_clone = app_handle.clone();

    // Mark bridge as active before spawning. The Arc handle is cloned into the
    // task so it can clear the flag when it exits (see BridgeActiveGuard).
    state.bridge_active.store(true, std::sync::atomic::Ordering::Release);
    let bridge_flag = std::sync::Arc::clone(&state.bridge_active);

    // Diagnostic: count frames and emit a heartbeat so the UI can show whether
    // audio is actually flowing into the ASR pipeline.
    let mut frame_count: u64 = 0;
    let mut last_heartbeat = std::time::Instant::now();
    let mut had_partial: bool = false; // Track if any partial result was emitted

    tokio::spawn(async move {
        // Ensure bridge_active is cleared when task exits (success or panic)
        let _guard = BridgeActiveGuard { flag: bridge_flag };

        loop {
            match audio_rx.recv().await {
                Some(samples) => {
                    frame_count += 1;

                    // Emit a heartbeat every 500ms so the UI knows audio is flowing.
                    if last_heartbeat.elapsed().as_millis() > 500 {
                        last_heartbeat = std::time::Instant::now();
                        let _ = app_handle_clone.emit("asr:heartbeat", serde_json::json!({
                            "frames": frame_count,
                            "samples": samples.len(),
                        }));
                    }

                    if let Some(ref asr) = asr_for_bridge {
                        if let Err(e) = asr.send_audio(&samples).await {
                            log::warn!("Failed to send audio frame: {}", e);
                        }
                        // The adapter's own VAD decides when a segment is ready;
                        // poll it every frame so partials/finals surface promptly.
                        match asr.receive_result().await {
                            Ok(Some(result)) => {
                                if !result.text.trim().is_empty() {
                                    had_partial = true;
                                }
                                let _ = app_handle_clone.emit("asr:result", serde_json::json!({
                                    "text": result.text,
                                    "is_final": result.is_final,
                                    "language": result.language,
                                    "confidence": result.confidence,
                                }));
                            }
                            Ok(None) => {
                                // No result yet — this is normal between segments.
                            }
                            Err(e) => {
                                log::error!("ASR receive_result error: {}", e);
                            }
                        }
                    }
                }
                None => {
                    // Channel closed, audio capture stopped — flush for any
                    // trailing audio the adapter hadn't finalized yet.
                    log::info!("Audio channel closed after {} frames, had_partial={}", frame_count, had_partial);
                    if let Some(ref asr) = asr_for_bridge {
                        match asr.flush().await {
                            Ok(Some(result)) => {
                                let _ = app_handle_clone.emit("asr:result", serde_json::json!({
                                    "text": result.text,
                                    "is_final": true,
                                    "language": result.language,
                                    "confidence": result.confidence,
                                }));
                            }
                            Ok(None) => {
                                // Only emit error if we never got ANY partial results.
                                // If we did get partials, the user saw text and the lack
                                // of a final result is normal (VAD didn't segment it).
                                if had_partial {
                                    log::debug!("Flush empty but had partial results — no error");
                                } else {
                                    log::warn!("Flush returned no text ({} frames processed)", frame_count);
                                    let _ = app_handle_clone.emit("asr:error", "没有识别到语音内容，请检查麦克风");
                                }
                            }
                            Err(e) => {
                                log::error!("ASR flush error: {}", e);
                                let _ = app_handle_clone.emit("asr:error", format!("识别失败: {}", e));
                            }
                        }
                    }
                    break;
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

    if let Some(ref mut audio) = *state.audio_capture.lock().map_err(|e| e.to_string())? {
        audio.stop_recording();
    }

    // The end-of-stream flush is owned by the bridge task: once
    // audio.stop_recording() closes the sample channel, the bridge drains the
    // remaining frames and flushes the engine for the final result. Flushing
    // here too would race that flush on the same shared engine and emit a
    // duplicate/truncated final (see the bridge task's None branch below).
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
                    "model_file_exists": p.model_file_exists,
                    "tokens_file_exists": p.tokens_file_exists,
                    "vad_model_exists": p.vad_model_exists,
                    "size_bytes": p.size_bytes,
                    "updated_at": p.updated_at,
                    "path": p.path.to_string_lossy(),
                    "default_model_filename": p.engine.default_model_filename(),
                    "tokens_filename": p.engine.tokens_filename(),
                    "vad_filename": p.engine.vad_model_filename(),
                })
            })
            .collect();
        Ok(result)
    } else {
        Ok(Vec::new())
    }
}

/// Start a local ASR server (auto-detects binary and model paths)
/// Switch ASR engine and check model availability (sherpa-onnx)
/// Returns the engine status for the UI to display
#[tauri::command]
pub fn switch_engine(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    engine: String,
) -> Result<serde_json::Value, String> {
    log::info!("Switching engine to: {}", engine);

    let engine_type = parse_engine_type(&engine);
    let is_local = matches!(engine_type, AsrEngineType::Funasr); // Only local engine now

    let mut result = serde_json::json!({
        "engine": engine,
        "is_local": is_local,
        "model_installed": false,
        "tokens_installed": false,
        "vad_installed": false,
        "endpoint": "",
        "status": "ok",
    });

    if is_local {
        let local_engine = resources::ResourceEngine::SenseVoice;

        // Check model files in separate scope
        let (model_installed, tokens_installed, vad_installed, endpoint) = {
            let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
            let manager = guard.as_ref().ok_or("Resource manager not initialized")?;

            let model_path = manager.model_path(local_engine);
            let tokens_path = manager.tokens_path(local_engine);
            let vad_path = manager.vad_model_path(local_engine);

            let model_ok = model_path.is_some();
            let tokens_ok = tokens_path.is_some();
            let vad_ok = vad_path.is_some();

            // Build endpoint in sherpa-onnx format: vad\x1Emodel\x1Etokens
            let endpoint = if model_ok && tokens_ok && vad_ok {
                format!(
                    "{}\x1E{}\x1E{}",
                    vad_path.unwrap().display(),
                    model_path.unwrap().display(),
                    tokens_path.unwrap().display()
                )
            } else {
                String::new()
            };

            (model_ok, tokens_ok, vad_ok, endpoint)
        };

        result["model_installed"] = serde_json::Value::Bool(model_installed);
        result["tokens_installed"] = serde_json::Value::Bool(tokens_installed);
        result["vad_installed"] = serde_json::Value::Bool(vad_installed);
        result["endpoint"] = serde_json::Value::String(endpoint.clone());

        if model_installed && tokens_installed && vad_installed {
            result["status"] = "ready".into();
            let _ = app_handle.emit("asr:status", "SenseVoice 模型已就绪");
        } else {
            result["status"] = "model_missing".into();
            let _ = app_handle.emit("asr:status", "SenseVoice 模型文件缺失，请在设置中安装");
        }
    }

    // Notify the floating window to update its engine display
    let _ = app_handle.emit("engine:switched", result.clone());

    Ok(result)
}

/// Install a model file into the models directory for a local engine
#[tauri::command]
pub fn install_model(
    state: State<'_, AppState>,
    engine: String,
    model_path: Option<String>,
    model_paths: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
    let manager = guard.as_ref().ok_or("Resource manager not initialized")?;

    let engine_type = resources::ResourceEngine::from_str(&engine)
        .ok_or_else(|| format!("Unknown engine: {}", engine))?;

    // Accept either a single path or a list. FunASR needs two GGUF files
    // (ASR model + FSMN VAD), so installing several at once is the norm.
    let inputs: Vec<String> = match (model_paths, model_path) {
        (Some(list), _) if !list.is_empty() => list,
        (_, Some(single)) => vec![single],
        _ => return Err("未指定模型文件".to_string()),
    };

    let models_dir = manager.ensure_models_dir(engine_type);
    let mut installed: Vec<String> = Vec::new();

    for input in inputs {
        let source = std::path::PathBuf::from(&input);
        if !source.exists() {
            return Err(format!("路径不存在: {}", input));
        }

        if source.is_dir() {
            // Copy every model-shaped file in the directory. The previous code
            // resolved a filename here but then called fs::copy on the directory
            // itself, which always failed.
            let entries = std::fs::read_dir(&source)
                .map_err(|e| format!("读取目录失败: {}", e))?;
            let mut found = false;
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let is_model = path
                    .extension()
                    .map(|e| {
                        let e = e.to_string_lossy().to_lowercase();
                        e == "onnx" || e == "txt" || e == "bin" || e == "gguf"
                    })
                    .unwrap_or(false);
                if !is_model {
                    continue;
                }
                let name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .ok_or_else(|| "无效的文件名".to_string())?;
                std::fs::copy(&path, models_dir.join(&name))
                    .map_err(|e| format!("复制 {} 失败: {}", name, e))?;
                installed.push(name);
                found = true;
            }
            if !found {
                return Err(format!("目录中没有找到 .onnx/.txt/.bin/.gguf 模型文件: {}", input));
            }
        } else {
            let name = source.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .ok_or_else(|| "无效的文件路径".to_string())?;
            std::fs::copy(&source, models_dir.join(&name))
                .map_err(|e| format!("复制 {} 失败: {}", name, e))?;
            installed.push(name);
        }
    }

    log::info!(
        "Installed {} model file(s) for {}: {:?}",
        installed.len(),
        engine_type.display_name(),
        installed
    );

    // Report what is still missing so the UI can tell the user precisely.
    Ok(serde_json::json!({
        "engine": engine_type.as_str(),
        "installed": installed,
        "model_exists": manager.model_path(engine_type).is_some(),
        "tokens_exists": manager.tokens_path(engine_type).is_some(),
        "vad_exists": manager.vad_model_path(engine_type).is_some(),
        "vad_required": true, // VAD always required for sherpa-onnx
        "vad_filename": engine_type.vad_model_filename(),
        "tokens_filename": engine_type.tokens_filename(),
    }))
}

/// Get the default endpoint for a local engine
#[tauri::command]
pub fn get_resource_endpoint(
    state: State<'_, AppState>,
    engine: String,
) -> Result<String, String> {
    let engine_type = resources::ResourceEngine::from_str(&engine)
        .ok_or_else(|| format!("Unknown engine: {}", engine))?;

    // Build sherpa-onnx endpoint from model paths
    let guard = state.resource_manager.lock().map_err(|e| e.to_string())?;
    let manager = guard.as_ref().ok_or("Resource manager not initialized")?;

    let vad_path = manager.vad_model_path(engine_type)
        .ok_or_else(|| "VAD model not found".to_string())?;
    let model_path = manager.model_path(engine_type)
        .ok_or_else(|| "Model file not found".to_string())?;
    let tokens_path = manager.tokens_path(engine_type)
        .ok_or_else(|| "Tokens file not found".to_string())?;

    Ok(format!(
        "{}\x1E{}\x1E{}",
        vad_path.display(),
        model_path.display(),
        tokens_path.display()
    ))
}

/// Open the settings window.
///
/// The settings window is pre-declared in tauri.conf.json (visible: false), so
/// it is created by Tauri at startup alongside the floating window. This command
/// only shows/focuses it. Runtime window creation is deliberately avoided:
/// dynamically creating a second WebView on Windows while a transparent
/// always-on-top window is live can fail to initialize or crash the whole
/// WebView2 process (blank window, unclosable, other windows go dead).
#[tauri::command]
pub fn open_settings<R: tauri::Runtime>(app_handle: AppHandle<R>) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("settings")
        .ok_or_else(|| "Settings window not found".to_string())?;

    // Keep the settings window always-on-top while shown so it stays above the
    // always-on-top floating window. Previously this toggled the flag true then
    // false synchronously, which on slower WebView2 could drop the settings
    // window back behind the floating bubble (appearing as a no-op). It stays
    // topmost until closed (close-to-hide in lib.rs hides but keeps it alive).
    let _ = window.set_always_on_top(true);
    window.show().map_err(|e| format!("Failed to show settings: {}", e))?;
    window.set_focus().map_err(|e| format!("Failed to focus settings: {}", e))?;

    Ok(())
}
