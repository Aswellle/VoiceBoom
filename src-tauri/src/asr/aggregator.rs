//! Transcript Aggregator
//!
//! Architecture Lock F: 只有 utterance final 才允许自动注入。
//!
//! Resolves partial/final duplication, coverage, and injection issues:
//! - Partial → replace currentPartial (no accumulation)
//! - SegmentFinal → commit segment, clear currentPartial
//! - UtteranceFinal → commit utterance, mark injection-ready
//!
//! For providers that don't emit explicit utterance-final events,
//! the session finalizer produces a unified utterance-final.

use crate::asr::AsrEvent;

/// A committed text segment ready for display/injection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedSegment {
    pub text: String,
    pub is_final: bool,
}

/// The transcript aggregator maintains internal state to deduplicate
/// and order transcript events from any provider.
#[derive(Debug, Clone, Default)]
pub struct TranscriptAggregator {
    /// Text committed so far (segments that are stable).
    committed_text: String,
    /// Current partial text (may still change).
    current_partial: String,
    /// Whether an utterance-final has been produced.
    utterance_finalized: bool,
    /// Session ID for tracking.
    session_id: Option<String>,
}

/// Result of processing an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatorResult {
    /// Text to display (committed + current partial).
    pub display_text: String,
    /// Whether this event triggers injection.
    pub injection_ready: bool,
    /// The finalized utterance text (if injection_ready).
    pub injection_text: Option<String>,
}

