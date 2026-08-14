// Debug ASR pipeline — minimal test following the official sherpa-onnx example
// Uses the SenseVoice model we already have.
// Run: cargo run --bin asr_debug --release

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::time::Instant;

fn main() {
    println!("=== ASR Debug Test ===\n");

    // Model paths — resolve relative to exe location
    let exe = std::env::current_exe().unwrap();
    let exe_dir = exe.parent().unwrap();
    let models_dir = exe_dir.join("../asr-bundle/sensevoice/models");
    println!("Looking for models in: {:?}", models_dir);

    // Fallback for dev mode
    let models_dir = if models_dir.exists() {
        models_dir
    } else {
        std::path::PathBuf::from("src-tauri/asr-bundle/sensevoice/models")
    };
    println!("Using models dir: {:?}", models_dir);
    println!();

    let vad_path = models_dir.join("silero_vad.onnx");
    let model_path = models_dir.join("model.int8.onnx");
    let tokens_path = models_dir.join("tokens.txt");

    println!("VAD model: {} ({})", vad_path.display(), vad_path.exists());
    println!("ASR model: {} ({})", model_path.display(), model_path.exists());
    println!("Tokens: {} ({})", tokens_path.display(), tokens_path.exists());
    println!();

    if !vad_path.exists() || !model_path.exists() || !tokens_path.exists() {
        eprintln!("ERROR: Model files not found!");
        std::process::exit(1);
    }

    // Create VAD (exact config from official example)
    let mut vad_config = sherpa_onnx::VadModelConfig::default();
    vad_config.silero_vad.model = Some(vad_path.to_string_lossy().to_string());
    vad_config.silero_vad.threshold = 0.5;
    vad_config.silero_vad.min_silence_duration = 0.1;
    vad_config.silero_vad.min_speech_duration = 0.25;
    vad_config.silero_vad.max_speech_duration = 8.0;
    vad_config.silero_vad.window_size = 512;
    vad_config.sample_rate = 16000;

    print!("Creating VAD...");
    let vad = match sherpa_onnx::VoiceActivityDetector::create(&vad_config, 20.0) {
        Some(v) => {
            println!(" OK");
            v
        }
        None => {
            eprintln!(" FAILED!");
            std::process::exit(1);
        }
    };

    // Create SenseVoice recognizer (exact config from official example)
    let mut rec_config = sherpa_onnx::OfflineRecognizerConfig::default();
    rec_config.model_config.sense_voice.model = Some(model_path.to_string_lossy().to_string());
    rec_config.model_config.sense_voice.language = Some("auto".to_string());
    rec_config.model_config.sense_voice.use_itn = false;
    rec_config.model_config.tokens = Some(tokens_path.to_string_lossy().to_string());
    rec_config.model_config.num_threads = 4;

    print!("Creating recognizer...");
    let recognizer = match sherpa_onnx::OfflineRecognizer::create(&rec_config) {
        Some(r) => {
            println!(" OK");
            r
        }
        None => {
            eprintln!(" FAILED!");
            std::process::exit(1);
        }
    };
    println!();

    // Audio capture
    let host = cpal::default_host();
    let device = host.default_input_device().expect("No input device");
    let supported = device.default_input_config().expect("No config");
    println!("Mic: {} ({} Hz, {:?}, {} ch)",
        device.name().unwrap_or_default(),
        supported.sample_rate().0,
        supported.sample_format(),
        supported.channels()
    );

    // Use native sample rate, capture mono
    let input_sr = supported.sample_rate().0 as i32;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();

    println!("Capturing at {} Hz native rate", input_sr);
    println!();

    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let err_fn = |err| eprintln!("Audio error: {:?}", err);

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                let mono: Vec<f32> = data.chunks(channels).map(|f| f[0]).collect();
                let _ = tx.send(mono);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                let mono: Vec<f32> = data.chunks(channels).map(|f| f[0] as f32 / 32768.0).collect();
                let _ = tx.send(mono);
            },
            err_fn,
            None,
        ),
        _ => {
            eprintln!("Unsupported format");
            std::process::exit(1);
        }
    }
    .expect("Failed to build stream");

    stream.play().expect("Failed to play");

    println!("=== Recording 10 seconds — SPEAK NOW ===\n");

    let start = Instant::now();
    let mut buffer: Vec<f32> = Vec::new();
    let mut offset: usize = 0;
    let mut speech_started = false;
    let mut started_time = Instant::now();
    let mut total_frames = 0;
    let mut vad_windows = 0;

    while start.elapsed().as_secs() < 10 {
        // Collect audio
        while let Ok(samples) = rx.try_recv() {
            total_frames += samples.len();
            buffer.extend_from_slice(&samples);
        }

        // Feed VAD in 512-sample windows (at 16kHz)
        while offset + 512 <= buffer.len() {
            vad.accept_waveform(&buffer[offset..offset + 512]);
            offset += 512;
            vad_windows += 1;

            if !speech_started && vad.detected() {
                speech_started = true;
                started_time = Instant::now();
                println!("[{:.1}s] VAD: speech detected!", start.elapsed().as_secs_f32());
            }
        }

        // Interim decode every 0.2s
        let elapsed = started_time.elapsed().as_secs_f32();
        if speech_started && elapsed > 0.2 {
            let s = recognizer.create_stream();
            s.accept_waveform(input_sr, &buffer);
            recognizer.decode(&s);
            if let Some(result) = s.get_result() {
                let text = result.text.trim();
                if !text.is_empty() {
                    println!("[{:.1}s] PARTIAL: {}", start.elapsed().as_secs_f32(), text);
                }
            }
            started_time = Instant::now();
        }

        // Process completed VAD segments
        while !vad.is_empty() {
            let segment = vad.front().unwrap();
            let samples = segment.samples().to_vec();
            vad.pop();

            let s = recognizer.create_stream();
            s.accept_waveform(input_sr, &samples);
            recognizer.decode(&s);
            if let Some(result) = s.get_result() {
                let text = result.text.trim();
                if !text.is_empty() {
                    println!("[{:.1}s] FINAL: {}", start.elapsed().as_secs_f32(), text);
                } else {
                    println!("[{:.1}s] FINAL: (empty text)", start.elapsed().as_secs_f32());
                }
            }

            buffer.clear();
            offset = 0;
            speech_started = false;
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    drop(stream);

    println!("\n=== Summary ===");
    println!("Total audio frames: {}", total_frames);
    println!("VAD windows processed: {}", vad_windows);
    println!("Speech detected: {}", speech_started);

    if vad_windows == 0 {
        println!("FAIL: No audio reached VAD!");
    } else if !speech_started {
        println!("WARN: Audio captured but VAD never detected speech");
        println!("  → Try speaking louder or check mic volume");
    }
}
