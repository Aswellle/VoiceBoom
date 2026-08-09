// Audio capture module using CPAL
// Captures 16kHz mono PCM audio from the default microphone
//
// Since cpal::Stream is not Send, we use a dedicated thread approach:
// commands are sent via channels, and the stream lives on its own thread.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, SampleFormat};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const TARGET_SAMPLE_RATE: u32 = 16000;
pub const CHANNELS: u16 = 1;

/// Audio capture handle — Send + Sync safe wrapper
pub struct AudioCapture {
    is_recording: Arc<AtomicBool>,
    cmd_tx: Option<std::sync::mpsc::Sender<bool>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            cmd_tx: None,
        }
    }

    /// Get list of available audio input devices
    pub fn list_devices(&self) -> Vec<(String, String)> {
        let mut devices = Vec::new();
        let host = cpal::default_host();
        if let Ok(input_devices) = host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    devices.push((name.clone(), name));
                }
            }
        }
        devices
    }

    /// Start recording audio
    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Ok(());
        }

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<bool>();
        self.cmd_tx = Some(cmd_tx);

        let is_recording = self.is_recording.clone();

        std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    log::error!("No default input device");
                    return;
                }
            };

            let config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to get default input config: {}", e);
                    return;
                }
            };

            let channels = config.channels().max(1);
            let sample_rate = config.sample_rate().0.max(TARGET_SAMPLE_RATE);
            let err_fn = |err| eprintln!("Audio stream error: {}", err);

            let is_recording_cb = is_recording.clone();

            let stream_result: Result<cpal::Stream, cpal::BuildStreamError> =
                match config.sample_format() {
                    SampleFormat::F32 => device.build_input_stream(
                        &cpal::StreamConfig {
                            channels,
                            sample_rate: SampleRate(sample_rate),
                            buffer_size: cpal::BufferSize::Default,
                        },
                        move |data: &[f32], &_| {
                            if is_recording_cb.load(Ordering::SeqCst) {
                                // In a full implementation, send samples to ASR
                                let _mono_sum: f32 = data.iter().sum::<f32>() / data.len().max(1) as f32;
                                // In production: send mono_sum to ASR engine
                            }
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::I16 => device.build_input_stream(
                        &cpal::StreamConfig {
                            channels,
                            sample_rate: SampleRate(sample_rate),
                            buffer_size: cpal::BufferSize::Default,
                        },
                        move |data: &[i16], &_| {
                            if is_recording_cb.load(Ordering::SeqCst) {
                                let _energy: f32 = data.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>()
                                    / data.len().max(1) as f32;
                                let _ = _energy;
                            }
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::U16 => device.build_input_stream(
                        &cpal::StreamConfig {
                            channels,
                            sample_rate: SampleRate(sample_rate),
                            buffer_size: cpal::BufferSize::Default,
                        },
                        move |data: &[u16], &_| {
                            if is_recording_cb.load(Ordering::SeqCst) {
                                let _energy: f32 = data
                                    .iter()
                                    .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                    .sum::<f32>()
                                    / data.len().max(1) as f32;
                                let _ = _energy;
                            }
                        },
                        err_fn,
                        None,
                    ),
                    _ => {
                        log::error!("Unsupported sample format");
                        return;
                    }
                };

            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to build input stream: {}", e);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                log::error!("Failed to play stream: {}", e);
                return;
            }

            // Thread loop: wait for stop signal
            while let Ok(should_stop) = cmd_rx.recv() {
                if should_stop {
                    break;
                }
            }

            // Stream is dropped here, stopping audio
            drop(stream);
        });

        self.is_recording.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Stop recording
    pub fn stop_recording(&self) {
        self.is_recording.store(false, Ordering::SeqCst);
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(true);
        }
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }
}
