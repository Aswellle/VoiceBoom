//! Fake Audio Source — generates synthetic audio frames for testing.
//!
//! Used by integration tests to simulate microphone input without real hardware.
//! Produces silence (all zeros) or a sine wave pattern.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::audio::pipeline::AudioFrame;

/// A fake audio source that generates synthetic frames.
pub struct FakeAudioSource {
    frame_count: AtomicUsize,
    sample_rate: u32,
    frame_samples: usize,
}

impl FakeAudioSource {
    /// Create a new fake audio source.
    pub fn new(sample_rate: u32, frame_samples: usize) -> Self {
        Self {
            frame_count: AtomicUsize::new(0),
            sample_rate,
            frame_samples,
        }
    }

    /// Generate the next audio frame (silence).
    pub fn next_frame(&self) -> AudioFrame {
        let seq = self.frame_count.fetch_add(1, Ordering::SeqCst);
        AudioFrame {
            sequence: seq as u64,
            timestamp: Instant::now(),
            samples: vec![0.0f32; self.frame_samples],
        }
    }

    /// Generate a frame with a sine wave (for VAD testing).
    pub fn next_sine_frame(&self, frequency: f32) -> AudioFrame {
        let seq = self.frame_count.fetch_add(1, Ordering::SeqCst);
        let samples: Vec<f32> = (0..self.frame_samples)
            .map(|i| {
                let t = (seq * self.frame_samples + i) as f32 / self.sample_rate as f32;
                (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5
            })
            .collect();
        AudioFrame {
            sequence: seq as u64,
            timestamp: Instant::now(),
            samples,
        }
    }

    /// Total frames generated so far.
    pub fn frame_count(&self) -> usize {
        self.frame_count.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_audio_source_generates_silence() {
        let source = FakeAudioSource::new(16000, 1024);
        let frame = source.next_frame();
        assert_eq!(frame.samples.len(), 1024);
        assert!(frame.samples.iter().all(|&s| s == 0.0));
        assert_eq!(source.frame_count(), 1);
    }

    #[test]
    fn test_fake_audio_source_generates_sine_wave() {
        let source = FakeAudioSource::new(16000, 1024);
        let frame = source.next_sine_frame(440.0);
        assert_eq!(frame.samples.len(), 1024);
        // Sine wave should have non-zero values
        assert!(frame.samples.iter().any(|&s| s != 0.0));
    }

    #[test]
    fn test_fake_audio_source_sequence_increments() {
        let source = FakeAudioSource::new(16000, 512);
        let f1 = source.next_frame();
        let f2 = source.next_frame();
        assert_eq!(f1.sequence, 0);
        assert_eq!(f2.sequence, 1);
    }
}