impl TranscriptAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the aggregator for a new session.
    pub fn reset(&mut self, session_id: Option<String>) {
        self.committed_text.clear();
        self.current_partial.clear();
        self.utterance_finalized = false;
        self.session_id = session_id;
    }

    /// Get the current display text (committed + partial).
    pub fn display_text(&self) -> String {
        if self.current_partial.is_empty() {
            self.committed_text.clone()
        } else if self.committed_text.is_empty() {
            self.current_partial.clone()
        } else {
            format!("{} {}", self.committed_text, self.current_partial)
        }
    }

    /// Get the committed text only.
    pub fn committed_text(&self) -> &str {
        &self.committed_text
    }

    /// Get the current partial text.
    pub fn current_partial(&self) -> &str {
        &self.current_partial
    }

    /// Check if the utterance has been finalized.
    pub fn is_utterance_finalized(&self) -> bool {
        self.utterance_finalized
    }

    /// Process a provider event and return the result.
    pub fn process_event(&mut self, event: &AsrEvent) -> AggregatorResult {
        match event {
            AsrEvent::Partial { text, .. } => self.handle_partial(text),
            AsrEvent::SegmentFinal { text, .. } => self.handle_segment_final(text),
            AsrEvent::UtteranceFinal { text, .. } => self.handle_utterance_final(text),
            AsrEvent::Error { .. } => AggregatorResult {
                display_text: self.display_text(),
                injection_ready: false,
                injection_text: None,
            },
        }
    }

    /// Handle a partial event: replace current partial (no accumulation).
    fn handle_partial(&mut self, text: &str) -> AggregatorResult {
        self.current_partial = text.to_string();
        AggregatorResult {
            display_text: self.display_text(),
            injection_ready: false,
            injection_text: None,
        }
    }

    /// Handle a segment final: commit the segment, clear partial.
    fn handle_segment_final(&mut self, text: &str) -> AggregatorResult {
        // Commit: append segment to committed text.
        if self.committed_text.is_empty() {
            self.committed_text = text.to_string();
        } else {
            self.committed_text.push(' ');
            self.committed_text.push_str(text);
        }
        // Clear the partial (it's now committed).
        self.current_partial.clear();
        AggregatorResult {
            display_text: self.display_text(),
            injection_ready: false,
            injection_text: None,
        }
    }

    /// Handle an utterance final: commit and mark injection-ready.
    fn handle_utterance_final(&mut self, text: &str) -> AggregatorResult {
        // Commit the final text.
        if self.committed_text.is_empty() {
            self.committed_text = text.to_string();
        } else {
            self.committed_text.push(' ');
            self.committed_text.push_str(text);
        }
        self.current_partial.clear();
        self.utterance_finalized = true;

        let injection_text = self.committed_text.clone();
        AggregatorResult {
            display_text: injection_text.clone(),
            injection_ready: true,
            injection_text: Some(injection_text),
        }
    }

    /// Finalize the session: if no utterance-final was received,
    /// produce one from the current state.
    /// This handles providers that don't emit explicit utterance-final.
    pub fn finalize(&mut self) -> AggregatorResult {
        if self.utterance_finalized {
            // Already finalized — nothing to do.
            return AggregatorResult {
                display_text: self.display_text(),
                injection_ready: false,
                injection_text: None,
            };
        }

        // Promote current partial to committed.
        if !self.current_partial.is_empty() {
            if self.committed_text.is_empty() {
                self.committed_text = self.current_partial.clone();
            } else {
                self.committed_text.push(' ');
                self.committed_text.push_str(&self.current_partial);
            }
            self.current_partial.clear();
        }

        self.utterance_finalized = true;
        let injection_text = self.committed_text.clone();
        let injection_ready = !injection_text.is_empty();

        AggregatorResult {
            display_text: injection_text.clone(),
            injection_ready,
            injection_text: if injection_ready {
                Some(injection_text)
            } else {
                None
            },
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::AsrEvent;

    fn partial(text: &str) -> AsrEvent {
        AsrEvent::Partial {
            text: text.into(),
            language: None,
        }
    }

    fn segment_final(text: &str) -> AsrEvent {
        AsrEvent::SegmentFinal {
            text: text.into(),
            language: None,
            confidence: None,
        }
    }

    fn utterance_final(text: &str) -> AsrEvent {
        AsrEvent::UtteranceFinal {
            text: text.into(),
            language: None,
            confidence: None,
        }
    }

    #[test]
    fn test_partial_replaces_previous() {
        let mut agg = TranscriptAggregator::new();

        // A partial
        let result = agg.process_event(&partial("hello"));
        assert_eq!(result.display_text, "hello");
        assert!(!result.injection_ready);

        // A partial2 replaces (not appends)
        let result = agg.process_event(&partial("hello world"));
        assert_eq!(result.display_text, "hello world");
        assert!(!result.injection_ready);

        // Verify no duplication: "hello hello world" would be wrong
        assert_ne!(result.display_text, "hello hello world");
    }

    #[test]
    fn test_segment_final_commits_and_clears_partial() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&partial("hello"));
        let result = agg.process_event(&segment_final("hello"));

        assert_eq!(result.display_text, "hello");
        assert!(!result.injection_ready);
        assert_eq!(agg.committed_text(), "hello");
        assert_eq!(agg.current_partial(), "");
    }

    #[test]
    fn test_utterance_final_triggers_injection() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&partial("hello"));
        let result = agg.process_event(&utterance_final("hello world"));

        assert_eq!(result.display_text, "hello world");
        assert!(result.injection_ready);
        assert_eq!(result.injection_text, Some("hello world".into()));
    }

    #[test]
    fn test_no_duplicate_aa_ab_aabb() {
        // Gate 8: A partial, A partial2, A segment final, B partial, B final
        // Must NOT generate AA, AB, AABB etc.
        let mut agg = TranscriptAggregator::new();

        // A partial
        agg.process_event(&partial("A"));
        assert_eq!(agg.display_text(), "A");

        // A partial2 (replaces)
        agg.process_event(&partial("A B"));
        assert_eq!(agg.display_text(), "A B");

        // A segment final (commits "A B")
        agg.process_event(&segment_final("A B"));
        assert_eq!(agg.display_text(), "A B");
        assert_eq!(agg.committed_text(), "A B");

        // B partial
        agg.process_event(&partial("C"));
        assert_eq!(agg.display_text(), "A B C");

        // B final (commits "C")
        let result = agg.process_event(&utterance_final("C"));
        assert_eq!(result.display_text, "A B C");
        assert!(result.injection_ready);
        assert_eq!(result.injection_text, Some("A B C".into()));

        // Verify no duplication
        assert_ne!(agg.committed_text(), "A A B B C");
        assert_ne!(agg.committed_text(), "A B A B C");
    }

    #[test]
    fn test_finalize_produces_utterance_final() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&partial("hello"));
        agg.process_event(&segment_final("hello world"));

        // No utterance-final received — finalize should produce one.
        let result = agg.finalize();
        assert!(result.injection_ready);
        assert_eq!(result.injection_text, Some("hello world".into()));
    }

    #[test]
    fn test_finalize_already_finalized_is_noop() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&utterance_final("hello"));
        assert!(agg.is_utterance_finalized());

        // Second finalize should be a no-op.
        let result = agg.finalize();
        assert!(!result.injection_ready);
        assert_eq!(result.injection_text, None);
    }

    #[test]
    fn test_finalize_promotes_partial() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&partial("partial text"));
        let result = agg.finalize();

        assert!(result.injection_ready);
        assert_eq!(result.injection_text, Some("partial text".into()));
    }

    #[test]
    fn test_finalize_empty_is_not_injection_ready() {
        let mut agg = TranscriptAggregator::new();

        let result = agg.finalize();
        assert!(!result.injection_ready);
        assert_eq!(result.injection_text, None);
    }

    #[test]
    fn test_reset_clears_state() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&utterance_final("hello"));
        assert!(agg.is_utterance_finalized());

        agg.reset(Some("new-session".into()));
        assert!(!agg.is_utterance_finalized());
        assert_eq!(agg.committed_text(), "");
        assert_eq!(agg.current_partial(), "");
        assert_eq!(agg.display_text(), "");
    }

    #[test]
    fn test_multiple_segments_commit_in_order() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&segment_final("first"));
        agg.process_event(&segment_final("second"));
        agg.process_event(&segment_final("third"));

        assert_eq!(agg.committed_text(), "first second third");
    }

    #[test]
    fn test_partial_after_segment_final() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&segment_final("committed"));
        agg.process_event(&partial("new partial"));

        assert_eq!(agg.display_text(), "committed new partial");
    }

    #[test]
    fn test_error_does_not_affect_state() {
        let mut agg = TranscriptAggregator::new();

        agg.process_event(&partial("hello"));
        let result = agg.process_event(&AsrEvent::Error {
            code: "TEST_ERROR".into(),
            message: "test".into(),
            retryable: false,
        });

        assert_eq!(result.display_text, "hello");
        assert!(!result.injection_ready);
    }

    #[test]
    fn test_ordering_with_interleaved_events() {
        let mut agg = TranscriptAggregator::new();

        // Simulate realistic event stream
        agg.process_event(&partial("The"));
        agg.process_event(&partial("The quick"));
        agg.process_event(&segment_final("The quick brown"));
        agg.process_event(&partial("fox"));
        agg.process_event(&segment_final("fox jumps"));
        agg.process_event(&partial("over"));
        let result = agg.process_event(&utterance_final("over the lazy dog"));

        assert_eq!(result.display_text, "The quick brown fox jumps over the lazy dog");
        assert!(result.injection_ready);
    }
}
