//! OpenAI Realtime Transcription adapter.
//!
//! Implements the current OpenAI Realtime Transcription WebSocket protocol:
//! - URL: wss://api.openai.com/v1/realtime?model=gpt-4o-transcribe
//! - Auth: Authorization: Bearer <key> + OpenAI-Beta: realtime=v1 header
//! - Events: session.update, input_audio_buffer.append/commit, response.create
//! - Results: response.output_text.delta (partial), response.text.done (final)
//!
//! P0-A fix: completely rewritten from the old broken implementation that
//! treated the HTTP REST endpoint as a WebSocket and assumed {"type":"final"}.

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

use crate::asr::engine_trait::{AsrConfig, AsrEngineType};
use crate::asr::{AsrEvent, AsrSession};

/// Parsed OpenAI Realtime event.
#[derive(Debug, PartialEq)]
pub enum OpenAIEvent {
    Partial { text: String },
    Final { text: String },
    SessionCreated,
    Error { message: String },
    ResponseDone,
    Closed,
}

/// Parse an OpenAI Realtime WebSocket JSON message.
pub fn parse_openai_event(json: &serde_json::Value) -> Option<OpenAIEvent> {
    let event_type = json["type"].as_str()?;

    match event_type {
        // Session lifecycle
        "session.created" | "session.updated" => Some(OpenAIEvent::SessionCreated),

        // Transcription partial result (delta)
        "response.output_text.delta" => {
            let text = json["delta"].as_str().unwrap_or("").to_string();
            if text.is_empty() {
                None
            } else {
                Some(OpenAIEvent::Partial { text })
            }
        }

        // Transcription final result
        "response.text.done" => {
            let text = json["text"].as_str().unwrap_or("").to_string();
            if text.is_empty() {
                None
            } else {
                Some(OpenAIEvent::Final { text })
            }
        }

        // Response complete — utterance ended
        "response.done" => Some(OpenAIEvent::ResponseDone),

        // Error
        "error" => {
            let message = json["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            Some(OpenAIEvent::Error { message })
        }

        // Close
        "close" => Some(OpenAIEvent::Closed),

        // Other events (input_audio_buffer.committed, conversation.item.created, etc.)
        _ => None,
    }
}

// ── OpenAI adapter ─────────────────────────────────────────────────────

/// OpenAI Realtime Transcription adapter implementing AsrSession.
pub struct OpenaiRealtimeAdapter {
    config: Option<AsrConfig>,
    cmd_tx: Option<mpsc::UnboundedSender<OpenaiCommand>>,
    event_rx: Option<mpsc::UnboundedReceiver<AsrEvent>>,
    shutdown: Option<Arc<Notify>>,
}

enum OpenaiCommand {
    /// Send base64-encoded audio.
    Audio(String),
    /// Commit the audio buffer and request a response.
    CommitAndRespond,
    /// Close the connection.
    Close,
}

impl OpenaiRealtimeAdapter {
    pub fn new() -> Self {
        Self {
            config: None,
            cmd_tx: None,
            event_rx: None,
            shutdown: None,
        }
    }

    /// Build the OpenAI Realtime WebSocket URL.
    fn build_url(&self, config: &AsrConfig) -> String {
        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| "wss://api.openai.com/v1/realtime".to_string());
        // Model is required for realtime. Default to gpt-4o-transcribe.
        format!("{}?model=gpt-4o-transcribe", endpoint)
    }

    /// Build the WebSocket request with auth headers.
    fn build_request(
        &self,
        url: String,
        api_key: &str,
    ) -> anyhow::Result<impl IntoClientRequest> {
        let mut request = url.into_client_request().map_err(|e| {
            anyhow::anyhow!("Failed to build WS request: {}", e)
        })?;
        let headers = request.headers_mut();
        headers.insert(
            "Authorization",
            format!("Bearer {}", api_key)
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid auth header"))?,
        );
        // OpenAI Realtime requires this beta header.
        headers.insert(
            "OpenAI-Beta",
            "realtime=v1".parse().unwrap(),
        );
        Ok(request)
    }

    /// Build the session.update event with transcription configuration.
    fn build_session_update(&self, config: &AsrConfig) -> serde_json::Value {
        let lang = if config.language == "auto" {
            None
        } else {
            Some(config.language.clone())
        };

        serde_json::json!({
            "type": "session.update",
            "session": {
                "type": "realtime",
                "input_audio_format": "pcm16",
                "output_audio_format": "pcm16",
                "input_audio_transcription": {
                    "model": "gpt-4o-transcribe",
                    "language": lang
                },
                "turn_detection": {
                    "type": "server_vad",
                    "threshold": 0.5,
                    "prefix_padding_ms": 300,
                    "silence_duration_ms": 500
                },
                "instructions": "You are a helpful assistant that transcribes speech accurately. Provide exact transcription without adding commentary or formatting."
            }
        })
    }
}

