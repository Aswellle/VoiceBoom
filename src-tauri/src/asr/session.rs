//! ASR session-oriented abstraction.
//!
//! Architecture Lock D: Provider event 必须先映射到统一 AsrEvent。
//! Architecture Lock J: 每个后台任务必须可取消、可结束、可观察。
//!
//! This module defines the new session-oriented ASR abstraction:
//! - `AsrEvent`: normalized event model (Partial, SegmentFinal, UtteranceFinal, Error)
//! - `AsrSession`: session-oriented trait replacing `StreamingAsrEngine`
//! - `FakeAsrSession`: configurable test provider
//!
//! Legacy types (`StreamingAsrEngine`, `AsrResult`, `AsrConfig`, `AsrEngineType`)
//! are re-exported from `engine_trait` for backward compatibility.

use async_trait::async_trait;
use super::engine_trait::{AsrConfig, StreamingAsrEngine};

// ── Normalized events ─────────────────────────────────────────────────

/// Normalized ASR events. Every provider maps its native events to this model.
#[derive(Debug, Clone, serde::Serialize)]
pub enum AsrEvent {
    /// Intermediate result — text may still change.
    Partial {
        text: String,
        language: Option<String>,
    },
    /// A segment is stable (provider won't change it), but the speaker may continue.
    SegmentFinal {
        text: String,
        language: Option<String>,
        confidence: Option<f64>,
    },
    /// A complete utterance — the speaker has finished. This is the only event
    /// that should trigger automatic text injection (Architecture Lock F).
    UtteranceFinal {
        text: String,
        language: Option<String>,
        confidence: Option<f64>,
    },
    /// An error occurred during recognition.
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
}

impl AsrEvent {
    /// Whether this event carries finalized text (segment or utterance).
    pub fn is_final(&self) -> bool {
        matches!(self, AsrEvent::SegmentFinal { .. } | AsrEvent::UtteranceFinal { .. })
    }

    /// Whether this is an utterance final (injection trigger).
    pub fn is_utterance_final(&self) -> bool {
        matches!(self, AsrEvent::UtteranceFinal { .. })
    }

    /// The text content of this event, if any.
    pub fn text(&self) -> Option<&str> {
        match self {
            AsrEvent::Partial { text, .. }
            | AsrEvent::SegmentFinal { text, .. }
            | AsrEvent::UtteranceFinal { text, .. } => Some(text),
            AsrEvent::Error { .. } => None,
        }
    }
}

// ── Session trait ──────────────────────────────────────────────────────

/// A session-oriented ASR interface.
///
/// Each recording creates a session. The session consumes audio frames and
/// produces normalized events. Sessions are single-use: after finalize(),
/// the session is complete and cannot be reused.
#[async_trait]
pub trait AsrSession: Send + Sync {
    /// Start the session with the given configuration.
    async fn start(&mut self, config: AsrConfig) -> anyhow::Result<()>;

    /// Push an audio frame into the session.
    async fn push_audio(&mut self, frame: &[f32]) -> anyhow::Result<()>;

    /// Get the next event (non-blocking). Returns None if no event is ready.
    async fn next_event(&mut self) -> anyhow::Result<Option<AsrEvent>>;

    /// Signal end of audio and flush remaining results.
    /// Returns the final utterance text if one was produced.
    async fn finalize(&mut self) -> anyhow::Result<()>;

    /// Shut down the session and release all resources.
    async fn shutdown(&mut self) -> anyhow::Result<()>;
    /// Get the session/engine name.
    fn name(&self) -> &str;

    /// Check if the session is ready to receive audio.
    fn is_ready(&self) -> bool;
}

// ── Legacy compatibility ──────────────────────────────────────────────
///
/// The old `StreamingAsrEngine` / `AsrResult` types are preserved for backward
/// compatibility during the migration. New code should use `AsrSession` / `AsrEvent`.


/// Adapter: wraps a legacy `StreamingAsrEngine` as an `AsrSession`.
/// This allows gradual migration — old adapters keep working while new ones
/// implement `AsrSession` directly.
pub struct LegacySessionAdapter {
    engine: Box<dyn StreamingAsrEngine>,
}

