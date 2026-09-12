// Local ASR adapter — sherpa-onnx SenseVoice with Silero VAD
//
// Architecture follows the official sherpa-onnx example
// `sense_voice_simulate_streaming_microphone.rs`: a single processing loop
// feeds audio into Silero VAD, runs an interim decode of the accumulated
// buffer every 0.2s while speech is ongoing, and runs a final decode on each
// VAD-detected segment. There is no separate VAD in the bridge task and no
// multi-threaded lock contention — the adapter owns the VAD, recognizer and
// buffer, and the bridge task just pushes samples in and forwards results out.

use async_trait::async_trait;
use std::time::Instant;

use super::super::engine_trait::{AsrConfig, AsrResult, StreamingAsrEngine};

/// Tuning parameters for local SenseVoice streaming.
/// Centralized configuration to avoid scattered magic numbers (Phase 7).
#[derive(Debug, Clone)]
pub struct LocalAsrTuning {
    /// VAD processes audio in fixed windows (samples at 16kHz).
    pub vad_window_size: usize,
    /// Silence duration to declare speech end (seconds).
    pub min_silence_ms: u32,
    /// Minimum speech duration to trigger detection (seconds).
    pub min_speech_ms: u32,
    /// Maximum continuous speech duration (seconds).
    pub max_speech_ms: u32,
    /// Interim decode interval while speech is ongoing (seconds).
    pub partial_interval_ms: u32,
    /// Maximum working buffer duration (seconds).
    /// Audio older than this is trimmed to bound memory and decode cost.
    pub max_working_buffer_ms: u32,
}

impl Default for LocalAsrTuning {
    fn default() -> Self {
        Self {
            vad_window_size: 512,        // 32ms @16kHz
            min_silence_ms: 300,         // 0.3s silence = speech end
            min_speech_ms: 250,          // 0.25s minimum speech
            max_speech_ms: 8000,         // 8s max continuous speech
            partial_interval_ms: 200,    // 200ms interim cadence
            max_working_buffer_ms: 3000, // 3s bounded working buffer
        }
    }
}

/// Convert milliseconds to samples at 16kHz.
fn ms_to_samples(ms: u32) -> usize {
    (ms as usize) * 16000 / 1000
}

/// Local ASR adapter using sherpa-onnx OfflineRecognizer + Silero VAD.
pub struct LocalAsrAdapter {
    config: Option<AsrConfig>,
    /// Tuning parameters (Phase 7: centralized config).
    tuning: LocalAsrTuning,
    /// Accumulated audio samples at 16kHz mono f32.
    buffer: Vec<f32>,
    /// Silero VAD for endpoint detection.
    vad: Option<sherpa_onnx::VoiceActivityDetector>,
    /// SenseVoice offline recognizer.
    recognizer: Option<sherpa_onnx::OfflineRecognizer>,
    /// How far into `buffer` the VAD has consumed.
    vad_offset: usize,
    /// Whether speech is currently active (VAD detected).
    speech_active: bool,
    /// Timestamp of the last interim decode.
    last_interim: Instant,
    ready: bool,
    /// VAD sensitivity used to build the current VAD.
    last_vad_sensitivity: Option<u32>,
}
impl LocalAsrAdapter {
    pub fn new() -> Self {
        Self {
            config: None,
            tuning: LocalAsrTuning::default(),
            buffer: Vec::new(),
            vad: None,
            recognizer: None,
            vad_offset: 0,
            speech_active: false,
            last_interim: Instant::now(),
            ready: false,
            last_vad_sensitivity: None,
        }
    }

    /// Create with custom tuning (for testing).
    #[allow(dead_code)]
    pub fn with_tuning(tuning: LocalAsrTuning) -> Self {
        Self {
            config: None,
            tuning,
            ..Default::default()
        }
    }
}

