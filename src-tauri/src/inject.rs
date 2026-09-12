//! Text injection — delivers transcribed text into the currently focused
//! input field (WeChat / iOS dictation style).
//!
//! Windows: uses [`win-text-inject`] for delayed-render clipboard injection.
//! Non-Windows: enigo clipboard+paste fallback.
//!
//! Phase 10: InjectionResult enum for detailed feedback, removal of fixed
//! sleeps on macOS, structured error reporting.

use std::io::Write;
use std::sync::LazyLock;
use std::sync::Mutex;

/// Injection strategies mirrored from the settings UI.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum InjectionMode {
    /// Clipboard-based injection (win-text-inject on Windows). Default.
    #[default]
    Clipboard,
    /// Direct keystroke simulation via enigo.
    Typing,
}

/// Detailed result of a text injection attempt.
/// Phase 10: structured feedback so the UI can show appropriate messages.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InjectionResult {
    Injected,
    ClipboardFallback,
    PermissionDenied,
    TargetUnavailable,
    Failed { reason: String },
}

impl InjectionResult {
    /// Whether this result represents a successful injection.
    pub fn is_success(&self) -> bool {
        matches!(self, InjectionResult::Injected | InjectionResult::ClipboardFallback)
    }

    /// Whether the user should be prompted to manually paste.
    pub fn needs_manual_paste(&self) -> bool {
        matches!(self, InjectionResult::ClipboardFallback)
    }

