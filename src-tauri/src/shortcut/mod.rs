// Global shortcut manager for push-to-talk activation
// Uses tauri-plugin-global-shortcut v2 API

use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// Global shortcut manager for push-to-talk activation
pub struct GlobalShortcutManager {
    app_handle: AppHandle,
    current_shortcut: Option<String>,
}

impl GlobalShortcutManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            current_shortcut: None,
        }
    }

    /// Register a global shortcut for push-to-talk
    pub fn register(&mut self, shortcut: &str) -> anyhow::Result<()> {
        let gs = self.app_handle.global_shortcut();

        // Unregister previous shortcut if any
        if let Some(ref prev) = self.current_shortcut {
            if let Ok(sc) = prev.parse::<Shortcut>() {
                gs.unregister(sc)?;
            }
        }

        let sc: Shortcut = shortcut.parse()?;
        let _app_handle = self.app_handle.clone();
        let shortcut_owned = shortcut.to_string();

        gs.on_shortcut(sc, move |app, _shortcut, event| {
            use tauri_plugin_global_shortcut::ShortcutState;
            match event.state {
                ShortcutState::Pressed => {
                    let _ = app.emit("shortcut:pressed", &shortcut_owned);
                }
                ShortcutState::Released => {
                    let _ = app.emit("shortcut:released", &shortcut_owned);
                }
            }
        })?;

        self.current_shortcut = Some(shortcut.to_string());
        log::info!("Registered global shortcut: {}", shortcut);
        Ok(())
    }

    /// Unregister the current shortcut
    pub fn unregister(&mut self) -> anyhow::Result<()> {
        if let Some(ref shortcut) = self.current_shortcut {
            let gs = self.app_handle.global_shortcut();
            if let Ok(sc) = shortcut.parse::<Shortcut>() {
                gs.unregister(sc)?;
            }
            self.current_shortcut = None;
        }
        Ok(())
    }

    /// Check if a shortcut is registered
    pub fn is_registered(&self, shortcut: &str) -> bool {
        if let Some(ref current) = self.current_shortcut {
            return current == shortcut;
        }
        false
    }

    /// Get the currently registered shortcut
    pub fn current_shortcut(&self) -> Option<&str> {
        self.current_shortcut.as_deref()
    }
}
