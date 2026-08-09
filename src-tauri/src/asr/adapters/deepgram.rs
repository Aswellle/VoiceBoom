// Deepgram streaming ASR adapter
// Uses Deepgram's real-time WebSocket API for low-latency speech recognition

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;

use super::super::engine_trait::{AsrConfig, AsrResult, StreamingAsrEngine};

/// Deepgram streaming adapter
pub struct DeepgramAdapter {
    config: Option<AsrConfig>,
    ws_sender: Option<mpsc::UnboundedSender<Vec<f32>>>,
    ws_receiver: Option<mpsc::UnboundedReceiver<AsrResult>>,
    ready: bool,
}

impl DeepgramAdapter {
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
impl StreamingAsrEngine for DeepgramAdapter {
    async fn initialize(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        self.config = Some(config.clone());

        let (tx_audio, mut rx_audio) = mpsc::unbounded_channel::<Vec<f32>>();
        let (tx_result, rx_result) = mpsc::unbounded_channel::<AsrResult>();

        self.ws_sender = Some(tx_audio);
        self.ws_receiver = Some(rx_result);

        let _api_key = config.api_key.clone().unwrap_or_default();
        let endpoint = config.endpoint.clone().unwrap_or_else(|| {
            "wss://api.deepgram.com/v1/listen".to_string()
        });
        let language = config.language.clone();
        let sample_rate = config.sample_rate;

        tokio::spawn(async move {
            // Build Deepgram WebSocket URL with query params
            let url_str = format!(
                "{}?encoding=linear16&sample_rate={}&channels=1&language={}",
                endpoint, sample_rate,
                if language == "auto" { "en" } else { &language }
            );

            match connect_async(url_str.clone()).await {
                Ok((mut ws_stream, _)) => {
                    log::info!("Connected to Deepgram WebSocket");

                    loop {
                        tokio::select! {
                            Some(audio) = rx_audio.recv() => {
                                if audio.is_empty() {
                                    // Empty audio = flush signal
                                    let _ = ws_stream.send(
                                        tokio_tungstenite::tungstenite::Message::Text(
                                            r#"{"type":"Flush"}"#.to_string()
                                        )
                                    ).await;
                                    continue;
                                }
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
                            Some(msg) = ws_stream.next() => {
                                match msg {
                                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                            if json["type"] == "Results" {
                                                let channel = &json["channel"];
                                                let alternatives = channel["alternatives"].as_array();
                                                if let Some(alt) = alternatives.and_then(|a| a.first()) {
                                                    let transcript = alt["transcript"]
                                                        .as_str()
                                                        .unwrap_or("")
                                                        .to_string();
                                                    let is_final = channel["is_final"].as_bool().unwrap_or(false);
                                                    if !transcript.is_empty() {
                                                        let _ = tx_result.send(AsrResult {
                                                            text: transcript,
                                                            is_final,
                                                            language: Some(language.clone()),
                                                            confidence: alt["confidence"].as_f64(),
                                                        });
                                                    }
                                                }
                                            }
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
                    log::error!("Failed to connect to Deepgram: {}", e);
                }
            }
        });

        self.ready = true;
        Ok(())
    }

    async fn send_audio(&mut self, audio_data: &[f32]) -> anyhow::Result<()> {
        if let Some(sender) = &self.ws_sender {
            sender.send(audio_data.to_vec())
                .map_err(|_| anyhow::anyhow!("Deepgram WS channel closed"))?;
        }
        Ok(())
    }

    async fn receive_result(&mut self) -> anyhow::Result<Option<AsrResult>> {
        if let Some(receiver) = &mut self.ws_receiver {
            match receiver.try_recv() {
                Ok(result) => Ok(Some(result)),
                Err(mpsc::error::TryRecvError::Empty) => Ok(None),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    Err(anyhow::anyhow!("Deepgram result channel disconnected"))
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
        if let Some(sender) = &self.ws_sender {
            sender.send(Vec::new()).ok();
        }
        self.ws_sender = None;
        self.ws_receiver = None;
        self.ready = false;
        Ok(())
    }

    fn name(&self) -> &str {
        "Deepgram"
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}
