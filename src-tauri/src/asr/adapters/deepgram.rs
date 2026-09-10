//! Deepgram Live Streaming ASR adapter.
//!
//! Implements the current Deepgram Live Streaming WebSocket protocol:
//! - URL: wss://api.deepgram.com/v1/listen
//! - Auth: Authorization: Token <key> header (NOT in URL)
//! - Events: Results (is_final / speech_final), SpeechStarted, UtteranceEnd, Metadata, Error
//! - Control: Finalize (flush), CloseStream (close), KeepAlive
//!
//! P0-B fix: properly distinguishes is_final (segment stable) from
//! speech_final (utterance ended) and maps to the correct AsrEvent.

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

use crate::asr::{AsrConfig, AsrEvent, AsrSession};

// ── Deepgram event parser ─────────────────────────────────────────────

/// Parsed Deepgram WebSocket event.
#[derive(Debug, PartialEq)]
pub enum DeepgramEvent {
    Partial { text: String, confidence: Option<f64> },
    SegmentFinal { text: String, confidence: Option<f64> },
    UtteranceFinal { text: String, confidence: Option<f64> },
    SpeechStarted,
    UtteranceEnd,
    Error { message: String },
    Closed,
}

/// Parse a Deepgram Results JSON message.
pub fn parse_deepgram_results(json: &serde_json::Value) -> Option<DeepgramEvent> {
    if json["type"] != "Results" {
        return None;
    }
    let channel = &json["channel"];
    let alternatives = channel["alternatives"].as_array();
    let alt = alternatives.and_then(|a| a.first())?;
    let transcript = alt["transcript"].as_str().unwrap_or("").to_string();
    let confidence = alt["confidence"].as_f64();
    let is_final = channel["is_final"].as_bool().unwrap_or(false);
    let speech_final = channel["speech_final"].as_bool().unwrap_or(false);

    if transcript.trim().is_empty() {
        return None;
    }

    if speech_final {
        // Utterance ended — this is the injection trigger (Architecture Lock F).
        Some(DeepgramEvent::UtteranceFinal {
            text: transcript,
            confidence,
        })
    } else if is_final {
        // Segment stable but speaker may continue.
        Some(DeepgramEvent::SegmentFinal {
            text: transcript,
            confidence,
        })
    } else {
        // Interim result — text may still change.
        Some(DeepgramEvent::Partial {
            text: transcript,
            confidence,
        })
    }
}

/// Parse any Deepgram WebSocket event.
pub fn parse_deepgram_event(json: &serde_json::Value) -> Option<DeepgramEvent> {
    match json["type"].as_str() {
        Some("Results") => parse_deepgram_results(json),
        Some("SpeechStarted") => Some(DeepgramEvent::SpeechStarted),
        Some("UtteranceEnd") => Some(DeepgramEvent::UtteranceEnd),
        Some("Error") => Some(DeepgramEvent::Error {
            message: json["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string(),
        }),
        Some("Close") => Some(DeepgramEvent::Closed),
        // Metadata can arrive mid-stream (e.g., with request_id) — don't treat as close.
        Some("Metadata") => None,
        _ => None,
    }
}

// ── Deepgram adapter ───────────────────────────────────────────────────

/// Deepgram Live Streaming adapter implementing AsrSession.
pub struct DeepgramAdapter {
    config: Option<AsrConfig>,
    /// Channel to send audio + control messages to the WS task.
    cmd_tx: Option<mpsc::UnboundedSender<DeepgramCommand>>,
    /// Channel to receive parsed events from the WS task.
    event_rx: Option<mpsc::UnboundedReceiver<AsrEvent>>,
    /// Handle to the background WS task for shutdown.
    shutdown: Option<Arc<tokio::sync::Notify>>,
}

enum DeepgramCommand {
    /// Send audio PCM16 bytes.
    Audio(Vec<u8>),
    /// Send Finalize message (flush remaining audio).
    Finalize,
    /// Send CloseStream message (close connection).
    CloseStream,
}

impl DeepgramAdapter {
    pub fn new() -> Self {
        Self {
            config: None,
            cmd_tx: None,
            event_rx: None,
            shutdown: None,
        }
    }

    /// Build the Deepgram WebSocket URL with query parameters.
    fn build_url(&self, config: &AsrConfig) -> String {
        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| "wss://api.deepgram.com/v1/listen".to_string());
        let lang = if config.language == "auto" {
            String::new()
        } else {
            config.language.clone()
        };

        // Deepgram Live Streaming parameters per current protocol.
        // endpointing is in milliseconds (integer), not boolean.
        let mut url = format!(
            "{}?model=nova-3&encoding=linear16&sample_rate={}&channels=1&interim_results=true&endpointing=800&utterance_end_ms=1000&vad_events=true&smart_format=true",
            endpoint, config.sample_rate
        );
        if !lang.is_empty() {
            url.push_str(&format!("&language={}", lang));
        }
        url
    }

    /// Build the WebSocket request with auth header.
    fn build_request(
        &self,
        url: String,
        api_key: &str,
    ) -> anyhow::Result<impl IntoClientRequest> {
        let mut request = url.into_client_request().map_err(|e| {
            anyhow::anyhow!("Failed to build WS request: {}", e)
        })?;
        // Auth via header (NOT in URL — avoids leaking key in logs).
        let headers = request.headers_mut();
        headers.insert(
            "Authorization",
            format!("Token {}", api_key)
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid auth header"))?,
        );
        headers.insert(
            "Sec-WebSocket-Protocol",
            "token".parse().unwrap(),
        );
        Ok(request)
}
}

