// ASR streaming manager — manages the lifecycle of an ASR engine connection
// Handles audio forwarding, result routing, and reconnection logic

use super::engine_trait::{AsrConfig, AsrEngineType, AsrResult, StreamingAsrEngine};
use super::adapters::{openai_whisper::OpenaiWhisperAdapter, deepgram::DeepgramAdapter};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Manages the active ASR streaming session
#[derive(Clone)]
pub struct AsrManager {
    engine: Option<Arc<Mutex<Box<dyn StreamingAsrEngine>>>>,
    config: Option<AsrConfig>,
}

impl AsrManager {
    pub fn new() -> Self {
        Self {
            engine: None,
            config: None,
        }
    }

    /// Initialize the ASR engine with the given configuration
    pub async fn initialize(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        let engine: Box<dyn StreamingAsrEngine> = match config.engine_type {
            AsrEngineType::OpenaiWhisper => Box::new(OpenaiWhisperAdapter::new()),
            AsrEngineType::Deepgram => Box::new(DeepgramAdapter::new()),
            AsrEngineType::WhisperCpp => {
                // TODO: Implement local Whisper.cpp adapter
                anyhow::bail!("Whisper.cpp adapter not yet implemented (V1.5 feature)")
            }
            AsrEngineType::Funasr => {
                // TODO: Implement FunASR adapter
                anyhow::bail!("FunASR adapter not yet implemented (V1.5 feature)")
            }
        };

        let mut engine = engine;
        engine.initialize(config.clone()).await?;
        self.engine = Some(Arc::new(Mutex::new(engine)));
        self.config = Some(config);
        Ok(())
    }

    /// Send audio data to the active engine
    pub async fn send_audio(&self, audio_data: &[f32]) -> anyhow::Result<()> {
        if let Some(engine) = &self.engine {
            let mut eng = engine.lock().await;
            eng.send_audio(audio_data).await?;
        }
        Ok(())
    }

    /// Try to receive a recognition result (non-blocking)
    pub async fn receive_result(&self) -> anyhow::Result<Option<AsrResult>> {
        if let Some(engine) = &self.engine {
            let mut eng = engine.lock().await;
            return eng.receive_result().await;
        }
        Ok(None)
    }

    /// Flush the engine and get final result
    pub async fn flush(&self) -> anyhow::Result<Option<AsrResult>> {
        if let Some(engine) = &self.engine {
            let mut eng = engine.lock().await;
            return eng.flush().await;
        }
        Ok(None)
    }

    /// Close the current engine
    pub async fn close(&mut self) -> anyhow::Result<()> {
        if let Some(engine) = self.engine.take() {
            let mut eng = engine.lock().await;
            eng.close().await?;
        }
        Ok(())
    }

    /// Check if engine is active
    pub fn is_active(&self) -> bool {
        self.engine.is_some()
    }
}
