//! Recording Session State Machine
//!
//! Architecture Lock A: Recording Session 是录音生命周期唯一权威状态源。
//!
//! State flow:
//! ```
//! Idle ──(start)──→ Starting ──(init ok)──→ Recording ──(stop)──→ Stopping
//!                                                             │
//! Error ◄──(init fail)──┘                                     ▼
//!  │                                                    Finalizing
//!  │                                                         │
//!  └──(reset)──→ Idle ◄──(flush done)───────────────────────┘
//!                       ◄──(flush timeout)──→ Error
//! ```

use serde;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// The logical states of a recording session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    Idle,
    Starting,
    Recording,
    Stopping,
    Finalizing,
    Error,
}

impl std::fmt::Display for RecordingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Starting => write!(f, "starting"),
            Self::Recording => write!(f, "recording"),
            Self::Stopping => write!(f, "stopping"),
            Self::Finalizing => write!(f, "finalizing"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// A single recording session — from hotkey press to final text injection.
/// This is the single authoritative source of truth for recording state.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecordingSession {
    pub session_id: String,
    pub state: RecordingState,
    pub engine: String,
    pub language: String,
    /// Unix milliseconds when recording actually began.
    pub started_at: Option<u64>,
    /// Unix milliseconds when the session ended (success or failure).
    pub stopped_at: Option<u64>,
    /// Error message if state == Error.
    pub error: Option<String>,
}

impl RecordingSession {
    /// Create a fresh session in Idle state.
    pub fn new(session_id: String, engine: String, language: String) -> Self {
        Self {
            session_id,
            state: RecordingState::Idle,
            engine,
            language,
            started_at: None,
            stopped_at: None,
            error: None,
        }
    }

    /// Whether the session is currently in an active (non-idle, non-error) state.
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            RecordingState::Starting
                | RecordingState::Recording
                | RecordingState::Stopping
                | RecordingState::Finalizing
        )
    }

    /// Whether a new recording can be started from the current state.
    pub fn can_start(&self) -> bool {
        matches!(self.state, RecordingState::Idle | RecordingState::Error)
    }

    /// Whether the current recording can be stopped.
    pub fn can_stop(&self) -> bool {
        matches!(self.state, RecordingState::Recording)
    }

    // ── Validated transitions ──────────────────────────────────────────

    /// Idle/Error → Starting.
    pub fn begin_start(&mut self) -> Result<(), String> {
        if !self.can_start() {
            return Err(format!(
                "无法开始录音：当前状态为 '{}'",
                self.state
            ));
        }
        self.state = RecordingState::Starting;
        self.error = None;
        Ok(())
    }

    /// Starting → Recording. Records the start timestamp.
    pub fn mark_recording(&mut self) -> Result<(), String> {
        if self.state != RecordingState::Starting {
            return Err(format!(
                "无法进入录音状态：当前为 '{}'",
                self.state
            ));
        }
        self.state = RecordingState::Recording;
        self.started_at = Some(now_ms());
        Ok(())
    }

    /// Recording → Stopping.
    pub fn begin_stop(&mut self) -> Result<(), String> {
        if !self.can_stop() {
            return Err(format!(
                "无法停止录音：当前状态为 '{}'",
                self.state
            ));
        }
        self.state = RecordingState::Stopping;
        Ok(())
    }

    /// Stopping → Finalizing.
    pub fn mark_finalizing(&mut self) -> Result<(), String> {
        if self.state != RecordingState::Stopping {
            return Err(format!(
                "无法进入收尾状态：当前为 '{}'",
                self.state
            ));
        }
        self.state = RecordingState::Finalizing;
        Ok(())
    }

    /// Finalizing → Idle. Records the stop timestamp.
    pub fn complete(&mut self) -> Result<(), String> {
        if self.state != RecordingState::Finalizing {
            return Err(format!(
                "无法完成收尾：当前状态为 '{}'",
                self.state
            ));
        }
        self.state = RecordingState::Idle;
        self.stopped_at = Some(now_ms());
        Ok(())
    }

    /// Any → Error. Records the error and stop timestamp.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = RecordingState::Error;
        self.error = Some(error.into());
        self.stopped_at = Some(now_ms());
    }

    /// Error → Idle. Clears error state for reuse.
    pub fn reset(&mut self) {
        self.state = RecordingState::Idle;
        self.error = None;
    }
}

