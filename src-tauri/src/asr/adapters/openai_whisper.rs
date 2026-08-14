// OpenAI Whisper API adapter — uses WebSocket for streaming recognition
// Connects to OpenAI's realtime speech-to-text endpoint

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;

use super::super::engine_trait::{AsrConfig, AsrResult, StreamingAsrEngine};

/// OpenAI Whisper streaming adapter
pub struct OpenaiWhisperAdapter {
    config: Option<AsrConfig>,
    ws_sender: Option<mpsc::UnboundedSender<Vec<f32>>>,
    ws_receiver: Option<mpsc::UnboundedReceiver<AsrResult>>,
    ready: bool,
}

impl OpenaiWhisperAdapter {
    pub fn new() -> Self {
        Self {
            config: None,
            ws_sender: None,
            ws_receiver: None,
            ready: false,
        }
    }
}

#[async_trait]
impl StreamingAsrEngine for OpenaiWhisperAdapter {
    async fn initialize(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        self.config = Some(config.clone());

        let (tx_audio, mut rx_audio) = mpsc::unbounded_channel::<Vec<f32>>();
        let (tx_result, rx_result) = mpsc::unbounded_channel::<AsrResult>();

        self.ws_sender = Some(tx_audio);
        self.ws_receiver = Some(rx_result);

        // Extract API key for authentication (C4 fix)
        let api_key = config.api_key.clone().unwrap_or_default();
        let endpoint = config.endpoint.clone().unwrap_or_else(|| {
            "wss://api.openai.com/v1/audio/transcriptions".to_string()
        });
        let language = config.language.clone();

        tokio::spawn(async move {
            // Build request with Authorization header for OpenAI authentication
            let request = match http::Request::builder()
                .uri(endpoint.clone())
                .header("Authorization", format!("Bearer {}", api_key))
                .header("OpenAI-Beta", "realtime-v1")
                .body(())
            {
                Ok(req) => req,
                Err(e) => {
                    log::error!("Failed to build WS request: {}", e);
                    return;
                }
            };

            match connect_async(request).await {
                Ok((mut ws_stream, _)) => {
                    log::info!("Connected to OpenAI Whisper WebSocket");

                    // M8 fix: Keepalive ping interval to prevent NAT timeout
                    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

                    loop {
                        tokio::select! {
                            audio = rx_audio.recv() => {
                                // Sender dropped (close()) closes the channel; stop
                                // the task so the WebSocket is torn down cleanly.
                                let Some(audio) = audio else { break };
                                // Convert f32 samples to PCM16 bytes
                                let pcm_bytes: Vec<u8> = audio.iter()
                                    .flat_map(|&s| {
                                        let clamped = s.clamp(-1.0, 1.0);
                                        let pcm = (clamped * i16::MAX as f32) as i16;
                                        pcm.to_le_bytes()
                                    })
                                    .collect();
                                if ws_stream.send(tokio_tungstenite::tungstenite::Message::Binary(pcm_bytes)).await.is_err() {
                                    break;
                                }
                            }
                            _ = ping_interval.tick() => {
                                // M8 fix: Send ping to keep connection alive
                                if ws_stream.send(tokio_tungstenite::tungstenite::Message::Ping(vec![])).await.is_err() {
                                    break;
                                }
                            }
                            Some(msg) = ws_stream.next() => {
                                match msg {
                                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                        // Parse JSON response
                                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                            let is_final = json["type"] == "final";
                                            let text_content = json["text"]
                                                .as_str()
                                                .unwrap_or("")
                                                .to_string();
                                            let _ = tx_result.send(AsrResult {
                                                text: text_content,
                                                is_final,
                                                language: Some(language.clone()),
                                                confidence: json["confidence"].as_f64(),
                                            });
                                        }
                                    }
                                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => break,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to connect to OpenAI Whisper: {}", e);
                }
            }
        });

        self.ready = true;
        Ok(())
    }

    async fn send_audio(&mut self, audio_data: &[f32]) -> anyhow::Result<()> {
        if let Some(sender) = &self.ws_sender {
            let data = audio_data.to_vec();
            sender.send(data).map_err(|_| anyhow::anyhow!("WS channel closed"))?;
        }
        Ok(())
    }

    async fn receive_result(&mut self) -> anyhow::Result<Option<AsrResult>> {
        if let Some(receiver) = &mut self.ws_receiver {
            match receiver.try_recv() {
                Ok(result) => Ok(Some(result)),
                Err(mpsc::error::TryRecvError::Empty) => Ok(None),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    Err(anyhow::anyhow!("WS result channel disconnected"))
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn flush(&mut self) -> anyhow::Result<Option<AsrResult>> {
        if let Some(sender) = &self.ws_sender {
            sender.send(Vec::new()).ok();
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        self.receive_result().await
    }

    async fn close(&mut self) -> anyhow::Result<()> {
        self.ws_sender = None;
        self.ws_receiver = None;
        self.ready = false;
        Ok(())
    }

    fn name(&self) -> &str {
        "OpenAI Whisper"
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}