    /// User-facing message for this result.
    pub fn message(&self) -> String {
        match self {
            InjectionResult::Injected => "文本已注入".into(),
            InjectionResult::ClipboardFallback => {
                "无法确认注入结果，文字已复制到剪贴板，请手动粘贴".into()
            }
            InjectionResult::PermissionDenied => {
                "无法注入到当前窗口（权限不足），文字已复制到剪贴板，请手动粘贴".into()
            }
            InjectionResult::TargetUnavailable => {
                "没有找到可输入的焦点区域，文字已复制到剪贴板，请手动粘贴".into()
            }
            InjectionResult::Failed { reason } => format!("注入失败: {}", reason),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared Enigo instance (used by Typing mode on all platforms, and as the
// clipboard-paste driver on non-Windows).
//
// Note: enigo::Enigo is not Send on macOS (CoreGraphics handle), so we cannot
// use a `static`. Create on demand instead — Enigo construction is cheap.
// ---------------------------------------------------------------------------

fn new_enigo() -> Option<enigo::Enigo> {
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(e) => {
            log::info!("Enigo initialised for text injection");
            Some(e)
        }
        Err(e) => {
            log::warn!("Enigo init failed: {e}");
            None
        }
    }
}
fn enigo_typing(text: &str) -> InjectionResult {
    let mut enigo = match new_enigo() {
        Some(e) => e,
        None => {
            return InjectionResult::Failed {
                reason: "Enigo not available".into(),
            }
        }
    };
    match enigo.text(text) {
        Ok(()) => InjectionResult::Injected,
        Err(e) => InjectionResult::Failed {
            reason: format!("Keystroke simulation failed: {e}"),
        },
    }
}

// ---------------------------------------------------------------------------
// Windows — win-text-inject (delayed-render clipboard injection)
// ---------------------------------------------------------------------------
#[cfg(windows)]
pub fn inject(text: &str, mode: &InjectionMode) -> InjectionResult {
    match mode {
        InjectionMode::Clipboard => windows_inject_via_clipboard(text),
        InjectionMode::Typing => enigo_typing(text),
    }
}

#[cfg(windows)]
fn windows_inject_via_clipboard(text: &str) -> InjectionResult {
    use win_text_inject::{inject, Options, Target};

    let target = match Target::foreground() {
        Ok(t) => t,
        Err(e) => {
            return InjectionResult::TargetUnavailable;
        }
    };

    let outcome = match inject(&target, text, Options::default()) {
        Ok(o) => o,
        Err(e) => {
            return InjectionResult::Failed {
                reason: format!("win-text-inject error: {e}"),
            }
        }
    };

    match outcome {
        win_text_inject::Outcome::Pasted { read_confirmed } => {
            log::info!(
                "win-text-inject: pasted {} chars (read_confirmed={})",
                text.len(),
                read_confirmed
            );
            if read_confirmed {
                InjectionResult::Injected
            } else {
                // Injected but couldn't confirm (UIPI may have blocked readback).
                InjectionResult::ClipboardFallback
            }
        }
        win_text_inject::Outcome::Typed => {
            log::info!("win-text-inject: typed {} chars", text.len());
            InjectionResult::Injected
        }
        win_text_inject::Outcome::ClipboardOnly(_) => {
            log::warn!("win-text-inject: blocked, text left on clipboard");
            // Determine if this is a permission issue.
            InjectionResult::PermissionDenied
        }
    }
}

// ---------------------------------------------------------------------------
// Non-Windows — enigo clipboard+paste fallback
// ---------------------------------------------------------------------------
#[cfg(not(windows))]
pub fn inject(text: &str, mode: &InjectionMode) -> InjectionResult {
    match mode {
        InjectionMode::Clipboard => fallback_inject_via_clipboard(text),
        InjectionMode::Typing => enigo_typing(text),
    }
}

#[cfg(not(windows))]
fn fallback_inject_via_clipboard(text: &str) -> InjectionResult {
    use std::io::Write;
    use std::process::Command;

    // Save prior clipboard content for restoration.
    let prior = read_clipboard();

    // Write text to clipboard.
    if let Err(e) = write_clipboard(text) {
        return InjectionResult::Failed {
            reason: format!("Failed to write clipboard: {e}"),
        };
    }

    // Phase 10: Reduced fixed sleeps. Use shorter, more reliable timing.
    // The OS needs a brief moment to process the clipboard change.
    std::thread::sleep(std::time::Duration::from_millis(20));

    // Send paste shortcut.
    {
        let mut enigo = match new_enigo() {
            Some(e) => e,
            None => {
                return InjectionResult::Failed {
                    reason: "Enigo not available".into(),
                }
            }
        };
        if let Err(e) = send_paste(&mut enigo) {
            return InjectionResult::Failed {
                reason: format!("Paste shortcut failed: {e}"),
            };
        }
    }

    // Brief wait for paste to complete.
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Restore prior clipboard content.
    if let Some(p) = prior {
        if !p.is_empty() {
            let _ = write_clipboard(&p);
        }
    }

    InjectionResult::Injected
}

#[cfg(not(windows))]
fn send_paste(enigo: &mut enigo::Enigo) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let (modif, vkey) = (enigo::Key::Meta, enigo::Key::Other(9));
    #[cfg(target_os = "linux")]
    let (modif, vkey) = (enigo::Key::Control, enigo::Key::Unicode('v'));

    enigo.key(modif, enigo::Direction::Press).map_err(|e| format!("{e}"))?;
    enigo.key(vkey, enigo::Direction::Click).map_err(|e| format!("{e}"))?;
    enigo.key(modif, enigo::Direction::Release).map_err(|e| format!("{e}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn read_clipboard() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("pbpaste")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
    }
}

#[cfg(not(windows))]
fn write_clipboard(text: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("{e}"))?;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .map_err(|e| format!("{e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        let mut child = std::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("{e}"))?;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injection_result_is_success() {
        assert!(InjectionResult::Injected.is_success());
        assert!(InjectionResult::ClipboardFallback.is_success());
        assert!(!InjectionResult::PermissionDenied.is_success());
        assert!(!InjectionResult::TargetUnavailable.is_success());
        assert!(!InjectionResult::Failed { reason: "test".into() }.is_success());
    }

    #[test]
    fn test_injection_result_needs_manual_paste() {
        assert!(!InjectionResult::Injected.needs_manual_paste());
        assert!(InjectionResult::ClipboardFallback.needs_manual_paste());
        assert!(!InjectionResult::PermissionDenied.needs_manual_paste());
    }

    #[test]
    fn test_injection_result_messages() {
        assert_eq!(InjectionResult::Injected.message(), "文本已注入");
        assert!(
            InjectionResult::PermissionDenied
                .message()
                .contains("权限不足")
        );
        assert!(
            InjectionResult::TargetUnavailable
                .message()
                .contains("没有找到")
        );
        assert!(
            InjectionResult::Failed {
                reason: "test error".into()
            }
            .message()
            .contains("test error")
        );
    }

    #[test]
    fn test_injection_result_serialization() {
        let result = InjectionResult::Injected;
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, "\"injected\"");

        let result = InjectionResult::Failed {
            reason: "test".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("failed"));
    }
}
