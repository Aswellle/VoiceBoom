// Audio capture module using CPAL
// Captures PCM audio from the default microphone and resamples to 16kHz mono.
//
// Since cpal::Stream is not Send, we use a dedicated thread approach:
// audio samples are sent through a tokio channel to the ASR engine.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Target sample rate for the ASR engine (SenseVoice expects 16kHz).
pub const TARGET_SAMPLE_RATE: u32 = 16000;

/// Manages audio capture from the default microphone.
pub struct AudioCapture {
    is_recording: Arc<AtomicBool>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
    cmd_tx: Option<std::sync::mpsc::Sender<bool>>,
    audio_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<f32>>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            cmd_tx: None,
            audio_tx: None,
        }
    }

    /// Start recording audio, returning a receiver for PCM samples (C2 fix).
    ///
    /// Uses the device's native sample rate (not all devices support 16kHz) and
    /// resamples to TARGET_SAMPLE_RATE in the callback.
    pub fn start_recording(
        &mut self,
        device_name: Option<&str>,
    ) -> anyhow::Result<tokio::sync::mpsc::UnboundedReceiver<Vec<f32>>> {
        if self.is_recording.load(Ordering::SeqCst) {
            anyhow::bail!("Already recording");
        }

        // Copy the device name into an owned String before moving into the
        // 'static thread — a borrowed &str would escape the method scope.
        let device_name = device_name.map(String::from);

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<bool>();
        self.cmd_tx = Some(cmd_tx);

        // Channel for sending PCM samples to ASR
        let (audio_tx, audio_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<f32>>();
        self.audio_tx = Some(audio_tx.clone());

        let is_recording = self.is_recording.clone();

        self.thread_handle = Some(std::thread::spawn(move || {
            let host = cpal::default_host();
            // Pick the requested device by name, or fall back to the default.
            let device = if let Some(ref name) = device_name {
                match host.input_devices() {
                    Ok(mut devs) => devs
                        .find(|d| d.name().ok().as_deref() == Some(name))
                        .or_else(|| host.default_input_device()),
                    Err(_) => host.default_input_device(),
                }
            } else {
                host.default_input_device()
            };
            let device = match device {
                Some(d) => d,
                None => {
                    log::error!("No input device available");
                    return;
                }
            };

            let supported = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to get default input config: {}", e);
                    return;
                }
            };

            let input_sample_rate = supported.sample_rate().0;
            // Clamp to >= 1 so a misbehaving device reporting 0 channels can't
            // cause data.chunks(0) to panic or a divide-by-zero in the downmix.
            let channels = (supported.channels() as usize).max(1);
            let sample_format = supported.sample_format();

            log::info!(
                "Audio device: {} Hz, {} channels, {:?}",
                input_sample_rate,
                channels,
                sample_format
            );

            // Use the device's NATIVE sample rate — forcing 16kHz fails on devices
            // that don't support it (e.g. many Realtek chips at 48kHz).
            let mut config: cpal::StreamConfig = supported.config();
            config.buffer_size = cpal::BufferSize::Default;

            let is_recording_cb = is_recording.clone();
            let audio_tx_cb = audio_tx.clone();

            let err_fn = |err| eprintln!("Audio stream error: {:?}", err);

            // Resample ratio: input_sr / target_sr
            let resample_ratio = input_sample_rate as f32 / TARGET_SAMPLE_RATE as f32;
            let mut resample_pos: f32 = 0.0;

            let stream_result: Result<cpal::Stream, cpal::BuildStreamError> =
                match sample_format {
                    SampleFormat::F32 => device.build_input_stream(
                        &config,
                        move |data: &[f32], &_| {
                            if !is_recording_cb.load(Ordering::SeqCst) {
                                return;
                            }
                            // Downmix to mono first
                            let mono: Vec<f32> = data
                                .chunks(channels)
                                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                                .collect();
                            // Resample to target rate using linear interpolation
                            let resampled =
                                resample_linear(&mono, resample_ratio, &mut resample_pos);
                            let _ = audio_tx_cb.send(resampled);
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::I16 => device.build_input_stream(
                        &config,
                        move |data: &[i16], &_| {
                            if !is_recording_cb.load(Ordering::SeqCst) {
                                return;
                            }
                            let mono: Vec<f32> = data
                                .chunks(channels)
                                .map(|frame| {
                                    frame
                                        .iter()
                                        .map(|&s| s as f32 / i16::MAX as f32)
                                        .sum::<f32>()
                                        / channels as f32
                                })
                                .collect();
                            let resampled =
                                resample_linear(&mono, resample_ratio, &mut resample_pos);
                            let _ = audio_tx_cb.send(resampled);
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::U16 => device.build_input_stream(
                        &config,
                        move |data: &[u16], &_| {
                            if !is_recording_cb.load(Ordering::SeqCst) {
                                return;
                            }
                            let mono: Vec<f32> = data
                                .chunks(channels)
                                .map(|frame| {
                                    frame
                                        .iter()
                                        .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                        .sum::<f32>()
                                        / channels as f32
                                })
                                .collect();
                            let resampled =
                                resample_linear(&mono, resample_ratio, &mut resample_pos);
                            let _ = audio_tx_cb.send(resampled);
                        },
                        err_fn,
                        None,
                    ),
                    _ => {
                        eprintln!("Unsupported sample format");
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

            log::info!(
                "Audio capture started at {} Hz, resampling to {} Hz",
                input_sample_rate,
                TARGET_SAMPLE_RATE
            );

            // Thread loop: wait for stop signal
            while let Ok(should_stop) = cmd_rx.recv() {
                if should_stop {
                    break;
                }
            }

            // Stream is dropped here, stopping audio
            drop(stream);
            log::info!("Audio capture stopped");
        }));

        self.is_recording.store(true, Ordering::SeqCst);
        Ok(audio_rx)
    }

    /// Stop recording.
    ///
    /// Drops the struct-held audio_tx clone so the mpsc channel closes once the
    /// CPAL thread exits. The bridge task's `recv()` then returns None, the task
    /// exits, and its BridgeActiveGuard resets bridge_active — without this, the
    /// channel stays open forever and every recording after the first deadlocks
    /// on the bridge_active wait in start_recording.
    pub fn stop_recording(&mut self) {
        self.is_recording.store(false, Ordering::SeqCst);
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(true);
        }
        // Drop our clone of the sender so the channel closes.
        self.audio_tx = None;
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    /// List available audio input devices.
    pub fn list_devices(&self) -> Vec<(String, String)> {
        let mut result = Vec::new();
        if let Ok(host) = cpal::default_host().input_devices() {
            for device in host {
                if let Ok(name) = device.name() {
                    result.push((name.clone(), name));
                }
            }
        }
        result
    }
}

/// Simple resampler with anti-aliasing for downsampling.
///
/// `ratio` = input_sample_rate / target_sample_rate.
/// `pos` tracks the current position in the input and is updated in place.
fn resample_linear(input: &[f32], ratio: f32, pos: &mut f32) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }
    if ratio <= 0.0 {
        return input.to_vec();
    }

    let input_len = input.len();
    let output_len = ((input_len as f32) / ratio) as usize;
    let mut output = Vec::with_capacity(output_len.max(1));

    // Downsampling (ratio > 1): box-filter over the decimation window before
    // picking each sample, attenuating frequencies above the output Nyquist
    // rate that would otherwise alias into the speech band (e.g. a 48 kHz mic
    // downsampled to 16 kHz). Upsampling keeps linear interpolation.
    let win = if ratio > 1.0 { ratio.ceil() as usize } else { 1 };

    let mut idx = *pos;
    while (idx as usize) < input_len {
        let i = idx as usize;
        let sample = if win > 1 {
            let start = i.saturating_sub(win / 2);
            let end = (i + win / 2 + 1).min(input_len);
            let count = end - start;
            if count > 0 {
                input[start..end].iter().sum::<f32>() / count as f32
            } else {
                0.0
            }
        } else {
            let frac = idx - i as f32;
            if i + 1 < input_len {
                input[i] * (1.0 - frac) + input[i + 1] * frac
            } else {
                input[i]
            }
        };
        output.push(sample);
        idx += ratio;
    }

    // Save position for next call (carry over fractional offset)
    if idx >= input_len as f32 {
        *pos = idx - input_len as f32;
    }

    output
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop_recording();
    }
}