#[async_trait]
impl AsrSession for OpenaiRealtimeAdapter {
    async fn start(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        self.config = Some(config.clone());
        let api_key = config.api_key.clone().unwrap_or_default();
        let url = self.build_url(&config);
        let request = self.build_request(url, &api_key)?;

        let (ws_stream, _) = connect_async(request).await.map_err(|e| {
            anyhow::anyhow!("OpenAI WS connection failed: {}", e)
        })?;

        let (mut ws_sink, mut ws_stream) = ws_stream.split();

        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<OpenaiCommand>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<AsrEvent>();

        self.cmd_tx = Some(cmd_tx);
        self.event_rx = Some(event_rx);

        let shutdown = Arc::new(Notify::new());
        self.shutdown = Some(shutdown.clone());

        // Send session configuration immediately after connect.
        let session_update = self.build_session_update(&config);
        let _ = ws_sink.send(Message::Text(session_update.to_string())).await;

        // Spawn background WS task.
        tokio::spawn(async move {
            let mut ping_interval =
                tokio::time::interval(std::time::Duration::from_secs(30));

            loop {
                tokio::select! {
                    // Commands from bridge task.
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(OpenaiCommand::Audio(b64)) => {
                                let msg = serde_json::json!({
                                    "type": "input_audio_buffer.append",
                                    "audio": b64
                                });
                                if ws_sink.send(Message::Text(msg.to_string())).await.is_err() {
                                    break;
                                }
                            }
                            Some(OpenaiCommand::CommitAndRespond) => {
                                // Commit the buffer.
                                let commit = serde_json::json!({
                                    "type": "input_audio_buffer.commit"
                                });
                                if ws_sink.send(Message::Text(commit.to_string())).await.is_err() {
                                    break;
                                }
                                // Request a response.
                                let response = serde_json::json!({
                                    "type": "response.create",
                                    "response": {
                                        "modalities": ["text"]
                                    }
                                });
                                if ws_sink.send(Message::Text(response.to_string())).await.is_err() {
                                    break;
                                }
                            }
                            Some(OpenaiCommand::Close) => {
                                break;
                            }
                            None => break,
                        }
                    }
                    // Messages from OpenAI.
                    Some(msg) = ws_stream.next() => {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    match parse_openai_event(&json) {
                                        Some(OpenAIEvent::Partial { text }) => {
                                            let _ = event_tx.send(AsrEvent::Partial {
                                                text,
                                                language: None,
                                            });
                                        }
                                        Some(OpenAIEvent::Final { text }) => {
                                            let _ = event_tx.send(AsrEvent::UtteranceFinal {
                                                text,
                                                language: None,
                                                confidence: None,
                                            });
                                        }
                                        Some(OpenAIEvent::Error { message }) => {
                                            let _ = event_tx.send(AsrEvent::Error {
                                                code: "OPENAI_ERROR".to_string(),
                                                message,
                                                retryable: true,
                                            });
                                        }
                                        Some(OpenAIEvent::ResponseDone) => break,
                                        Some(OpenAIEvent::Closed) => break,
                                        _ => {}
                                    }
                                }
                            }
                            Ok(Message::Close(_)) => break,
                            Err(e) => {
                                let _ = event_tx.send(AsrEvent::Error {
                                    code: "OPENAI_WS_ERROR".to_string(),
                                    message: e.to_string(),
                                    retryable: true,
                                });
                                break;
                            }
                            _ => {}
                        }
                    }
                    // Keepalive ping.
                    _ = ping_interval.tick() => {
                        if ws_sink.send(Message::Ping(vec![])).await.is_err() {
                            break;
                        }
                    }
                    // External shutdown.
                    _ = shutdown.notified() => break,
                }
            }
        });

        Ok(())
    }

    async fn push_audio(&mut self, frame: &[f32]) -> anyhow::Result<()> {
        let sender = self.cmd_tx.as_ref().ok_or_else(|| {
            anyhow::anyhow!("OpenAI: not started")
        })?;
        // Convert f32 samples to PCM16, then base64 encode.
        let pcm_bytes: Vec<u8> = frame
            .iter()
            .flat_map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                let pcm = (clamped * i16::MAX as f32) as i16;
                pcm.to_le_bytes()
            })
            .collect();
        let b64 = base64_encode(&pcm_bytes);
        sender
            .send(OpenaiCommand::Audio(b64))
            .map_err(|_| anyhow::anyhow!("OpenAI: WS task closed"))?;
        Ok(())
    }

    async fn next_event(&mut self) -> anyhow::Result<Option<AsrEvent>> {
        let receiver = self.event_rx.as_mut().ok_or_else(|| {
            anyhow::anyhow!("OpenAI: not started")
        })?;
        match receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(anyhow::anyhow!(
                "OpenAI: event channel disconnected"
            )),
        }
    }

    async fn finalize(&mut self) -> anyhow::Result<()> {
        // Commit audio buffer and request final response.
        if let Some(sender) = &self.cmd_tx {
            let _ = sender.send(OpenaiCommand::CommitAndRespond);
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        if let Some(shutdown) = &self.shutdown {
            shutdown.notify_one();
        }
        if let Some(sender) = &self.cmd_tx {
            let _ = sender.send(OpenaiCommand::Close);
        }
        self.cmd_tx = None;
        self.event_rx = None;
        self.shutdown = None;
        Ok(())
    }

    fn name(&self) -> &str {
        "OpenAI Realtime"
    }

    fn is_ready(&self) -> bool {
        self.cmd_tx.is_some()
    }
}