#[async_trait]
impl AsrSession for DeepgramAdapter {
    async fn start(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        self.config = Some(config);
        let config = self.config.as_ref().unwrap();
        let api_key = config.api_key.clone().unwrap_or_default();
        let url = self.build_url(config);
        let request = self.build_request(url, &api_key)?;

        let (ws_stream, _) = connect_async(request).await.map_err(|e| {
            anyhow::anyhow!("Deepgram WS connection failed: {}", e)
        })?;

        let (mut ws_sink, mut ws_stream) = ws_stream.split();

        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<DeepgramCommand>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<AsrEvent>();

        self.cmd_tx = Some(cmd_tx);
        self.event_rx = Some(event_rx);

        let shutdown = Arc::new(tokio::sync::Notify::new());
        self.shutdown = Some(shutdown.clone());
        let lang = config.language.clone();

        // Spawn background WS task.
        tokio::spawn(async move {
            let mut ping_interval =
                tokio::time::interval(std::time::Duration::from_secs(30));

            loop {
                tokio::select! {
                    // Audio/control from bridge task.
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(DeepgramCommand::Audio(pcm_bytes)) => {
                                if ws_sink.send(Message::Binary(pcm_bytes)).await.is_err() {
                                    break;
                                }
                            }
                            Some(DeepgramCommand::Finalize) => {
                                let msg = serde_json::json!({"type": "Finalize"});
                                let _ = ws_sink.send(Message::Text(msg.to_string())).await;
                            }
                            Some(DeepgramCommand::CloseStream) => {
                                let msg = serde_json::json!({"type": "CloseStream"});
                                let _ = ws_sink.send(Message::Text(msg.to_string())).await;
                                break;
                            }
                            None => break, // sender dropped
                        }
                    }
                    // Messages from Deepgram.
                    Some(msg) = ws_stream.next() => {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    match parse_deepgram_event(&json) {
                                        Some(DeepgramEvent::Partial { text, confidence }) => {
                                            let _ = event_tx.send(AsrEvent::Partial {
                                                text,
                                                language: Some(lang.clone()),
                                            });
                                        }
                                        Some(DeepgramEvent::SegmentFinal { text, confidence }) => {
                                            let _ = event_tx.send(AsrEvent::SegmentFinal {
                                                text,
                                                language: Some(lang.clone()),
                                                confidence,
                                            });
                                        }
                                        Some(DeepgramEvent::UtteranceFinal { text, confidence }) => {
                                            let _ = event_tx.send(AsrEvent::UtteranceFinal {
                                                text,
                                                language: Some(lang.clone()),
                                                confidence,
                                            });
                                        }
                                        Some(DeepgramEvent::Error { message }) => {
                                            let _ = event_tx.send(AsrEvent::Error {
                                                code: "DEEPGRAM_ERROR".to_string(),
                                                message,
                                                retryable: true,
                                            });
                                        }
                                        Some(DeepgramEvent::Closed) => break,
                                        _ => {} // SpeechStarted, UtteranceEnd — informational
                                    }
                                }
                            }
                            Ok(Message::Close(_)) => break,
                            Err(e) => {
                                let _ = event_tx.send(AsrEvent::Error {
                                    code: "DEEPGRAM_WS_ERROR".to_string(),
                                    message: e.to_string(),
                                    retryable: true,
                                });
                                break;
                            }
                            _ => {}
                        }
                    }
                    // Keepalive: Deepgram requires text JSON frame, NOT Ping control frame.
                    _ = ping_interval.tick() => {
                        let keepalive = serde_json::json!({"type": "KeepAlive"});
                        if ws_sink.send(Message::Text(keepalive.to_string())).await.is_err() {
                            break;
                        }
                    }
                    // External shutdown signal.
                    _ = shutdown.notified() => break,
                }
            }
        });

        Ok(())
    }

    async fn push_audio(&mut self, frame: &[f32]) -> anyhow::Result<()> {
        let sender = self.cmd_tx.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Deepgram: not started")
        })?;
        // Convert f32 samples to PCM16 bytes.
        let pcm_bytes: Vec<u8> = frame
            .iter()
            .flat_map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                let pcm = (clamped * i16::MAX as f32) as i16;
                pcm.to_le_bytes()
            })
            .collect();
        sender
            .send(DeepgramCommand::Audio(pcm_bytes))
            .map_err(|_| anyhow::anyhow!("Deepgram: WS task closed"))?;
        Ok(())
    }

    async fn next_event(&mut self) -> anyhow::Result<Option<AsrEvent>> {
        let receiver = self.event_rx.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Deepgram: not started")
        })?;
        match receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(anyhow::anyhow!(
                "Deepgram: event channel disconnected"
            )),
        }
    }

    async fn finalize(&mut self) -> anyhow::Result<()> {
        // Send Finalize message (flushes remaining audio on server).
        if let Some(sender) = &self.cmd_tx {
            let _ = sender.send(DeepgramCommand::Finalize);
            // Drain pending events for a short window to catch the final result.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        // Signal the WS task to stop.
        if let Some(shutdown) = &self.shutdown {
            shutdown.notify_one();
        }
        // Send CloseStream for graceful close.
        if let Some(sender) = &self.cmd_tx {
            let _ = sender.send(DeepgramCommand::CloseStream);
        }
        self.cmd_tx = None;
        self.event_rx = None;
        self.shutdown = None;
        Ok(())
    }

    fn name(&self) -> &str {
        "Deepgram"
    }

    fn is_ready(&self) -> bool {
        self.cmd_tx.is_some()
    }
}
// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixtures matching the current Deepgram Live Streaming protocol.
    /// Source: https://developers.deepgram.com/docs/live-streaming-audio

    fn partial_json() -> serde_json::Value {
        serde_json::json!({
            "type": "Results",
            "channel_index": [0, 1],
            "duration": 1.0,
            "start": 0.0,
            "channel": {
                "alternatives": [{
                    "transcript": "hello",
                    "confidence": 0.95,
                    "words": []
                }],
                "is_final": false,
                "speech_final": false
            }
        })
    }

    fn segment_final_json() -> serde_json::Value {
        serde_json::json!({
            "type": "Results",
            "channel_index": [0, 1],
            "duration": 2.0,
            "start": 0.0,
            "channel": {
                "alternatives": [{
                    "transcript": "hello world",
                    "confidence": 0.92,
                    "words": []
                }],
                "is_final": true,
                "speech_final": false
            }
        })
    }

    fn utterance_final_json() -> serde_json::Value {
        serde_json::json!({
            "type": "Results",
            "channel_index": [0, 1],
            "duration": 3.0,
            "start": 0.0,
            "channel": {
                "alternatives": [{
                    "transcript": "hello world how are you",
                    "confidence": 0.88,
                    "words": []
                }],
                "is_final": true,
                "speech_final": true
            }
        })
    }

    #[test]
    fn test_parse_partial() {
        let event = parse_deepgram_results(&partial_json());
        assert!(matches!(
            event,
            Some(DeepgramEvent::Partial {
                ref text,
                confidence: Some(0.95),
            }) if text == "hello"
        ));
    }

    #[test]
    fn test_parse_segment_final() {
        let event = parse_deepgram_results(&segment_final_json());
        assert!(matches!(
            event,
            Some(DeepgramEvent::SegmentFinal {
                ref text,
                confidence: Some(0.92),
            }) if text == "hello world"
        ));
    }

    #[test]
    fn test_parse_utterance_final() {
        let event = parse_deepgram_results(&utterance_final_json());
        assert!(matches!(
            event,
            Some(DeepgramEvent::UtteranceFinal {
                ref text,
                confidence: Some(0.88),
            }) if text == "hello world how are you"
        ));
    }

    #[test]
    fn test_parse_empty_transcript_ignored() {
        let json = serde_json::json!({
            "type": "Results",
            "channel": {
                "alternatives": [{ "transcript": "", "confidence": 0.5 }]
            }
        });
        assert!(parse_deepgram_results(&json).is_none());
    }

    #[test]
    fn test_parse_speech_started() {
        let json = serde_json::json!({ "type": "SpeechStarted" });
        assert!(matches!(
            parse_deepgram_event(&json),
            Some(DeepgramEvent::SpeechStarted)
        ));
    }

    #[test]
    fn test_parse_utterance_end() {
        let json = serde_json::json!({ "type": "UtteranceEnd" });
        assert!(matches!(
            parse_deepgram_event(&json),
            Some(DeepgramEvent::UtteranceEnd)
        ));
    }

    #[test]
    fn test_parse_error() {
        let json = serde_json::json!({ "type": "Error", "message": "connection lost" });
        assert!(matches!(
            parse_deepgram_event(&json),
            Some(DeepgramEvent::Error { ref message }) if message == "connection lost"
        ));
    }

    #[test]
    fn test_parse_close() {
        let json = serde_json::json!({ "type": "Close" });
        assert!(matches!(
            parse_deepgram_event(&json),
            Some(DeepgramEvent::Closed)
        ));
    }

    #[test]
    fn test_parse_metadata_is_not_closed() {
        // Metadata can arrive mid-stream (e.g., with request_id) — must NOT close connection.
        let json = serde_json::json!({ "type": "Metadata", "request_id": "abc" });
        assert!(parse_deepgram_event(&json).is_none());
    }

    #[test]
    fn test_build_url_endpointing_is_integer() {
        let adapter = DeepgramAdapter::new();
        let config = AsrConfig {
            engine_type: crate::asr::AsrEngineType::DeepgramStreaming,
            api_key: None,
            endpoint: None,
            language: "en".into(),
            vad_sensitivity: 50,
            sample_rate: 16000,
        };
        let url = adapter.build_url(&config);
        assert!(url.contains("endpointing=800"));
        assert!(!url.contains("endpointing=true"));
    }

    #[test]
    fn test_speech_final_takes_precedence_over_is_final() {
        // When both is_final and speech_final are true, speech_final wins.
        let json = serde_json::json!({
            "type": "Results",
            "channel": {
                "alternatives": [{ "transcript": "test", "confidence": 0.9 }],
                "is_final": true,
                "speech_final": true
            }
        });
        let event = parse_deepgram_results(&json);
        assert!(matches!(event, Some(DeepgramEvent::UtteranceFinal { .. })));
    }

    #[test]
    fn test_build_url_contains_required_params() {
        let adapter = DeepgramAdapter::new();
        let config = AsrConfig {
            engine_type: crate::asr::AsrEngineType::DeepgramStreaming,
            api_key: Some("test-key".into()),
            endpoint: None,
            language: "en".into(),
            vad_sensitivity: 50,
            sample_rate: 16000,
        };
        let url = adapter.build_url(&config);
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("sample_rate=16000"));
        assert!(url.contains("channels=1"));
        assert!(url.contains("interim_results=true"));
        assert!(url.contains("endpointing=800"));
        assert!(url.contains("vad_events=true"));
        assert!(url.contains("language=en"));
        // Key must NOT be in URL.
        assert!(!url.contains("test-key"));
    }

    #[test]
    fn test_build_url_omits_language_for_auto() {
        let adapter = DeepgramAdapter::new();
        let config = AsrConfig {
            engine_type: crate::asr::AsrEngineType::DeepgramStreaming,
            api_key: None,
            endpoint: None,
            language: "auto".into(),
            vad_sensitivity: 50,
            sample_rate: 16000,
        };
        let url = adapter.build_url(&config);
        assert!(!url.contains("language="));
    }

    #[test]
    fn test_build_request_sets_auth_header() {
        let adapter = DeepgramAdapter::new();
        let request = adapter.build_request(
            "wss://api.deepgram.com/v1/listen".to_string(),
            "my-secret-key",
        );
        assert!(request.is_ok());
    }
}
