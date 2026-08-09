// ASR Engine trait — abstract interface for pluggable ASR backends
// New engines implement this trait (Whisper API, Deepgram, Whisper.cpp, FunASR, etc.)

use async_trait::async_trait;

/// Recognition result from ASR engine
#[derive(Debug, Clone, serde::Serialize)]
pub struct AsrResult {
    /// The recognized text
    pub text: String,
    /// Whether this is a final result (true) or partial/interim (false)
    pub is_final: bool,
    /// Detected language (ISO 639-1 code)
    pub language: Option<String>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: Option<f64>,
}

/// Configuration for ASR engine initialization
#[derive(Debug, Clone)]
pub struct AsrConfig {
    /// Engine type identifier
    pub engine_type: AsrEngineType,
    /// API key for cloud engines
    pub api_key: Option<String>,
    /// API endpoint URL
    pub endpoint: Option<String>,
    /// Language setting ("auto" for auto-detect)
    pub language: String,
    /// Sample rate of input audio
    pub sample_rate: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AsrEngineType {
    OpenaiWhisper,
    Deepgram,
    WhisperCpp,
    Funasr,
}

/// Core ASR engine trait — all engines must implement this
#[async_trait]
pub trait StreamingAsrEngine: Send + Sync {
    /// Initialize the engine with configuration
    async fn initialize(&mut self, config: AsrConfig) -> anyhow::Result<()>;

    /// Send audio chunk to the engine for streaming recognition
    async fn send_audio(&mut self, audio_data: &[f32]) -> anyhow::Result<()>;

    /// Receive the next recognition result (non-blocking)
    async fn receive_result(&mut self) -> anyhow::Result<Option<AsrResult>>;

    /// Signal end of audio stream, flush remaining results
    async fn flush(&mut self) -> anyhow::Result<Option<AsrResult>>;

    /// Close the engine and release resources
    async fn close(&mut self) -> anyhow::Result<()>;

    /// Get the engine name
    fn name(&self) -> &str;

    /// Check if the engine is ready
    fn is_ready(&self) -> bool;
}
