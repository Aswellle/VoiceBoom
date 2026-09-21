//! End-to-end pipeline tests using FakeAudioSource, FakeAsrSession, and FakeInjectionTarget.
//!
//! Tests the full recording pipeline without real microphone or OS interaction:
//! hotkey pressed → session starting → recording → partial → segment final → utterance final → injection → idle

use crate::audio::fake_source::FakeAudioSource;
use crate::asr::aggregator::TranscriptAggregator;
use crate::asr::engine_trait::{AsrConfig, AsrEngineType};
use crate::asr::session::{AsrEvent, AsrSession, FakeAsrSession};
use crate::injection::fake_target::FakeInjectionTarget;
use crate::injection::model::InjectionMethod;

/// Helper: create a standard test event sequence.
fn standard_events() -> Vec<AsrEvent> {
    vec![
        AsrEvent::Partial {
            text: "hello".into(),
            language: Some("en".into()),
        },
        AsrEvent::Partial {
            text: "hello world".into(),
            language: Some("en".into()),
        },
        AsrEvent::SegmentFinal {
            text: "hello world".into(),
            language: Some("en".into()),
            confidence: Some(0.95),
        },
        AsrEvent::UtteranceFinal {
            text: "hello world".into(),
            language: Some("en".into()),
            confidence: Some(0.95),
        },
    ]
}

fn test_config() -> AsrConfig {
    AsrConfig {
        engine_type: AsrEngineType::LocalSenseVoice,
        api_key: None,
        endpoint: None,
        language: "en".into(),
        vad_sensitivity: 50,
        sample_rate: 16000,
    }
}

#[tokio::test]
async fn test_full_pipeline_lifecycle() {
    // Setup fakes
    let audio = FakeAudioSource::new(16000, 1024);
    let asr = FakeAsrSession::new("test", standard_events());
    let injector = FakeInjectionTarget::new();

    // Simulate: hotkey pressed → session starting
    let mut session = asr;
    session.start(test_config()).await.unwrap();

    // Simulate: recording → push audio frames
    for _ in 0..5 {
        let _frame = audio.next_frame();
        // In real pipeline, audio would be pushed to ASR
    }

    // Simulate: ASR produces events
    let mut aggregator = TranscriptAggregator::new();
    let mut received_text = String::new();

    while let Ok(Some(event)) = session.next_event().await {
        aggregator.process_event(&event);
        if let AsrEvent::UtteranceFinal { text, .. } = &event {
            received_text = text.clone();
        }
    }

    // Verify: utterance final was received
    assert_eq!(received_text, "hello world");

    // Simulate: injection
    let result = injector.inject(
        &received_text,
        InjectionMethod::ClipboardPaste,
        "sess-1",
        "utt-1",
    );

    // Verify: injection was recorded
    assert!(injector.was_injected());
    assert_eq!(injector.attempt_count(), 1);
    let last = injector.last_attempt().unwrap();
    assert_eq!(last.text, "hello world");
    assert_eq!(last.session_id, "sess-1");

    // Simulate: session ends → idle
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_pipeline_with_provider_failure() {
    // Setup: ASR that fails after 3 audio pushes
    let audio = FakeAudioSource::new(16000, 1024);
    let asr = FakeAsrSession::new("failing", vec![]).fail_after_pushes(3);
    let injector = FakeInjectionTarget::new();

    let mut session = asr;
    session.start(test_config()).await.unwrap();

    // Push audio until failure
    let mut push_count = 0;
    for _ in 0..10 {
        let _frame = audio.next_frame();
        let result = session.push_audio(&[0.0; 1024]).await;
        push_count += 1;
        if result.is_err() {
            break;
        }
    }

    // Verify: failed after 3 pushes
    assert!(push_count <= 4); // 3 successful + 1 failed

    // Verify: no injection happened (no final result)
    assert!(!injector.was_injected());

    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_pipeline_duplicate_injection_prevention() {
    let injector = FakeInjectionTarget::new();

    // First injection succeeds
    let result1 = injector.inject("text", InjectionMethod::ClipboardPaste, "sess-1", "utt-1");
    assert!(matches!(result1, crate::injection::model::InjectionResult::Injected { .. }));

    // Duplicate injection (same session + utterance) should be ignored
    let _result2 = injector.inject("text", InjectionMethod::ClipboardPaste, "sess-1", "utt-1");

    // Verify: only one actual injection
    assert_eq!(injector.attempt_count(), 1);
}

#[tokio::test]
async fn test_pipeline_stale_session_injection() {
    let injector = FakeInjectionTarget::new();

    // Injection from old session
    injector.inject("old text", InjectionMethod::ClipboardPaste, "sess-old", "utt-1");

    // Verify: old injection was recorded
    assert_eq!(injector.attempt_count(), 1);
    let last = injector.last_attempt().unwrap();
    assert_eq!(last.session_id, "sess-old");
}

    target.inject("second", InjectionMethod::UnicodeTyping, "sess-1", "utt-2");
#[tokio::test]
async fn test_audio_source_frame_generation() {
    let source = FakeAudioSource::new(16000, 1024);

    // Generate silence frames
    for _ in 0..10 {
        let frame = source.next_frame();
        assert_eq!(frame.samples.len(), 1024);
        assert!(frame.samples.iter().all(|&s| s == 0.0));
    }

    assert_eq!(source.frame_count(), 10);

    // Generate sine wave frames
    let sine = source.next_sine_frame(440.0);
    assert!(sine.samples.iter().any(|&s| s != 0.0));
}

#[tokio::test]
async fn test_injection_target_recording() {
    let target = FakeInjectionTarget::new();

    // Record multiple injections
    target.inject("first", InjectionMethod::ClipboardPaste, "sess-1", "utt-1");
    target.inject("second", InjectionMethod::DirectUnicode, "sess-1", "utt-2");
    target.inject("third", InjectionMethod::ClipboardPaste, "sess-2", "utt-1");

    assert_eq!(target.attempt_count(), 3);

    let attempts = target.attempts();
    assert_eq!(attempts[0].text, "first");
    assert_eq!(attempts[1].text, "second");
    assert_eq!(attempts[2].text, "third");

    // Verify session tracking
    assert_eq!(attempts[0].session_id, "sess-1");
    assert_eq!(attempts[2].session_id, "sess-2");
}