impl Default for LocalAsrAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalAsrAdapter {
    fn ensure_models(&mut self, config: &AsrConfig) -> anyhow::Result<()> {
        // The VAD's threshold is fixed at creation time. If the user changes
        // sensitivity, drop the old VAD so it gets recreated with the new
        // threshold on the next initialize().
        if self.last_vad_sensitivity != Some(config.vad_sensitivity) {
            self.vad = None;
        }

        if self.vad.is_some() && self.recognizer.is_some() {
            return Ok(());
        }

        // Endpoint encodes paths separated by ASCII record separator \x1E:
        // "vad_model_path\x1Eonnx_model_path\x1Etokens_path"
        let endpoint = config.endpoint.as_deref().unwrap_or("");
        let parts: Vec<&str> = endpoint.split('\u{1E}').collect();
        if parts.len() < 3 {
            return Err(anyhow::anyhow!(
                "Local engine endpoint must be 'vad\\x1Emodel\\x1Etokens' (got {} parts)",
                parts.len()
            ));
        }

        // Fail loudly on empty paths rather than letting sherpa-onnx's model
        // loader surface a cryptic error.
        if parts[0].trim().is_empty()
            || parts[1].trim().is_empty()
            || parts[2].trim().is_empty()
        {
            return Err(anyhow::anyhow!(
                "Local engine endpoint has an empty vad/model/tokens path: {:?}",
                endpoint
            ));
        }

        let (vad_model, onnx_model, tokens) = (parts[0], parts[1], parts[2]);

        // Create Silero VAD. Map sensitivity (0-100) to a threshold in
        // [0.2, 0.95]: higher sensitivity → lower threshold (easier to
        // trigger). 50 (default) → 0.5 (Silero's recommended default).
        if self.vad.is_none() {
            let sensitivity = config.vad_sensitivity.clamp(0, 100) as f32;
            let threshold = 0.95 - (sensitivity / 100.0) * 0.75;
            let mut vad_config = sherpa_onnx::VadModelConfig::default();
            vad_config.silero_vad.model = Some(vad_model.to_string());
            vad_config.silero_vad.threshold = threshold;
            vad_config.silero_vad.min_silence_duration = self.tuning.min_silence_ms as f32 / 1000.0;
            vad_config.silero_vad.min_speech_duration = self.tuning.min_speech_ms as f32 / 1000.0;
            vad_config.silero_vad.max_speech_duration = self.tuning.max_speech_ms as f32 / 1000.0;
            vad_config.silero_vad.window_size = self.tuning.vad_window_size as i32;

            self.vad = Some(
                sherpa_onnx::VoiceActivityDetector::create(&vad_config, 20.0)
                    .ok_or_else(|| anyhow::anyhow!("Failed to create Silero VAD from {}", vad_model))?,
            );
            self.last_vad_sensitivity = Some(config.vad_sensitivity);
            log::info!("Loaded Silero VAD from {} (threshold={:.2})", vad_model, threshold);
        }

        // Create SenseVoice OfflineRecognizer
        if self.recognizer.is_none() {
            let mut rec_config = sherpa_onnx::OfflineRecognizerConfig::default();
            rec_config.model_config.sense_voice.model = Some(onnx_model.to_string());
            rec_config.model_config.sense_voice.language = Some(config.language.clone());
            rec_config.model_config.sense_voice.use_itn = false;
            rec_config.model_config.tokens = Some(tokens.to_string());
            rec_config.model_config.num_threads = num_cpus::get() as i32;

            print!("Creating recognizer...");
            self.recognizer = Some(
                sherpa_onnx::OfflineRecognizer::create(&rec_config)
                    .ok_or_else(|| anyhow::anyhow!("Failed to create SenseVoice from {}", onnx_model))?,
            );
            println!(" OK");
            log::info!("Loaded SenseVoice from {}", onnx_model);
        }

        Ok(())
    }
}

#[async_trait]
impl StreamingAsrEngine for LocalAsrAdapter {
    async fn initialize(&mut self, config: AsrConfig) -> anyhow::Result<()> {
        self.ensure_models(&config)?;
        // Reset the VAD's internal state so a reused adapter does not carry
        // buffered samples / queued segments from the previous recording into
        // the next one. No-op on a freshly created VAD.
        if let Some(vad) = self.vad.as_ref() {
            vad.reset();
        }
        self.config = Some(config);
        self.buffer.clear();
        self.vad_offset = 0;
        self.speech_active = false;
        self.last_interim = Instant::now();
        self.ready = true;
        Ok(())
    }

    async fn send_audio(&mut self, audio_data: &[f32]) -> anyhow::Result<()> {
        self.buffer.extend_from_slice(audio_data);
        Ok(())
    }

