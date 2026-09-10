use crate::asr::engine_trait::{AsrConfig, AsrEngineType};
use crate::asr::AsrEvent;
use crate::resources;
use crate::AppState;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique, human-sortable session ID.
/// Format: `{unix_ms}-{counter}` (e.g. `1726000000000-42`).
fn generate_session_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}", ts, counter)
}

/// Emit a `recording:state` event with the current session snapshot.
fn emit_state(app_handle: &AppHandle, state: &crate::session::RecordingSession) {
    let _ = app_handle.emit("recording:state", state.clone());
}

/// Emit an `asr:result` event from an AsrEvent.
fn emit_asr_event(app_handle: &AppHandle, event: &crate::asr::AsrEvent) {
    let _ = app_handle.emit("asr:result", serde_json::json!({
        "text": event.text().unwrap_or(""),
        "is_final": event.is_final(),
        "language": match event {
            crate::asr::AsrEvent::Partial { language, .. }
            | crate::asr::AsrEvent::SegmentFinal { language, .. }
            | crate::asr::AsrEvent::UtteranceFinal { language, .. } => language.clone(),
            _ => None,
        },
        "confidence": match event {
            crate::asr::AsrEvent::SegmentFinal { confidence, .. }
            | crate::asr::AsrEvent::UtteranceFinal { confidence, .. } => *confidence,
            _ => None,
        },
    }));
}
// Tauri command handlers — bridge between frontend and Rust backend


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
    device: Option<String>,
    vadSensitivity: Option<u32>,
) -> Result<(), String> {
    let session_id = generate_session_id();
    let engine_name = engine.clone().unwrap_or_else(|| "openai_whisper".to_string());
    let language_name = language.clone().unwrap_or_else(|| "auto".to_string());

    // ── State transition: Idle/Error → Starting ─────────────────────
    // Replaces RecordingClaim: the session state machine guarantees only one
    // active recording at a time. begin_start() fails if not in Idle/Error.
    {
        let mut session = state.session.lock().map_err(|e| e.to_string())?;
        session.session_id = session_id.clone();
        session.engine = engine_name.clone();
        session.language = language_name.clone();
        session.begin_start().map_err(|e| {
            log::warn!("[session={}] recording.start rejected: {}", session_id, e);
            "已经在录音中，请先停止当前录音".to_string()
        })?;
        emit_state(&app_handle, &session);
    }
    log::info!("[session={}] recording.start engine={}", session_id, engine_name);

    // The previous recording's bridge task may still be finishing its final
    // transcription (seconds, for local engines). Wait for it to wind down
    // instead of rejecting the user — push-to-talk is often used in quick
    // succession, and an error toast there reads as the app being broken.
    {
        const WAIT_STEP: std::time::Duration = std::time::Duration::from_millis(50);
        const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
        let started = std::time::Instant::now();
        loop {
            let active = {
                let session = state.session.lock().map_err(|e| e.to_string())?;
                let active = session.is_active();
                if !active {
                    // Finalize the wait: reset the old session for reuse
                    drop(session);
                    let mut session = state.session.lock().map_err(|e| e.to_string())?;
                    session.reset();
                }
                active
            };
            if !active {
                break;
            }
            if started.elapsed() >= MAX_WAIT {
                let mut session = state.session.lock().map_err(|e| e.to_string())?;
                session.fail("上一段语音仍在识别中，请稍候再试");
                emit_state(&app_handle, &session);
                return Err("上一段语音仍在识别中，请稍候再试".to_string());
            }
            tokio::time::sleep(WAIT_STEP).await;
        }
    }

    let engine_str = engine.as_deref().unwrap_or("openai_whisper");
    let engine_type = parse_engine_type(engine_str);

    // Auto-configure endpoint for local engines (sherpa-onnx)
    let mut resolved_endpoint = endpoint.clone();
    let is_local = matches!(engine_type, AsrEngineType::Funasr);
    log::info!("[session={}] recording.config engine={} is_local={}", session_id, engine_name, is_local);
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
                let mut session = state.session.lock().map_err(|e| e.to_string())?;
                session.fail(&e);
                emit_state(&app_handle, &session);
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
        // Phase 11: Resolve API key from secure storage if not provided.
        let resolved_api_key = match apiKey {
            Some(k) if !k.is_empty() => Some(k),
            _ => {
                let store = crate::secure_keystore::platform_key_store();
                store.retrieve("voiceboom-api-key").ok().flatten()
            }
        };
        let config = AsrConfig {
            engine_type: engine_type,
            api_key: resolved_api_key,
            endpoint: resolved_endpoint.clone(),
            language: language.clone().unwrap_or_else(|| "auto".to_string()),
            vad_sensitivity: vadSensitivity.unwrap_or(50),
            sample_rate: 16000,
        };
        log::info!("[session={}] asr.initialize engine={}", session_id, engine_name);
        match asr.initialize(config).await {
            Ok(()) => {
                log::info!("[session={}] asr.ready", session_id);
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
        let mut session = state.session.lock().map_err(|e| e.to_string())?;
        session.fail("ASR 引擎初始化失败，请检查配置");
        emit_state(&app_handle, &session);
        return Err("ASR 引擎初始化失败，请检查配置".to_string());
    }

    // C2 fix: Start audio capture and get PCM sample receiver
    let mut audio_rx = {
        let mut audio_guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
        if let Some(ref mut audio) = *audio_guard {
            audio.start_recording(device.as_deref()).map_err(|e| e.to_string())?
        } else {
            let mut session = state.session.lock().map_err(|e| e.to_string())?;
            session.fail("Audio capture not initialized");
            emit_state(&app_handle, &session);
            return Err("Audio capture not initialized".to_string());
        }
    };

    // ── State transition: Starting → Recording ──────────────────────
    {
        let mut session = state.session.lock().map_err(|e| e.to_string())?;
        session.mark_recording().map_err(|e| {
            log::error!("[session={}] state transition failed: {}", session_id, e);
            e
        })?;
        emit_state(&app_handle, &session);
    }

    // Spawn bridge task that forwards audio -> ASR -> frontend events.
    //
    // Endpoint detection lives entirely inside the ASR adapter now (sherpa-onnx's
    // Silero VAD for local engines, or the cloud provider's own segmentation).
    // The bridge's only job: push samples in, forward whatever the adapter reports.
    let asr_manager_for_bridge = {
        let guard = state.asr_manager.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    // Phase 9: AsrManager is always initialized in setup, but handle None gracefully.
    let asr_manager_for_bridge = match asr_manager_for_bridge {
        Some(mgr) => mgr,
        None => {
            let mut session = state.session.lock().map_err(|e| e.to_string())?;
            session.fail("ASR manager not initialized");
            emit_state(&app_handle, &session);
            return Err("ASR manager not initialized".to_string());
        }
    };
    let session_for_bridge = state.session.clone();
    let app_handle_clone = app_handle.clone();
    let session_id_clone = session_id.clone();

    // Diagnostic: count frames and emit a heartbeat so the UI can show whether
    // audio is actually flowing into the ASR pipeline.
    let mut frame_count: u64 = 0;
    let mut last_heartbeat = std::time::Instant::now();
    let mut had_partial: bool = false;

    tokio::spawn(async move {
        log::info!("[session={}] bridge.start frames=0", session_id_clone);

        loop {
            match audio_rx.recv().await {
                Some(frame) => {
                    frame_count += 1;

                    // Emit a heartbeat every 500ms so the UI knows audio is flowing.
                    if last_heartbeat.elapsed().as_millis() > 500 {
                        last_heartbeat = std::time::Instant::now();
                        let _ = app_handle_clone.emit("asr:heartbeat", serde_json::json!({
                            "frames": frame_count,
                            "samples": frame.samples.len(),
                        }));
                    }

                    // Push audio to ASR and poll for results.
                    if let Err(e) = asr_manager_for_bridge.send_audio(&frame.samples).await {
                        log::warn!("Failed to send audio frame: {}", e);
                    }
                    // Poll for events every frame so partials/finals surface promptly.
                    match asr_manager_for_bridge.receive_event().await {
                        Ok(Some(event)) => {
                            let is_partial = event.text().map_or(false, |t| !t.trim().is_empty());
                            if is_partial {
                                had_partial = true;
                            }
                            emit_asr_event(&app_handle_clone, &event);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            log::error!("ASR receive_event error: {}", e);
                        }
                    }
                }
                None => {
                    // Channel closed, audio capture stopped — finalize.
                    log::info!("[session={}] recording.flush frames={} had_partial={}", session_id_clone, frame_count, had_partial);

                    // ── State transition: Stopping → Finalizing ───────────
                    if let Ok(mut session) = session_for_bridge.lock() {
                        if session.mark_finalizing().is_ok() {
                            emit_state(&app_handle_clone, &session);
                        }
                    }

                    // Phase 9: Event-driven finalize + drain (no sleep).
                    // Signal end of audio, then drain events until utterance-final or timeout.
                    let timeout = std::time::Duration::from_secs(5);
                    let (events, timed_out) = asr_manager_for_bridge.finalize_and_drain(timeout).await;

                    // Emit all drained events.
                    for event in &events {
                        emit_asr_event(&app_handle_clone, event);
                    }

                    // If timeout, emit finalization_timeout.
                    if timed_out {
                        log::warn!("[session={}] finalization_timeout", session_id_clone);
                        let _ = app_handle_clone.emit("asr:timeout", serde_json::json!({
                            "message": "识别收尾超时，已强制结束",
                        }));
                    }

                    // If no events at all and no partials, emit error.
                    if events.is_empty() && !had_partial {
                        log::warn!("[session={}] no text recognized ({} frames)", session_id_clone, frame_count);
                        let _ = app_handle_clone.emit("asr:error", "没有识别到语音内容，请检查麦克风");
                    }

                    // ── State transition: Finalizing → Idle ────────────────
                    if let Ok(mut session) = session_for_bridge.lock() {
                        if let Err(e) = session.complete() {
                            log::warn!("[session={}] finalizing→idle failed: {}", session.session_id, e);
                        }
                        emit_state(&app_handle_clone, &session);
                        log::info!("[session={}] recording.complete", session.session_id);
                    }
                    break;
                }
            }
        }
    });

    // Emit event to frontend with session_id for end-to-end tracing.
    let _ = app_handle.emit("recording:started", serde_json::json!({
        "session_id": session_id,
    }));
    Ok(())
}

/// Stop audio recording and flush ASR results
#[tauri::command]
pub async fn stop_recording(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // ── State transition: Recording → Stopping ────────────────────
    {
        let mut session = state.session.lock().map_err(|e| e.to_string())?;
        let sid = session.session_id.clone();
        session.begin_stop().map_err(|e| {
            log::warn!("[session={}] recording.stop rejected: {}", sid, e);
            e
        })?;
        emit_state(&app_handle, &session);
        log::info!("[session={}] recording.stop", sid);
    }

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

/// Inject transcribed text into the currently focused input field.
///
/// `mode` selects the strategy: `"clipboard"` (default, win-text-inject on
/// Windows) or `"typing"` (enigo keystroke simulation).
#[tauri::command]
pub async fn inject_text(text: String, mode: Option<String>) -> Result<serde_json::Value, String> {
    if text.is_empty() {
        return Ok(serde_json::json!({ "result": "injected" }));
    }
    let mode = mode
        .and_then(|m| serde_json::from_str::<crate::inject::InjectionMode>(&format!("\"{m}\"")).ok())
        .unwrap_or_default();
    log::info!("inject_text: {} chars, mode={:?}", text.len(), mode);
    let result = crate::inject::inject(&text, &mode);
    let json = serde_json::to_value(&result).map_err(|e| format!("{e}"))?;
    Ok(json)
}

/// Enable or disable automatic startup at system boot.
///
/// Uses the `tauri-plugin-autostart` crate, which handles the platform-specific
/// mechanisms (Windows registry Run key, macOS LaunchAgent, Linux .desktop file).
#[tauri::command]
pub async fn set_auto_start(
    app_handle: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app_handle.autolaunch();
    if enabled {
        manager.enable().map_err(|e| format!("Failed to enable autostart: {e}"))?;
        log::info!("Auto-start enabled");
    } else {
        manager.disable().map_err(|e| format!("Failed to disable autostart: {e}"))?;
        log::info!("Auto-start disabled");
    }
    Ok(())
}

/// Check whether automatic startup is currently enabled.
#[tauri::command]
pub async fn get_auto_start(app_handle: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app_handle.autolaunch();
    manager.is_enabled().map_err(|e| format!("Failed to query autostart: {e}"))
}

/// Persist the cloud API key into OS secure storage.
/// Phase 11: No longer stores plaintext in SQLite.
#[tauri::command]
pub fn save_api_key(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    apiKey: String,
) -> Result<(), String> {
    if apiKey.is_empty() {
        // Delete the key if empty.
        let store = crate::secure_keystore::platform_key_store();
        store.delete("voiceboom-api-key").map_err(|e| e.to_string())?;
        // Also clean up legacy SQLite entry.
        if let Ok(db) = state.db.lock() {
            if let Some(ref db) = *db {
                let _ = db.delete_model_config("apiKey");
            }
        }
        return Ok(());
    }
    let store = crate::secure_keystore::platform_key_store();
    store.store("voiceboom-api-key", &apiKey).map_err(|e| e.to_string())?;
    log::info!("save_api_key: stored {} chars in OS secure storage", apiKey.len());
    Ok(())
}

/// Retrieve the cloud API key from OS secure storage.
/// Phase 11: Reads from secure store, not SQLite.
#[tauri::command]
pub fn get_api_key(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let store = crate::secure_keystore::platform_key_store();
    let result = store.retrieve("voiceboom-api-key").map_err(|e| e.to_string())?;
    if result.is_some() {
        return Ok(result);
    }
    // Fallback: check legacy SQLite entry for migration.
    if let Ok(db) = state.db.lock() {
        if let Some(ref db) = *db {
            if let Ok(Some(key)) = db.get_model_config("apiKey") {
                // Migrate to secure storage.
                let store = crate::secure_keystore::platform_key_store();
                if store.store("voiceboom-api-key", &key).is_ok() {
                    let _ = db.delete_model_config("apiKey");
                    log::info!("get_api_key: migrated legacy key to secure storage");
                }
                return Ok(Some(key));
            }
        }
    }
    Ok(None)
}

/// Get performance metrics for the audio pipeline.
/// Phase 15: Latency instrumentation.
#[tauri::command]
pub fn get_performance_metrics(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    // Return latency metrics from the tracker.
    // For now, return a placeholder — full integration requires
    // shared LatencyTracker state.
    Ok(serde_json::json!({
        "status": "not_fully_integrated",
        "message": "Latency tracking is available via LatencyTracker in asr::latency. Full integration requires shared state in AppState.",
        "pipeline_stages": [
            "t0_capture",
            "t1_enqueue",
            "t2_dequeue",
            "t3_provider_send",
            "t4_provider_partial",
            "t5_frontend_event",
            "t6_injection_start",
            "t7_injection_complete"
        ]
    }))
}
