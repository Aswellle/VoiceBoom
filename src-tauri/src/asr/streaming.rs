//! ASR session manager — manages the lifecycle of an ASR session connection.
//!
//! Wraps an `AsrSession` (the new session-oriented trait) and provides
//! backward compatibility via `LegacySessionAdapter` for adapters that
//! still implement the old `StreamingAsrEngine` trait.

use super::engine_trait::{AsrConfig, AsrEngineType, AsrResult, StreamingAsrEngine};
use super::adapters::{openai_whisper::OpenaiWhisperAdapter, deepgram::DeepgramAdapter, local::LocalAsrAdapter};
use super::session::{AsrEvent, AsrSession, LegacySessionAdapter};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Manages the active ASR session.
#[derive(Clone)]
pub struct AsrManager {
    session: Option<Arc<Mutex<Box<dyn AsrSession>>>>,
    config: Option<AsrConfig>,
}

impl AsrManager {
    pub fn new() -> Self {
        Self {
            session: None,
            config: None,
        }
    }

    /// Initialize the ASR session with the given configuration.
    pub async fn initialize(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        // Reuse the resident local adapter across recordings so the SenseVoice
        // ONNX model + Silero VAD (~240 MB) are not reloaded from disk on every
        // push-to-talk press. Cloud adapters are rebuilt per recording.
        let is_local = matches!(config.engine_type, AsrEngineType::Funasr);
        let can_reuse = is_local
            && self.session.is_some()
            && self.config.as_ref().map(|prev| {
                prev.engine_type == config.engine_type
                    && prev.language == config.language
                    && prev.vad_sensitivity == config.vad_sensitivity
            }).unwrap_or(false);

        if can_reuse {
            return Ok(());
        }

        // Shut down the previous session before replacing it.
        if let Some(old) = self.session.take() {
            let mut s = old.lock().await;
            let _ = s.shutdown().await;
        }

        // Create the new session. Deepgram uses AsrSession directly;
        // legacy adapters are wrapped via LegacySessionAdapter.
        let mut session: Box<dyn AsrSession> = match config.engine_type {
            AsrEngineType::OpenaiWhisper => {
                let engine: Box<dyn StreamingAsrEngine> = Box::new(OpenaiWhisperAdapter::new());
                Box::new(LegacySessionAdapter::new(engine))
            }
            AsrEngineType::Deepgram => Box::new(DeepgramAdapter::new()),
            AsrEngineType::WhisperCpp | AsrEngineType::Funasr => {
                let engine: Box<dyn StreamingAsrEngine> = Box::new(LocalAsrAdapter::new());
                Box::new(LegacySessionAdapter::new(engine))
            }
        };

        // Set config (Deepgram needs this before start()) and start.
        session.start(config.clone()).await?;
        self.config = Some(config);
        Ok(())
    }

    /// Send audio data to the active session.
    pub async fn send_audio(&self, audio_data: &[f32]) -> anyhow::Result<()> {
        if let Some(session) = &self.session {
            let mut s = session.lock().await;
            s.push_audio(audio_data).await?;
        }
        Ok(())
    }

    /// Try to receive the next event (non-blocking).
    pub async fn receive_event(&self) -> anyhow::Result<Option<AsrEvent>> {
        if let Some(session) = &self.session {
            let mut s = session.lock().await;
            s.next_event().await
        } else {
            Ok(None)
        }
    }

    /// Legacy compatibility: receive the next result as AsrResult.
    pub async fn receive_result(&self) -> anyhow::Result<Option<AsrResult>> {
        let event = self.receive_event().await?;
        Ok(event.map(|e| match e {
            AsrEvent::Partial { text, language, .. } => AsrResult {
                text,
                is_final: false,
                language,
                confidence: None,
            },
            AsrEvent::SegmentFinal {
                text, language, confidence, ..
            } => AsrResult {
                text,
                is_final: true,
                language,
                confidence,
            },
            AsrEvent::UtteranceFinal {
                text, language, confidence, ..
            } => AsrResult {
                text,
                is_final: true,
                language,
                confidence,
            },
            AsrEvent::Error { message, .. } => AsrResult {
                text: String::new(),
                is_final: false,
                language: None,
                confidence: None,
            },
        }))
    }

    /// Flush the session for final results.
    pub async fn flush(&self) -> anyhow::Result<Option<AsrResult>> {
        if let Some(session) = &self.session {
            let mut s = session.lock().await;
            s.finalize().await?;
        }
        // After finalize, drain remaining events and return the last final one.
        let mut last_final = None;
        loop {
            let event = self.receive_event().await?;
            match event {
                Some(e) if e.is_final() => {
                    last_final = Some(AsrResult {
                        text: e.text().unwrap_or("").to_string(),
                        is_final: true,
                        language: match &e {
                            AsrEvent::SegmentFinal { language, .. }
                            | AsrEvent::UtteranceFinal { language, .. } => language.clone(),
                            _ => None,
                        },
                        confidence: match &e {
                            AsrEvent::SegmentFinal { confidence, .. }
                            | AsrEvent::UtteranceFinal { confidence, .. } => *confidence,
                            _ => None,
                        },
                    });
                }
                Some(_) => continue,
                None => break,
            }
        }
        Ok(last_final)
    }

    /// Close the current session.
    pub async fn close(&self) -> anyhow::Result<()> {
        if let Some(session) = &self.session {
            let mut s = session.lock().await;
            s.shutdown().await?;
        }
        Ok(())
    }

    /// Check if session is active.
    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }
}
