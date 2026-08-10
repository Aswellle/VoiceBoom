// VoiceBoom AI — Main application component
// Renders the floating window and manages window routing

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { FloatingWindow } from './components/FloatingWindow';
import { SettingsPanel } from './components/Settings';
import { useAppStore } from './stores/useAppStore';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

/// Window label detection — uses Tauri 2.0 API
/// Falls back to URL parameter or 'floating' default
function getWindowLabel(): string {
  try {
    const label = getCurrentWindow().label;
    if (label) return label;
  } catch {
    // Fallback to URL parameter
    if (typeof window !== 'undefined') {
      const params = new URLSearchParams(window.location.search);
      const urlLabel = params.get('window');
      if (urlLabel) return urlLabel;
    }
  }
  return 'floating';
}

export default function App() {
  const label = getWindowLabel();
  const isSettingsWindow = label === 'settings';
  const settings = useAppStore((s) => s.settings);
  const loadSettings = useAppStore((s) => s.loadSettings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  // Apply theme to document
  useEffect(() => {
    const root = document.documentElement;
    if (settings.theme === 'dark') {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }
  }, [settings.theme]);

  // Register global shortcut on startup
  useEffect(() => {
    // Load persisted settings first
    loadSettings();

    // Register the push-to-talk shortcut
    invoke('register_shortcut', { shortcut: settings.shortcut }).catch((e) => {
      console.error('Failed to register shortcut:', e);
    });
  }, []); // Only run once on mount

  // Re-register shortcut when it changes in settings
  useEffect(() => {
    invoke('register_shortcut', { shortcut: settings.shortcut }).catch((e) => {
      console.error('Failed to update shortcut:', e);
    });
  }, [settings.shortcut]);

  // Listen for tray menu events (engine/language changes)
  useEffect(() => {
    const unlistenEngine = listen<string>('tray:set-engine', (event) => {
      const engineId = event.payload;
      updateSettings({ engine: engineId as any });
      // Automation: switch_engine auto-configures local servers
      invoke('switch_engine', { engine: engineId })
        .then((result) => {
          const status = result as any;
          if (status.is_local && status.status === 'server_started') {
            useAppStore.getState().showToast('本地服务器已自动启动');
          } else if (status.is_local && status.status === 'model_missing') {
            useAppStore.getState().showToast('需要安装本地模型文件');
          }
        })
        .catch(() => {});
    });
    const unlistenLanguage = listen<string>('tray:set-language', (event) => {
      updateSettings({ language: event.payload });
    });

    return () => {
      unlistenEngine.then((f) => f());
      unlistenLanguage.then((f) => f());
    };
  }, []);

  if (isSettingsWindow) {
    return (
      <div className="w-full h-full">
        <SettingsPanel />
      </div>
    );
  }

  return (
    <div className="w-full h-full flex items-center justify-center p-4 bg-transparent">
      <div className="w-full h-full max-w-[900px]">
        <FloatingWindow />
      </div>
    </div>
  );
}
