//! Fake Injection Target — records injection attempts for testing.
//!
//! Used by integration tests to verify that text injection is called
//! with the correct parameters, without actually interacting with the OS.

use crate::injection::model::{InjectionMethod, InjectionResult};
use std::sync::{Arc, Mutex};

/// Records all injection attempts for test verification.
#[derive(Debug, Clone, Default)]
pub struct FakeInjectionTarget {
    pub attempts: Arc<Mutex<Vec<InjectionAttempt>>>,
}

/// A single recorded injection attempt.
#[derive(Debug, Clone)]
pub struct InjectionAttempt {
    pub text: String,
    pub method: InjectionMethod,
    pub session_id: String,
    pub utterance_id: String,
}

impl FakeInjectionTarget {
    /// Create a new fake injection target.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an injection attempt.
    pub fn inject(
        &self,
        text: &str,
        method: InjectionMethod,
        session_id: &str,
        utterance_id: &str,
    ) -> InjectionResult {
        let attempt = InjectionAttempt {
            text: text.to_string(),
            method,
            session_id: session_id.to_string(),
            utterance_id: utterance_id.to_string(),
        };
        self.attempts.lock().unwrap().push(attempt);
        InjectionResult::Injected {
            method,
            verified: true,
        }
    }

    /// Get all recorded attempts.
    pub fn attempts(&self) -> Vec<InjectionAttempt> {
        self.attempts.lock().unwrap().clone()
    }

    /// Get the number of injection attempts.
    pub fn attempt_count(&self) -> usize {
        self.attempts.lock().unwrap().len()
    }

    /// Check if any injection was attempted.
    pub fn was_injected(&self) -> bool {
        self.attempt_count() > 0
    }

    /// Get the last injection attempt.
    pub fn last_attempt(&self) -> Option<InjectionAttempt> {
        self.attempts.lock().unwrap().last().cloned()
    }

    /// Clear all recorded attempts.
    pub fn clear(&self) {
        self.attempts.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_injection_target_records_attempts() {
        let target = FakeInjectionTarget::new();
        assert!(!target.was_injected());

        target.inject("hello", InjectionMethod::ClipboardPaste, "sess-1", "utt-1");
        assert!(target.was_injected());
        assert_eq!(target.attempt_count(), 1);

        let last = target.last_attempt().unwrap();
        assert_eq!(last.text, "hello");
        assert_eq!(last.session_id, "sess-1");
        assert_eq!(last.utterance_id, "utt-1");
    }

    #[test]
    fn test_fake_injection_target_multiple_attempts() {
        let target = FakeInjectionTarget::new();
        target.inject("first", InjectionMethod::ClipboardPaste, "sess-1", "utt-1");
        target.inject("second", InjectionMethod::UnicodeTyping, "sess-1", "utt-2");

        assert_eq!(target.attempt_count(), 2);
        let attempts = target.attempts();
        assert_eq!(attempts[0].text, "first");
        assert_eq!(attempts[1].text, "second");
    }

    #[test]
    fn test_fake_injection_target_clear() {
        let target = FakeInjectionTarget::new();
        target.inject("test", InjectionMethod::ClipboardPaste, "sess-1", "utt-1");
        assert_eq!(target.attempt_count(), 1);

        target.clear();
        assert_eq!(target.attempt_count(), 0);
        assert!(!target.was_injected());
    }
}