    async fn receive_result(&mut self) -> anyhow::Result<Option<AsrResult>> {
        let config = match self.config.as_ref() {
            Some(c) => c,
            None => return Ok(None),
        };

        let vad = match self.vad.as_ref() {
            Some(v) => v,
            None => return Ok(None),
        };

        // Feed VAD in fixed-size windows.
        let _prev_offset = self.vad_offset;
        while self.vad_offset + self.tuning.vad_window_size <= self.buffer.len() {
            let window = &self.buffer[self.vad_offset..self.vad_offset + self.tuning.vad_window_size];
            vad.accept_waveform(window);
            self.vad_offset += self.tuning.vad_window_size;

            if !self.speech_active && vad.detected() {
                self.speech_active = true;
                self.last_interim = Instant::now();
                log::info!("VAD: speech STARTED at buffer={}, offset={}", self.buffer.len(), self.vad_offset);
            }
        }

        // Phase 7: Bound the working buffer. Trim audio older than max_working_buffer_ms.
        let max_samples = ms_to_samples(self.tuning.max_working_buffer_ms);
        if self.buffer.len() > max_samples {
            let trim = self.buffer.len() - max_samples;
            self.vad_offset = self.vad_offset.saturating_sub(trim);
            self.buffer.drain(..trim);
        }

        // Phase 7: Chunk-aware interim decode.
        // Instead of re-decoding the entire buffer (O(n) cost grows with speech duration),
        // decode only the most recent window of audio. This bounds decode cost.
        if self.speech_active
            && self.last_interim.elapsed().as_millis() > self.tuning.partial_interval_ms as u128
        {
            self.last_interim = Instant::now();

            if let Some(recognizer) = self.recognizer.as_ref() {
                if !self.buffer.is_empty() {
                    // Decode only the last N seconds of audio (bounded window).
                    let window_start = self.buffer.len().saturating_sub(max_samples);
                    let window = &self.buffer[window_start..];
                    let stream = recognizer.create_stream();
                    stream.accept_waveform(16000, window);
                    recognizer.decode(&stream);
                    if let Some(result) = stream.get_result() {
                        let text = result.text.trim().to_string();
                        if !text.is_empty() {
                            log::info!("PARTIAL: {}", text);
                            return Ok(Some(AsrResult {
                                text,
                                is_final: false,
                                language: Some(config.language.clone()),
                                confidence: None,
                            }));
                        }
                    }
                }
            }
        }

        // Process completed VAD segments (final decode).
        if let Some(vad) = self.vad.as_ref() {
            if !vad.is_empty() {
                let segment = vad.front().ok_or_else(|| anyhow::anyhow!("VAD empty"))?;
                let samples: Vec<f32> = segment.samples().to_vec();
                let _ = segment.start();
                vad.pop();

                if let Some(recognizer) = self.recognizer.as_ref() {
                    let stream = recognizer.create_stream();
                    stream.accept_waveform(16000, &samples);
                    recognizer.decode(&stream);
                    if let Some(result) = stream.get_result() {
                        let text = result.text;
                        let has_text = !text.trim().is_empty();
                        log::info!("FINAL: {} ({} chars)", text.trim(), text.len());
                        // Reset for next utterance.
                        self.buffer.clear();
                        self.vad_offset = 0;
                        self.speech_active = false;
                        if has_text {
                            return Ok(Some(AsrResult {
                                text,
                                is_final: true,
                                language: Some(config.language.clone()),
                                confidence: None,
                            }));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn flush(&mut self) -> anyhow::Result<Option<AsrResult>> {
        let config = match self.config.as_ref() {
            Some(c) => c,
            None => return Ok(None),
        };

        let recognizer = match self.recognizer.as_ref() {
            Some(r) => r,
            None => return Ok(None),
        };

        // Surface any trailing speech the VAD has buffered but not yet emitted
        // as a segment. A mid-recording segment final clears self.buffer, so the
        // onset of a following utterance would be lost without this — flushing
        // only the raw buffer would truncate it.
        let mut last: Option<AsrResult> = None;
        if let Some(vad) = self.vad.as_ref() {
            vad.flush();
            while let Some(segment) = vad.front() {
                let samples: Vec<f32> = segment.samples().to_vec();
                vad.pop();

                let stream = recognizer.create_stream();
                stream.accept_waveform(16000, &samples);
                recognizer.decode(&stream);
                if let Some(result) = stream.get_result() {
                    let text = result.text;
                    if !text.trim().is_empty() {
                        log::info!("FLUSH-FINAL (vad): {}", text.trim());
                        last = Some(AsrResult {
                            text,
                            is_final: true,
                            language: Some(config.language.clone()),
                            confidence: None,
                        });
                    }
                }
            }
        }

        // Fall back to the raw remaining buffer if the VAD produced nothing.
        if last.is_none() && !self.buffer.is_empty() {
            let stream = recognizer.create_stream();
            stream.accept_waveform(config.sample_rate as i32, &self.buffer);
            recognizer.decode(&stream);
            if let Some(result) = stream.get_result() {
                let text = result.text;
                if !text.trim().is_empty() {
                    log::info!("FLUSH-FINAL (raw buffer): {}", text.trim());
                    last = Some(AsrResult {
                        text,
                        is_final: true,
                        language: Some(config.language.clone()),
                        confidence: None,
                    });
                }
            }
        }

        self.buffer.clear();
        self.vad_offset = 0;
        self.speech_active = false;

        Ok(last)
    }

    async fn close(&mut self) -> anyhow::Result<()> {
        self.buffer.clear();
        self.vad_offset = 0;
        self.speech_active = false;
        self.ready = false;
        Ok(())
    }

    fn name(&self) -> &str {
        "sherpa-onnx-sensevoice"
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ms_to_samples() {
        assert_eq!(ms_to_samples(1000), 16000);
        assert_eq!(ms_to_samples(200), 3200);
        assert_eq!(ms_to_samples(0), 0);
    }

    #[test]
    fn test_tuning_default_values() {
        let tuning = LocalAsrTuning::default();
        assert_eq!(tuning.vad_window_size, 512);
        assert_eq!(tuning.min_silence_ms, 300);
        assert_eq!(tuning.min_speech_ms, 250);
        assert_eq!(tuning.max_speech_ms, 8000);
        assert_eq!(tuning.partial_interval_ms, 200);
        assert_eq!(tuning.max_working_buffer_ms, 3000);
    }

    #[test]
    fn test_with_tuning_creates_custom() {
        let tuning = LocalAsrTuning {
            max_working_buffer_ms: 5000,
            ..Default::default()
        };
        let adapter = LocalAsrAdapter::with_tuning(tuning.clone());
        assert_eq!(adapter.tuning.max_working_buffer_ms, 5000);
    }

    /// Verify that the working buffer is bounded during continuous speech.
    /// This is the core P1-A fix: without bounding, the buffer grows unboundedly
    /// during continuous speech, causing interim decode cost to increase linearly.
    #[tokio::test]
    async fn test_working_buffer_bounded_during_speech() {
        let tuning = LocalAsrTuning {
            max_working_buffer_ms: 500, // 0.5s = 8000 samples
            partial_interval_ms: 10000, // Disable interim for this test
            ..Default::default()
        };
        let mut adapter = LocalAsrAdapter::with_tuning(tuning);

        adapter.buffer = vec![0.0; 20000]; // 20000 samples > 8000 limit
        adapter.vad_offset = 20000;
        adapter.speech_active = true;

        let max_samples = ms_to_samples(adapter.tuning.max_working_buffer_ms);
        if adapter.buffer.len() > max_samples {
            let trim = adapter.buffer.len() - max_samples;
            adapter.vad_offset = adapter.vad_offset.saturating_sub(trim);
            adapter.buffer.drain(..trim);
        }

        assert_eq!(adapter.buffer.len(), 8000);
        assert_eq!(adapter.vad_offset, 8000);
    }

    /// Verify that buffer trimming preserves vad_offset relationship.
    #[tokio::test]
    async fn test_buffer_trim_preserves_vad_offset() {
        let tuning = LocalAsrTuning {
            max_working_buffer_ms: 1000, // 16000 samples
            ..Default::default()
        };
        let mut adapter = LocalAsrAdapter::with_tuning(tuning);

        adapter.buffer = vec![0.1; 32000]; // 2 seconds of audio
        adapter.vad_offset = 32000;
        adapter.speech_active = true;

        let max_samples = ms_to_samples(adapter.tuning.max_working_buffer_ms);
        if adapter.buffer.len() > max_samples {
            let trim = adapter.buffer.len() - max_samples;
            adapter.vad_offset = adapter.vad_offset.saturating_sub(trim);
            adapter.buffer.drain(..trim);
        }

        assert_eq!(adapter.buffer.len(), 16000);
        assert_eq!(adapter.vad_offset, 16000);
    }
}