/// Simple base64 encoding.
fn base64_encode(input: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixtures matching the current OpenAI Realtime protocol.

    fn session_created_json() -> serde_json::Value {
        serde_json::json!({
            "type": "session.created",
            "session": {
                "id": "sess_abc123",
                "model": "gpt-4o-transcribe"
            }
        })
    }

    fn partial_delta_json() -> serde_json::Value {
        serde_json::json!({
            "type": "response.output_text.delta",
            "delta": "hello"
        })
    }

    fn final_text_json() -> serde_json::Value {
        serde_json::json!({
            "type": "response.text.done",
            "text": "hello world"
        })
    }

    fn response_done_json() -> serde_json::Value {
        serde_json::json!({
            "type": "response.done",
            "response": { "id": "resp_123" }
        })
    }

    fn error_json() -> serde_json::Value {
        serde_json::json!({
            "type": "error",
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error"
            }
        })
    }

    #[test]
    fn test_parse_session_created() {
        let event = parse_openai_event(&session_created_json());
        assert!(matches!(event, Some(OpenAIEvent::SessionCreated)));
    }

    #[test]
    fn test_parse_partial_delta() {
        let event = parse_openai_event(&partial_delta_json());
        assert!(matches!(
            event,
            Some(OpenAIEvent::Partial { ref text }) if text == "hello"
        ));
    }

    #[test]
    fn test_parse_final_text() {
        let event = parse_openai_event(&final_text_json());
        assert!(matches!(
            event,
            Some(OpenAIEvent::Final { ref text }) if text == "hello world"
        ));
    }

    #[test]
    fn test_parse_response_done() {
        let event = parse_openai_event(&response_done_json());
        assert!(matches!(event, Some(OpenAIEvent::ResponseDone)));
    }

    #[test]
    fn test_parse_error() {
        let event = parse_openai_event(&error_json());
        assert!(matches!(
            event,
            Some(OpenAIEvent::Error { ref message }) if message == "Invalid API key"
        ));
    }

    #[test]
    fn test_parse_empty_delta_ignored() {
        let json = serde_json::json!({ "type": "response.output_text.delta", "delta": "" });
        assert!(parse_openai_event(&json).is_none());
    }

    #[test]
    fn test_build_url_contains_model() {
        let adapter = OpenaiRealtimeAdapter::new();
        let config = AsrConfig {
            engine_type: crate::asr::AsrEngineType::OpenAIRealtimeTranscription,
            api_key: Some("test-key".into()),
            endpoint: None,
            language: "en".into(),
            vad_sensitivity: 50,
            sample_rate: 16000,
        };
        let url = adapter.build_url(&config);
        assert!(url.contains("model=gpt-4o-transcribe"));
        // Key must NOT be in URL.
        assert!(!url.contains("test-key"));
    }

    #[test]
    fn test_build_request_sets_auth_header() {
        let adapter = OpenaiRealtimeAdapter::new();
        let request = adapter.build_request(
            "wss://api.openai.com/v1/realtime".to_string(),
            "my-secret-key",
        );
        assert!(request.is_ok());
    }

    #[test]
    fn test_build_session_update_contains_required_fields() {
        let adapter = OpenaiRealtimeAdapter::new();
        let config = AsrConfig {
            engine_type: crate::asr::AsrEngineType::OpenAIRealtimeTranscription,
            api_key: None,
            endpoint: None,
            language: "en".into(),
            vad_sensitivity: 50,
            sample_rate: 16000,
        };
        let update = adapter.build_session_update(&config);
        assert_eq!(update["type"], "session.update");
        assert_eq!(update["session"]["input_audio_format"], "pcm16");
        assert_eq!(update["session"]["input_audio_transcription"]["model"], "gpt-4o-transcribe");
        assert_eq!(update["session"]["turn_detection"]["type"], "server_vad");
    }

    #[test]
    fn test_base64_encode() {
        let encoded = base64_encode(&[0x68, 0x65, 0x6c, 0x6c, 0x6f]); // "hello"
        assert_eq!(encoded, "aGVsbG8=");
    }
}
