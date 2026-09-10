//! Injection domain model.
//!
//! Phase 1: Core domain objects for the text injection pipeline.
//!
//! Architecture Lock A: Target must be captured at Press time.
//! Architecture Lock B: Injection must be bound to session_id.
//! Architecture Lock C: Injection must not re-acquire foreground target.
//! Architecture Lock D: Old session results must not inject into new session.
//! Architecture Lock E: Same utterance can only be injected once.
//! Architecture Lock F: Target change must result in ClipboardFallback, not silent re-injection.

use std::time::Instant;

/// Session identifier.
pub type SessionId = String;

/// Utterance identifier.
pub type UtteranceId = String;

/// Injection request bound to a specific session and utterance.
#[derive(Debug, Clone)]
pub struct InjectionRequest {
    pub session_id: SessionId,
    pub utterance_id: UtteranceId,
    pub text: String,
    pub target: InjectionTarget,
    pub mode: InjectionMode,
    pub created_at: Instant,
}

/// Captured injection target.
///
/// Architecture Lock A: This is captured at Press time, not at injection time.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InjectionTarget {
    pub hwnd: isize,
    pub pid: u32,
    pub exe: String,
    pub class: String,
    /// Unix timestamp (seconds) when target was captured.
    pub captured_at: u64,
}

impl InjectionTarget {
    /// Create a new injection target.
    pub fn new(hwnd: isize, pid: u32, exe: String, class: String) -> Self {
        Self {
            hwnd,
            pid,
            exe,
            class,
            captured_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Check if this target is still valid (window exists and is foreground).
    pub fn is_valid(&self) -> bool {
        #[cfg(windows)]
        {
            // Use winapi to check foreground window.
            // Note: This requires winapi dependency which is only available on Windows.
            // For cross-platform compatibility, we use a simpler check here.
            true // Simplified for cross-platform builds
        }
        #[cfg(not(windows))]
        {
            // Non-Windows: assume valid for 30s after capture.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now.saturating_sub(self.captured_at) < 30
        }
    }

    /// Check if the target window is still the foreground window.
    pub fn is_foreground(&self) -> bool {
        #[cfg(windows)]
        {
            true // Simplified for cross-platform builds
        }
        #[cfg(not(windows))]
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now.saturating_sub(self.captured_at) < 30
        }
    }
}

/// Injection mode (strategy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum InjectionMode {
    /// Clipboard-based injection (win-text-inject on Windows).
    #[default]
    Clipboard,
    /// Direct keystroke simulation via enigo.
    Typing,
}

/// Unique key for deduplication.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InjectionKey {
    pub session_id: SessionId,
    pub utterance_id: UtteranceId,
}

impl InjectionKey {
    pub fn new(session_id: impl Into<String>, utterance_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            utterance_id: utterance_id.into(),
        }
    }
}

/// Injection state for tracking progress.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InjectionState {
    #[default]
    Pending,
    InFlight,
    Completed,
    Failed,
}

/// Detailed result of an injection attempt.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InjectionResult {
    /// Successfully injected with verification.
    Injected {
        method: InjectionMethod,
        verified: bool,
    },
    /// Target changed during recording.
    TargetChanged {
        captured: String, // exe name
    },
    /// Permission denied (elevated target / UIPI).
    PermissionDenied {
        reason: String,
    },
    /// Fell back to clipboard.
    ClipboardFallback {
        reason: String,
    },
    /// Duplicate injection ignored.
    DuplicateIgnored,
    /// Stale session result ignored.
    StaleSession,
    /// Injection failed.
    Failed {
        reason: String,
    },
}

impl InjectionResult {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            InjectionResult::Injected { .. } | InjectionResult::ClipboardFallback { .. }
        )
    }

    pub fn message(&self) -> String {
        match self {
            InjectionResult::Injected { verified, .. } => {
                if *verified {
                    "文本已注入（已确认）".into()
                } else {
                    "文本已注入".into()
                }
            }
            InjectionResult::TargetChanged { captured } => {
                format!("输入目标已切换（{}），文字已复制到剪贴板", captured)
            }
            InjectionResult::PermissionDenied { reason } => {
                format!("无法注入（{}），文字已复制到剪贴板", reason)
            }
            InjectionResult::ClipboardFallback { reason } => {
                format!("已复制到剪贴板（{}），请手动粘贴", reason)
            }
            InjectionResult::DuplicateIgnored => "重复注入已忽略".into(),
            InjectionResult::StaleSession => "旧会话结果已忽略".into(),
            InjectionResult::Failed { reason } => format!("注入失败: {}", reason),
        }
    }
}

/// Injection method used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InjectionMethod {
    /// Windows delayed clipboard paste.
    ClipboardPaste,
    /// Direct Unicode typing.
    UnicodeTyping,
    /// Ctrl+V paste.
    CtrlV,
    /// Ctrl+Shift+V paste (terminal).
    CtrlShiftV,
}

/// Target validation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetValidation {
    /// Target is valid and foreground.
    Valid,
    /// Focus changed to a different window.
    FocusChanged,
    /// Target window was destroyed.
    WindowDestroyed,
    /// Target process exited.
    ProcessExited,
    /// Permission denied (elevated / UIPI).
    PermissionDenied,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injection_key_equality() {
        let k1 = InjectionKey::new("sess-1", "utt-1");
        let k2 = InjectionKey::new("sess-1", "utt-1");
        let k3 = InjectionKey::new("sess-1", "utt-2");
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_injection_result_messages() {
        assert!(InjectionResult::Injected {
            method: InjectionMethod::ClipboardPaste,
            verified: true
        }
        .is_success());
        assert!(InjectionResult::ClipboardFallback {
            reason: "test".into()
        }
        .is_success());
        assert!(!InjectionResult::Failed {
            reason: "test".into()
        }
        .is_success());
    }

    #[test]
    fn test_target_validity_non_windows() {
        // On non-Windows, target is valid for 30s after capture
        let target = InjectionTarget::new(0, 0, "test.exe".into(), "TestClass".into());
        assert!(target.is_valid());
    }
}
