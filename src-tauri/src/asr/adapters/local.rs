// Local ASR adapter — connects to locally-running Whisper.cpp / FunASR servers
// These servers are bundled with the app and started via ServerManager

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;

use super::super::engine_trait::{AsrConfig, AsrResult, StreamingAsrEngine};

/// Local ASR adapter (Whisper.cpp or FunASR WebSocket server)
pub struct LocalAsrAdapter {
    config: Option<AsrConfig>,
    ws_sender: Option<mpsc::UnboundedSender<Vec<f32>>>,
    ws_receiver: Option<mpsc::UnboundedReceiver<AsrResult>>,
    ready: bool,
}

impl LocalAsrAdapter {
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
impl StreamingAsrEngine for LocalAsrAdapter {
    async fn initialize(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        self.config = Some(config.clone());

        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| "ws://127.0.0.1:8080".to_string());

        // Ensure endpoint starts with ws:// or wss://
        if !endpoint.starts_with("ws://") && !endpoint.starts_with("wss://") {
            return Err(anyhow::anyhow!("Invalid WebSocket endpoint: {}", endpoint));
        }

        let (tx_audio, mut rx_audio) = mpsc::unbounded_channel::<Vec<f32>>();
        let (tx_result, rx_result) = mpsc::unbounded_channel::<AsrResult>();

        self.ws_sender = Some(tx_audio);
        self.ws_receiver = Some(rx_result);

        let language = config.language.clone();

        tokio::spawn(async move {
            // Try to connect, retrying a few times to give the server time to start
            let mut ws_stream = None;
            for attempt in 0..5 {
                match connect_async(endpoint.clone()).await {
                    Ok((stream, _)) => {
                        ws_stream = Some(stream);
                        break;
                    }
                    Err(e) => {
                        if attempt == 4 {
                            log::error!("Failed to connect to local ASR server after 5 attempts: {}", e);
                            return;
                        }
                        log::info!("Waiting for local ASR server (attempt {})...", attempt + 1);
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                }
            }

            let mut ws_stream = match ws_stream {
                Some(s) => s,
                None => return,
            };

            log::info!("Connected to local ASR server at {}", endpoint);

            // Keepalive ping every 30s
            let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));

            loop {
                tokio::select! {
                    Some(audio) = rx_audio.recv() => {
                        if audio.is_empty() {
                            // Flush signal - send a JSON flush message
                            let _ = ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(
                                r#"{"action":"flush"}"#.to_string()
                            )).await;
                            continue;
                        }
                        // Send PCM16 audio data
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
                        if ws_stream.send(tokio_tungstenite::tungstenite::Message::Ping(vec![])).await.is_err() {
                            break;
                        }
                    }
                    Some(msg) = ws_stream.next() => {
                        match msg {
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                // Parse JSON response - the server returns transcription results
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    // Try to extract text from various response formats
                                    let text_content = json["text"].as_str()
                                        .or_else(|| json["transcript"].as_str())
                                        .or_else(|| json["result"].as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let is_final = json["is_final"].as_bool().unwrap_or(true);
                                    if !text_content.is_empty() {
                                        let _ = tx_result.send(AsrResult {
                                            text: text_content,
                                            is_final,
                                            language: Some(language.clone()),
                                            confidence: json["confidence"].as_f64(),
                                        });
                                    }
                                }
                            }
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => break,
                            _ => {}
                        }
                    }
                }
            }
        });

        self.ready = true;
        Ok(())
    }

    async fn send_audio(&mut self, audio_data: &[f32]) -> anyhow::Result<()> {
        if let Some(sender) = &self.ws_sender {
            sender.send(audio_data.to_vec())
                .map_err(|_| anyhow::anyhow!("Local ASR channel closed"))?;
        }
        Ok(())
    }

    async fn receive_result(&mut self) -> anyhow::Result<Option<AsrResult>> {
        if let Some(receiver) = &mut self.ws_receiver {
            match receiver.try_recv() {
                Ok(result) => Ok(Some(result)),
                Err(mpsc::error::TryRecvError::Empty) => Ok(None),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    Err(anyhow::anyhow!("Local ASR result channel disconnected"))
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
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        self.receive_result().await
    }

    async fn close(&mut self) -> anyhow::Result<()> {
        self.ws_sender = None;
        self.ws_receiver = None;
        self.ready = false;
        Ok(())
    }

    fn name(&self) -> &str {
        "Local ASR"
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}
