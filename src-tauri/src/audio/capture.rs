// Audio capture module using CPAL
// Captures 16kHz mono PCM audio from the default microphone
//
// Since cpal::Stream is not Send, we use a dedicated thread approach:
// audio samples are sent through a tokio channel to the ASR engine.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, SampleFormat};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

pub const TARGET_SAMPLE_RATE: u32 = 16000;
pub const CHANNELS: u16 = 1;

/// Audio capture handle — Send + Sync safe wrapper
pub struct AudioCapture {
    is_recording: Arc<AtomicBool>,
    cmd_tx: Option<std::sync::mpsc::Sender<bool>>,
    // C2 fix: store the audio sender so ASR can receive samples
    audio_tx: Option<mpsc::UnboundedSender<Vec<f32>>>,
    // m10 fix: Store thread handle for clean shutdown
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        // Signal thread to stop and wait for it
        self.stop_recording();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            cmd_tx: None,
            audio_tx: None,
            thread_handle: None,
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

    /// Start recording audio, returning a receiver for PCM samples (C2 fix)
    pub fn start_recording(&mut self) -> anyhow::Result<mpsc::UnboundedReceiver<Vec<f32>>> {
        if self.is_recording.load(Ordering::SeqCst) {
            anyhow::bail!("Already recording");
        }

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<bool>();
        self.cmd_tx = Some(cmd_tx);

        // C2 fix: Create channel for sending PCM samples to ASR
        let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();
        self.audio_tx = Some(audio_tx.clone());

        let is_recording = self.is_recording.clone();

        // m10 fix: Store thread handle for clean shutdown
        self.thread_handle = Some(std::thread::spawn(move || {
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

            // M5 fix: Use target sample rate, not max
            let channels = config.channels().max(1);
            let err_fn = |err| eprintln!("Audio stream error: {}", err);

            let is_recording_cb = is_recording.clone();
            let audio_tx_cb = audio_tx.clone();

            let stream_result: Result<cpal::Stream, cpal::BuildStreamError> =
                match config.sample_format() {
                    SampleFormat::F32 => device.build_input_stream(
                        &cpal::StreamConfig {
                            channels,
                            sample_rate: SampleRate(TARGET_SAMPLE_RATE),
                            buffer_size: cpal::BufferSize::Default,
                        },
                        move |data: &[f32], &_| {
                            if !is_recording_cb.load(Ordering::SeqCst) {
                                return;
                            }
                            // M5 fix: Downmix to mono
                            let mono: Vec<f32> = if channels == 1 {
                                data.to_vec()
                            } else {
                                data.chunks(channels as usize)
                                    .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                                    .collect()
                            };
                            // C2 fix: Send PCM samples to ASR via channel
                            let _ = audio_tx_cb.send(mono);
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::I16 => device.build_input_stream(
                        &cpal::StreamConfig {
                            channels,
                            sample_rate: SampleRate(TARGET_SAMPLE_RATE),
                            buffer_size: cpal::BufferSize::Default,
                        },
                        move |data: &[i16], &_| {
                            if !is_recording_cb.load(Ordering::SeqCst) {
                                return;
                            }
                            let mono: Vec<f32> = data
                                .chunks(channels as usize)
                                .map(|frame| {
                                    frame.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>()
                                        / channels as f32
                                })
                                .collect();
                            let _ = audio_tx_cb.send(mono);
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::U16 => device.build_input_stream(
                        &cpal::StreamConfig {
                            channels,
                            sample_rate: SampleRate(TARGET_SAMPLE_RATE),
                            buffer_size: cpal::BufferSize::Default,
                        },
                        move |data: &[u16], &_| {
                            if !is_recording_cb.load(Ordering::SeqCst) {
                                return;
                            }
                            let mono: Vec<f32> = data
                                .chunks(channels as usize)
                                .map(|frame| {
                                    frame
                                        .iter()
                                        .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                        .sum::<f32>()
                                        / channels as f32
                                })
                                .collect();
                            let _ = audio_tx_cb.send(mono);
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
        }));

        self.is_recording.store(true, Ordering::SeqCst);
        Ok(audio_rx)
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