/// Thread-safe handle to the current recording session.
pub type SessionHandle = Arc<Mutex<RecordingSession>>;

/// Create a new session handle in Idle state.
pub fn new_session_handle(session_id: String, engine: String, language: String) -> SessionHandle {
    Arc::new(Mutex::new(RecordingSession::new(session_id, engine, language)))
}

/// Current unix timestamp in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session() -> RecordingSession {
        RecordingSession::new("test-123".into(), "funasr".into(), "auto".into())
    }

    #[test]
    fn test_idle_is_not_active() {
        let s = make_session();
        assert!(!s.is_active());
        assert!(s.can_start());
        assert!(!s.can_stop());
    }

    #[test]
    fn test_full_lifecycle() {
        let mut s = make_session();
        // Idle → Starting → Recording → Stopping → Finalizing → Idle
        s.begin_start().unwrap();
        assert_eq!(s.state, RecordingState::Starting);
        assert!(s.is_active());

        s.mark_recording().unwrap();
        assert_eq!(s.state, RecordingState::Recording);
        assert!(s.started_at.is_some());

        s.begin_stop().unwrap();
        assert_eq!(s.state, RecordingState::Stopping);

        s.mark_finalizing().unwrap();
        assert_eq!(s.state, RecordingState::Finalizing);

        s.complete().unwrap();
        assert_eq!(s.state, RecordingState::Idle);
        assert!(s.stopped_at.is_some());
    }

    #[test]
    fn test_duplicate_start_rejected() {
        let mut s = make_session();
        s.begin_start().unwrap();
        s.mark_recording().unwrap();
        // Cannot start while recording
        assert!(s.begin_start().is_err());
    }

    #[test]
    fn test_stop_only_from_recording() {
        let mut s = make_session();
        // Cannot stop from Idle
        assert!(s.begin_stop().is_err());
        s.begin_start().unwrap();
        // Cannot stop from Starting
        assert!(s.begin_stop().is_err());
    }

    #[test]
    fn test_start_failure_rolls_to_error() {
        let mut s = make_session();
        s.begin_start().unwrap();
        s.fail("ASR init failed");
        assert_eq!(s.state, RecordingState::Error);
        assert_eq!(s.error.as_deref(), Some("ASR init failed"));
        assert!(s.stopped_at.is_some());
        // Cannot stop from Error
        assert!(!s.can_stop());
        // Can restart from Error
        assert!(s.can_start());
    }

    #[test]
    fn test_audio_failure_rolls_to_error() {
        let mut s = make_session();
        s.begin_start().unwrap();
        s.mark_recording().unwrap();
        s.fail("Audio device lost");
        assert_eq!(s.state, RecordingState::Error);
        assert!(!s.is_active());
    }

    #[test]
    fn test_reset_from_error() {
        let mut s = make_session();
        s.begin_start().unwrap();
        s.fail("some error");
        assert_eq!(s.state, RecordingState::Error);
        s.reset();
        assert_eq!(s.state, RecordingState::Idle);
        assert!(s.error.is_none());
        assert!(s.can_start());
    }

    #[test]
    fn test_invalid_transitions_rejected() {
        let mut s = make_session();
        // Cannot mark_recording from Idle
        assert!(s.mark_recording().is_err());
        // Cannot begin_stop from Idle
        assert!(s.begin_stop().is_err());
        // Cannot complete from Idle
        assert!(s.complete().is_err());
        // Cannot mark_finalizing from Idle
        assert!(s.mark_finalizing().is_err());
    }

    #[test]
    fn test_start_stop_start_cycle() {
        let mut s = make_session();
        // First cycle
        s.begin_start().unwrap();
        s.mark_recording().unwrap();
        s.begin_stop().unwrap();
        s.mark_finalizing().unwrap();
        s.complete().unwrap();
        // Second cycle (reuse)
        assert!(s.can_start());
        s.begin_start().unwrap();
        s.mark_recording().unwrap();
        assert_eq!(s.state, RecordingState::Recording);
    }
}