impl LegacySessionAdapter {
    pub fn new(engine: Box<dyn StreamingAsrEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl AsrSession for LegacySessionAdapter {
    async fn start(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        // Legacy engines use initialize() before start().
        self.engine.initialize(config).await
    }

    async fn push_audio(&mut self, frame: &[f32]) -> anyhow::Result<()> {
        self.engine.send_audio(frame).await
    }

    async fn next_event(&mut self) -> anyhow::Result<Option<AsrEvent>> {
        let result = self.engine.receive_result().await?;
        Ok(result.map(|r| {
            if r.is_final {
                AsrEvent::SegmentFinal {
                    text: r.text,
                    language: r.language,
                    confidence: r.confidence,
                }
            } else {
                AsrEvent::Partial {
                    text: r.text,
                    language: r.language,
                }
            }
        }))
    }

    async fn finalize(&mut self) -> anyhow::Result<()> {
        self.engine.flush().await?;
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.engine.close().await
    }

    fn name(&self) -> &str {
        self.engine.name()
    }

    fn is_ready(&self) -> bool {
        self.engine.is_ready()
    }
}

// ── Fake provider for testing ─────────────────────────────────────────

/// A configurable fake ASR provider for integration tests.
/// Can simulate partial/final events, network failures, and out-of-order events.
pub struct FakeAsrSession {
    name: String,
    ready: bool,
    events: Vec<AsrEvent>,
    event_index: usize,
    push_count: u64,
    fail_after_pushes: Option<u64>,
}

impl FakeAsrSession {
    /// Create a fake session with a fixed sequence of events.
    pub fn new(name: &str, events: Vec<AsrEvent>) -> Self {
        Self {
            name: name.to_string(),
            ready: false,
            events,
            event_index: 0,
            push_count: 0,
            fail_after_pushes: None,
        }
    }

    /// Configure the session to fail after N audio pushes.
    pub fn fail_after_pushes(mut self, n: u64) -> Self {
        self.fail_after_pushes = Some(n);
        self
    }

    /// Get the number of audio frames pushed so far.
    pub fn push_count(&self) -> u64 {
        self.push_count
    }
}

#[async_trait]
impl AsrSession for FakeAsrSession {
    async fn start(&mut self, _config: AsrConfig) -> anyhow::Result<()> {
        self.ready = true;
        Ok(())
    }

    async fn push_audio(&mut self, _frame: &[f32]) -> anyhow::Result<()> {
        self.push_count += 1;
        if let Some(limit) = self.fail_after_pushes {
            if self.push_count > limit {
                anyhow::bail!("Fake ASR: simulated failure after {limit} pushes");
            }
        }
        Ok(())
    }

    async fn next_event(&mut self) -> anyhow::Result<Option<AsrEvent>> {
        if self.event_index < self.events.len() {
            let event = self.events[self.event_index].clone();
            self.event_index += 1;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    async fn finalize(&mut self) -> anyhow::Result<()> {
        // Emit any remaining events
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.ready = false;
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AsrConfig {
        AsrConfig {
            engine_type: AsrEngineType::DeepgramStreaming,
            api_key: Some("test-key".into()),
            endpoint: None,
            language: "en".into(),
            vad_sensitivity: 50,
            sample_rate: 16000,
        }
    }

    #[test]
    fn test_asr_event_is_final() {
        assert!(!AsrEvent::Partial {
            text: "hello".into(),
            language: None,
        }
        .is_final());
        assert!(AsrEvent::SegmentFinal {
            text: "hello".into(),
            language: None,
            confidence: None,
        }
        .is_final());
        assert!(AsrEvent::UtteranceFinal {
            text: "hello world".into(),
            language: None,
            confidence: Some(0.95),
        }
        .is_final());
    }

    #[test]
    fn test_asr_event_is_utterance_final() {
        assert!(!AsrEvent::SegmentFinal {
            text: "hello".into(),
            language: None,
            confidence: None,
        }
        .is_utterance_final());
        assert!(AsrEvent::UtteranceFinal {
            text: "hello".into(),
            language: None,
            confidence: None,
        }
        .is_utterance_final());
    }

    #[test]
    fn test_asr_event_text() {
        assert_eq!(
            AsrEvent::Partial {
                text: "hi".into(),
                language: None,
            }
            .text(),
            Some("hi")
        );
        assert_eq!(
            AsrEvent::Error {
                code: "ERR".into(),
                message: "fail".into(),
                retryable: false,
            }
            .text(),
            None
        );
    }

    #[tokio::test]
    async fn test_fake_session_lifecycle() {
        let events = vec![
            AsrEvent::Partial {
                text: "hel".into(),
                language: None,
            },
            AsrEvent::Partial {
                text: "hello".into(),
                language: None,
            },
            AsrEvent::UtteranceFinal {
                text: "hello world".into(),
                language: Some("en".into()),
                confidence: Some(0.9),
            },
        ];
        let mut session = FakeAsrSession::new("fake", events);
        assert!(!session.is_ready());
        session.start(test_config()).await.unwrap();
        assert!(session.is_ready());

        // Push some audio
        session.push_audio(&[0.0; 1024]).await.unwrap();
        session.push_audio(&[0.0; 1024]).await.unwrap();
        assert_eq!(session.push_count(), 2);

        // Consume events
        let e1 = session.next_event().await.unwrap().unwrap();
        assert!(matches!(e1, AsrEvent::Partial { ref text, .. } if text == "hel"));

        let e2 = session.next_event().await.unwrap().unwrap();
        assert!(matches!(e2, AsrEvent::Partial { ref text, .. } if text == "hello"));

        let e3 = session.next_event().await.unwrap().unwrap();
        assert!(matches!(e3, AsrEvent::UtteranceFinal { ref text, .. } if text == "hello world"));
        assert!(e3.is_utterance_final());

        // No more events
        assert!(session.next_event().await.unwrap().is_none());

        session.shutdown().await.unwrap();
        assert!(!session.is_ready());
    }

    #[tokio::test]
    async fn test_fake_session_failure() {
        let mut session = FakeAsrSession::new("fake", vec![])
            .fail_after_pushes(3);
        session.start(test_config()).await.unwrap();

        session.push_audio(&[0.0; 100]).await.unwrap();
        session.push_audio(&[0.0; 100]).await.unwrap();
        session.push_audio(&[0.0; 100]).await.unwrap();
        // 4th push should fail
        assert!(session.push_audio(&[0.0; 100]).await.is_err());
    }

    #[tokio::test]
    async fn test_legacy_adapter_maps_results() {
        // Create a fake legacy engine and wrap it
        let legacy = Box::new(FakeLegacyEngine {
            results: vec![
                AsrResult {
                    text: "partial".into(),
                    is_final: false,
                    language: None,
                    confidence: None,
                },
                AsrResult {
                    text: "final".into(),
                    is_final: true,
                    language: Some("en".into()),
                    confidence: Some(0.8),
                },
            ],
            index: 0,
        });
        let mut adapter = LegacySessionAdapter::new(legacy);
        adapter.start(test_config()).await.unwrap();

        let e1 = adapter.next_event().await.unwrap().unwrap();
        assert!(matches!(e1, AsrEvent::Partial { ref text, .. } if text == "partial"));

        let e2 = adapter.next_event().await.unwrap().unwrap();
        assert!(matches!(e2, AsrEvent::SegmentFinal { ref text, .. } if text == "final"));
        assert!(e2.is_final());
    }

    /// A minimal legacy engine for testing the adapter.
    struct FakeLegacyEngine {
        results: Vec<AsrResult>,
        index: usize,
    }

    #[async_trait]
    impl StreamingAsrEngine for FakeLegacyEngine {
        async fn initialize(&mut self, _: AsrConfig) -> anyhow::Result<()> {
            Ok(())
        }
        async fn send_audio(&mut self, _: &[f32]) -> anyhow::Result<()> {
            Ok(())
        }
        async fn receive_result(&mut self) -> anyhow::Result<Option<AsrResult>> {
            if self.index < self.results.len() {
                let r = self.results[self.index].clone();
                self.index += 1;
                Ok(Some(r))
            } else {
                Ok(None)
            }
        }
        async fn flush(&mut self) -> anyhow::Result<Option<AsrResult>> {
            Ok(None)
        }
        async fn close(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        fn name(&self) -> &str {
            "fake-legacy"
        }
        fn is_ready(&self) -> bool {
            true
        }
    }
}
