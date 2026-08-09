// Platform-specific shortcut utilities
// Handles OS-specific behavior for global hotkeys

/// Get the default push-to-talk shortcut for the current platform
pub fn default_shortcut() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Cmd+Space"
    }
    #[cfg(target_os = "windows")]
    {
        "Ctrl+Space"
    }
    #[cfg(target_os = "linux")]
    {
        "Ctrl+Space"
    }
}

/// Get the alternative push-to-talk shortcut
pub fn alternative_shortcut() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Option+Space"
    }
    #[cfg(target_os = "windows")]
    {
        "Ctrl+Shift+V"
    }
    #[cfg(target_os = "linux")]
    {
        "Ctrl+Shift+V"
    }
}

/// Validate a shortcut string format
pub fn validate_shortcut(shortcut: &str) -> bool {
    // Basic validation: shortcut should contain at least one modifier and one key
    let parts: Vec<&str> = shortcut.split('+').collect();
    if parts.len() < 2 {
        return false;
    }
    let modifiers = ["Ctrl", "Alt", "Shift", "Cmd", "Option", "Super", "Meta"];
    let has_modifier = parts.iter().any(|p| modifiers.contains(&p.trim()));
    has_modifier
}
