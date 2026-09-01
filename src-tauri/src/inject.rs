//! Text injection — delivers transcribed text into the currently focused
//! input field (WeChat / iOS dictation style).
//!
//! Windows: uses [`win-text-inject`] for delayed-render clipboard injection.
//! Non-Windows: enigo clipboard+paste fallback.

use std::sync::LazyLock;
use std::sync::Mutex;
use enigo::Keyboard;

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

// ---------------------------------------------------------------------------
// Shared Enigo instance (used by Typing mode on all platforms, and as the
// clipboard-paste driver on non-Windows).
// ---------------------------------------------------------------------------
static ENIGO: LazyLock<Mutex<Option<enigo::Enigo>>> = LazyLock::new(|| {
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(e) => {
            log::info!("Enigo initialised for text injection");
            Mutex::new(Some(e))
        }
        Err(e) => {
            log::warn!("Enigo init failed: {e}");
            Mutex::new(None)
        }
    }
});

fn enigo_typing(text: &str) -> Result<(), String> {
    let mut guard = ENIGO.lock().map_err(|e| format!("{e}"))?;
    let enigo = guard
        .as_mut()
        .ok_or_else(|| "Enigo not available".to_string())?;
    enigo.text(text).map_err(|e| format!("{e}"))
}

// ---------------------------------------------------------------------------
// Windows — win-text-inject (delayed-render clipboard injection)
// ---------------------------------------------------------------------------
#[cfg(windows)]
pub fn inject(text: &str, mode: &InjectionMode) -> Result<(), String> {
    match mode {
        InjectionMode::Clipboard => windows_inject_via_clipboard(text),
        InjectionMode::Typing => enigo_typing(text),
    }
}

#[cfg(windows)]
fn windows_inject_via_clipboard(text: &str) -> Result<(), String> {
    use win_text_inject::{inject, Options, Target};

    let target = Target::foreground().map_err(|e| format!("{e}"))?;
    let outcome = inject(&target, text, Options::default()).map_err(|e| format!("{e}"))?;

    match outcome {
        win_text_inject::Outcome::Pasted { read_confirmed } => {
            log::info!(
                "win-text-inject: pasted {} chars (read_confirmed={})",
                text.len(),
                read_confirmed
            );
            Ok(())
        }
        win_text_inject::Outcome::Typed => {
            log::info!("win-text-inject: typed {} chars", text.len());
            Ok(())
        }
        win_text_inject::Outcome::ClipboardOnly(_) => {
            log::warn!("win-text-inject: blocked, text left on clipboard");
            Err("无法自动注入到当前窗口（可能是权限更高的程序），文字已复制到剪贴板，请手动粘贴".into())
        }
    }
}

// ---------------------------------------------------------------------------
// Non-Windows — enigo clipboard+paste fallback
// ---------------------------------------------------------------------------
#[cfg(not(windows))]
pub fn inject(text: &str, mode: &InjectionMode) -> Result<(), String> {
    match mode {
        InjectionMode::Clipboard => fallback_inject_via_clipboard(text),
        InjectionMode::Typing => enigo_typing(text),
    }
}

#[cfg(not(windows))]
fn fallback_inject_via_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Command;

    let prior = read_clipboard();

    write_clipboard(text)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    {
        let mut guard = ENIGO.lock().map_err(|e| format!("{e}"))?;
        let enigo = guard
            .as_mut()
            .ok_or_else(|| "Enigo not available".to_string())?;
        send_paste(enigo)?;
    }
    std::thread::sleep(std::time::Duration::from_millis(100));

    if let Some(p) = prior {
        if !p.is_empty() {
            write_clipboard(&p)?;
        }
    }
    Ok(())
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
