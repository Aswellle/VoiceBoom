// Voice Activity Detection (VAD) module
// Simple energy-based VAD for detecting speech segments

/// VAD configuration
pub struct VadConfig {
    /// Energy threshold above which audio is considered speech
    pub energy_threshold: f32,
    /// Minimum number of consecutive speech frames to trigger speech start
    pub min_speech_frames: usize,
    /// Number of consecutive silence frames to trigger speech end
    pub max_silence_frames: usize,
    /// Frame size in samples (20ms at 16kHz = 320 samples)
    pub frame_size: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            energy_threshold: 0.001,
            min_speech_frames: 3,
            max_silence_frames: 25,
            frame_size: 320,
        }
    }
}

/// VAD state machine
pub struct VoiceActivityDetector {
    config: VadConfig,
    speech_frame_count: usize,
    silence_frame_count: usize,
    is_speech_active: bool,
}

/// VAD result for a processed frame
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadState {
    Silence,
    SpeechStart,
    InSpeech,
    SpeechEnd,
}

impl VoiceActivityDetector {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            speech_frame_count: 0,
            silence_frame_count: 0,
            is_speech_active: false,
        }
    }

    /// Process a frame of audio samples
    pub fn process_frame(&mut self, samples: &[f32]) -> VadState {
        let energy = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;

        if energy > self.config.energy_threshold {
            self.speech_frame_count += 1;
            self.silence_frame_count = 0;

            if !self.is_speech_active && self.speech_frame_count >= self.config.min_speech_frames {
                self.is_speech_active = true;
                VadState::SpeechStart
            } else if self.is_speech_active {
                VadState::InSpeech
            } else {
                VadState::Silence
            }
        } else {
            self.silence_frame_count += 1;

            if self.is_speech_active && self.silence_frame_count >= self.config.max_silence_frames {
                self.is_speech_active = false;
                self.speech_frame_count = 0;
                VadState::SpeechEnd
            } else {
                VadState::Silence
            }
        }
    }

    /// Reset VAD state
    pub fn reset(&mut self) {
        self.speech_frame_count = 0;
        self.silence_frame_count = 0;
        self.is_speech_active = false;
    }

    /// Check if speech is currently active
    pub fn is_speech_active(&self) -> bool {
        self.is_speech_active
    }
}
