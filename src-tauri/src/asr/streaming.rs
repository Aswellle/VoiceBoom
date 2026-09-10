//! ASR session manager — manages the lifecycle of an ASR session connection.
//!
//! Wraps an `AsrSession` (the new session-oriented trait) and provides
//! backward compatibility via `LegacySessionAdapter` for adapters that
//! still implement the old `StreamingAsrEngine` trait.

use super::adapters::{openai_realtime::OpenaiRealtimeAdapter, deepgram::DeepgramAdapter, local::LocalAsrAdapter};
use super::engine_trait::{AsrConfig, AsrEngineType, AsrResult, StreamingAsrEngine};
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
            AsrEngineType::OpenaiWhisper => Box::new(OpenaiRealtimeAdapter::new()),
            AsrEngineType::Deepgram => Box::new(DeepgramAdapter::new()),
            AsrEngineType::WhisperCpp | AsrEngineType::Funasr => {
                let engine: Box<dyn StreamingAsrEngine> = Box::new(LocalAsrAdapter::new());
                Box::new(LegacySessionAdapter::new(engine))
            }
        };

        // Set config (Deepgram needs this before start()) and start.
        session.start(config.clone()).await?;
        // P0 fix: store the session so it isn't dropped.
        self.session = Some(Arc::new(Mutex::new(session)));
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
    /// Flush the session: signal end of audio, then drain all pending events.
    /// Returns the events collected during flush and whether a timeout occurred.
    ///
    /// This replaces the old sleep(500ms) mechanism with event-driven draining:
    /// 1. Call session.finalize() to signal end of audio (no sleep)
    /// 2. Loop draining events until utterance-final or timeout
    /// 3. If timeout, return timeout=true so caller can emit finalization_timeout
    pub async fn finalize_and_drain(
        &self,
        timeout: std::time::Duration,
    ) -> (Vec<AsrEvent>, bool) {
        let mut events = Vec::new();
        let mut timed_out = false;

        // Signal end of audio to the provider.
        if let Some(session) = &self.session {
            let mut s = session.lock().await;
            if let Err(e) = s.finalize().await {
                log::warn!("[AsrManager] finalize error: {}", e);
            }
        }

        // Drain events until utterance-final or timeout.
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = tokio::time::Instant::now();
            if now > deadline {
                timed_out = true;
                log::warn!("[AsrManager] finalize_and_drain timeout after {:?}", timeout);
                break;
            }
            let remaining = deadline - now;

            // Wait for next event with a short poll interval.
            let event = match tokio::time::timeout(
                std::time::Duration::from_millis(100),
                self.receive_event(),
            ).await
            {
                Ok(Ok(event)) => event,
                Ok(Err(_)) => break, // channel error
                Err(_) => continue, // timeout on this poll iteration
            };

            match event {
                Some(e) => {
                    let is_utterance_final = matches!(e, AsrEvent::UtteranceFinal { .. });
                    events.push(e);
                    if is_utterance_final {
                        break;
                    }
                }
                None => {
                    // No event ready — check if we should continue waiting.
                    // Small delay to avoid busy-looping.
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }

        (events, timed_out)
    }

    /// Legacy compatibility: flush returning AsrResult.
    pub async fn flush(&self) -> anyhow::Result<Option<AsrResult>> {
        let (events, _timed_out) = self.finalize_and_drain(std::time::Duration::from_secs(5)).await;
        // Return the last final event as AsrResult.
        for e in events.iter().rev() {
            if e.is_final() {
                return Ok(Some(AsrResult {
                    text: e.text().unwrap_or("").to_string(),
                    is_final: true,
                    language: match e {
                        AsrEvent::SegmentFinal { language, .. }
                        | AsrEvent::UtteranceFinal { language, .. } => language.clone(),
                        _ => None,
                    },
                    confidence: match e {
                        AsrEvent::SegmentFinal { confidence, .. }
                        | AsrEvent::UtteranceFinal { confidence, .. } => *confidence,
                        _ => None,
                    },
                }));
            }
        }
        Ok(None)
    }

    /// Close the current session.
    pub async fn close(&mut self) -> anyhow::Result<()> {
        if let Some(session) = self.session.take() {
            let mut s = session.lock().await;
            let _ = s.shutdown().await;
        }
        self.session = None;
        self.config = None;
        Ok(())
    }

    /// Check if session is active.
    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::session::{AsrEvent, FakeAsrSession, AsrSession};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn test_config() -> AsrConfig {
        AsrConfig {
            engine_type: crate::asr::engine_trait::AsrEngineType::Deepgram,
            api_key: Some("test-key".into()),
            endpoint: None,
            language: "en".into(),
            vad_sensitivity: 50,
            sample_rate: 16000,
        }
    }

    /// Gate 9: Test that finalize_and_drain collects events until utterance-final.
    #[tokio::test]
    async fn test_finalize_and_drain_collects_events() {
        let fake = FakeAsrSession::new("fake", vec![
            AsrEvent::Partial { text: "hello".into(), language: None },
            AsrEvent::UtteranceFinal { text: "hello world".into(), language: None, confidence: None },
        ]);
        let mgr = AsrManager {
            session: Some(Arc::new(Mutex::new(Box::new(fake)))),
            config: Some(test_config()),
        };

        let (events, timed_out) = mgr.finalize_and_drain(std::time::Duration::from_secs(5)).await;
        assert!(!events.is_empty());
        assert!(!timed_out);
    }

    /// Gate 9: Test timeout when provider never finalizes.
    #[tokio::test]
    async fn test_finalize_and_drain_timeout() {
        let fake = FakeAsrSession::new("fake", vec![]);
        let mgr = AsrManager {
            session: Some(Arc::new(Mutex::new(Box::new(fake)))),
            config: Some(test_config()),
        };

        let (events, timed_out) = mgr.finalize_and_drain(std::time::Duration::from_millis(200)).await;
        assert!(timed_out);
        assert!(events.is_empty());
    }

    /// Gate 9: Test no audio scenario (empty session).
    #[tokio::test]
    async fn test_finalize_no_audio() {
        let fake = FakeAsrSession::new("fake", vec![]);
        let mgr = AsrManager {
            session: Some(Arc::new(Mutex::new(Box::new(fake)))),
            config: Some(test_config()),
        };

        let (events, timed_out) = mgr.finalize_and_drain(std::time::Duration::from_millis(200)).await;
        assert!(timed_out);
        assert!(events.is_empty());
    }

    /// Gate 9: Test repeated stop (flush called multiple times).
    #[tokio::test]
    async fn test_repeated_flush() {
        let fake = FakeAsrSession::new("fake", vec![
            AsrEvent::UtteranceFinal { text: "done".into(), language: None, confidence: None },
        ]);
        let mgr = AsrManager {
            session: Some(Arc::new(Mutex::new(Box::new(fake)))),
            config: Some(test_config()),
        };

        let (events1, timed_out1) = mgr.finalize_and_drain(std::time::Duration::from_secs(5)).await;
        assert!(!events1.is_empty());
        assert!(!timed_out1);

        let (events2, timed_out2) = mgr.finalize_and_drain(std::time::Duration::from_millis(200)).await;
        assert!(timed_out2);
    }

    /// Gate 9: Test that finalize signals end of audio (no fixed sleep).
    #[tokio::test]
    async fn test_finalize_signals_without_sleep() {
        let start = std::time::Instant::now();
        let fake = FakeAsrSession::new("fake", vec![]);
        let mgr = AsrManager {
            session: Some(Arc::new(Mutex::new(Box::new(fake)))),
            config: Some(test_config()),
        };

        let (_events, _timed_out) = mgr.finalize_and_drain(std::time::Duration::from_millis(100)).await;
        let elapsed = start.elapsed();

        // Should complete near the timeout, not a fixed 500ms sleep.
        assert!(elapsed < std::time::Duration::from_millis(500),
            "finalize_and_drain took {:?}, expected < 500ms", elapsed);
    }
}
