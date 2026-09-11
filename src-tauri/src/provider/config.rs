// Provider configuration types (spec section 10, 24).
//
// Each cloud provider is identified by a stable ProviderId. ProviderConfig
// holds the user's chosen endpoint, model, and a credential_ref that points
// into OS secure storage — the actual API key NEVER lives in SQLite.

use serde::{Deserialize, Serialize};

/// Stable identifiers for all supported ASR providers (spec section 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    /// Local SenseVoice (offline, sherpa-onnx).
    LocalSenseVoice,
    /// OpenAI Realtime Transcription (WebSocket).
    OpenAIRealtime,
    /// Deepgram Streaming (WebSocket).
    DeepgramStreaming,
    /// OpenAI Whisper (REST, via WebSocket wrapper).
    OpenAIWhisper,
    /// Custom OpenAI-compatible endpoint.
    CustomOpenAICompatible,
}

impl ProviderId {
    /// Whether this provider runs locally (no network / API key).
    pub fn is_local(&self) -> bool {
        matches!(self, ProviderId::LocalSenseVoice)
    }

    /// Whether this provider requires an API key.
    pub fn requires_credential(&self) -> bool {
        !self.is_local()
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::LocalSenseVoice => "SenseVoice (本地)",
            ProviderId::OpenAIRealtime => "OpenAI Realtime",
            ProviderId::DeepgramStreaming => "Deepgram Streaming",
            ProviderId::OpenAIWhisper => "OpenAI Whisper",
            ProviderId::CustomOpenAICompatible => "自定义 (OpenAI 兼容)",
        }
    }

    /// Default WebSocket/REST endpoint for this provider.
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ProviderId::LocalSenseVoice => "",
            ProviderId::OpenAIRealtime => "wss://api.openai.com/v1/realtime",
            ProviderId::DeepgramStreaming => "wss://api.deepgram.com/v1/listen",
            ProviderId::OpenAIWhisper => "https://api.openai.com/v1/audio/transcriptions",
            ProviderId::CustomOpenAICompatible => "",
        }
    }

    /// Default model name for this provider.
    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderId::LocalSenseVoice => "sensevoice",
            ProviderId::OpenAIRealtime => "gpt-4o-transcribe",
            ProviderId::DeepgramStreaming => "nova-3",
            ProviderId::OpenAIWhisper => "whisper-1",
            ProviderId::CustomOpenAICompatible => "",
        }
    }

    /// All cloud (non-local) providers, for registry iteration.
    pub fn cloud_providers() -> &'static [ProviderId] {
        &[
            ProviderId::OpenAIRealtime,
            ProviderId::DeepgramStreaming,
            ProviderId::OpenAIWhisper,
            ProviderId::CustomOpenAICompatible,
        ]
    }
}

impl std::str::FromStr for ProviderId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local_sensevoice" => Ok(ProviderId::LocalSenseVoice),
            "openai_realtime" => Ok(ProviderId::OpenAIRealtime),
            "deepgram_streaming" => Ok(ProviderId::DeepgramStreaming),
            "openai_whisper" => Ok(ProviderId::OpenAIWhisper),
            "custom_openai_compatible" => Ok(ProviderId::CustomOpenAICompatible),
            // Legacy aliases for backward compatibility.
            "openai_whisper_legacy" => Ok(ProviderId::OpenAIRealtime),
            "deepgram" => Ok(ProviderId::DeepgramStreaming),
            other => Err(format!("Unknown provider: {other}")),
        }
    }
}

/// The user's high-level voice engine mode (spec section 39).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    /// Automatically prefer local; fall back to cloud when local unavailable.
    Automatic,
    /// Force offline local engine only.
    Offline,
    /// Force a specific cloud provider.
    Cloud,
}

/// Per-persistent provider configuration (spec section 24).
///
/// Stored in SQLite `provider_config`. The credential_ref points to an entry
/// in OS secure storage — the actual secret is NOT stored here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: ProviderId,
    /// User-overridden endpoint (empty = use provider default).
    pub endpoint: String,
    /// Model name (empty = use provider default).
    pub model: String,
    /// Reference to the credential in OS secure storage, e.g. "openai-default".
    pub credential_ref: String,
    /// Whether this provider is enabled in the UI.
    pub enabled: bool,
}

impl ProviderConfig {
    /// Create a config with provider-specific defaults.
    pub fn new(provider: ProviderId) -> Self {
        Self {
            provider,
            endpoint: provider.default_endpoint().to_string(),
            model: provider.default_model().to_string(),
            credential_ref: format!("{}-default", provider_key_name(&provider)),
            enabled: !provider.is_local(),
        }
    }

    /// Resolve the effective endpoint (user override or default).
    pub fn effective_endpoint(&self) -> String {
        if self.endpoint.is_empty() {
            self.provider.default_endpoint().to_string()
        } else {
            self.endpoint.clone()
        }
    }

    /// Resolve the effective model (user override or default).
    pub fn effective_model(&self) -> String {
        if self.model.is_empty() {
            self.provider.default_model().to_string()
        } else {
            self.model.clone()
        }
    }
}

/// Map a provider to its stable credential-store account suffix.
pub(crate) fn provider_key_name(provider: &ProviderId) -> &'static str {
    match provider {
        ProviderId::LocalSenseVoice => "local",
        ProviderId::OpenAIRealtime => "openai",
        ProviderId::DeepgramStreaming => "deepgram",
        ProviderId::OpenAIWhisper => "openai-whisper",
        ProviderId::CustomOpenAICompatible => "custom",
    }
}

/// Build the SQLite key for a provider config field.
pub fn config_key(provider: ProviderId, field: &str) -> String {
    format!("provider.{}.{}", provider_key_name(&provider), field)
}
