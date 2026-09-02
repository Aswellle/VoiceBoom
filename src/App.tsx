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
  const setShortcutRegistered = useAppStore((s) => s.setShortcutRegistered);

  // Apply theme to document. When set to "auto", follow the OS preference
  // via prefers-color-scheme and live-update on change.
  useEffect(() => {
    const root = document.documentElement;
    const apply = (dark: boolean) => {
      if (dark) root.classList.add('dark');
      else root.classList.remove('dark');
    };
    if (settings.theme === 'auto') {
      const mq = window.matchMedia('(prefers-color-scheme: dark)');
      apply(mq.matches);
      const handler = (e: MediaQueryListEvent) => apply(e.matches);
      mq.addEventListener('change', handler);
      return () => mq.removeEventListener('change', handler);
    }
    apply(settings.theme === 'dark');
  }, [settings.theme]);

  // Load persisted settings on startup (all windows need the real settings)
  useEffect(() => {
    loadSettings();
  }, []); // Only run once on mount

  // Register global shortcut on startup — ONLY in the floating window.
  // The settings window runs the same App component; registering there would
  // fight over the same global hotkey with the floating window.
  useEffect(() => {
    if (isSettingsWindow) return;
    invoke('register_shortcut', { shortcut: settings.shortcut })
      .then(() => {
        setShortcutRegistered(true);
        console.log('[App] Shortcut registered:', settings.shortcut);
      })
      .catch((e) => {
        setShortcutRegistered(false);
        // Use alert for visibility in GUI app
        alert(`快捷键注册失败: ${e}\n请尝试使用其他快捷键组合`);
        console.error('[App] Failed to register shortcut:', e);
      });
  }, []); // Only run once on mount

  // Re-register shortcut when it changes in settings (floating window only)
  useEffect(() => {
    if (isSettingsWindow) return;
    invoke('register_shortcut', { shortcut: settings.shortcut })
      .then(() => {
        setShortcutRegistered(true);
        console.log('[App] Shortcut re-registered:', settings.shortcut);
      })
      .catch((e) => {
        setShortcutRegistered(false);
        alert(`快捷键更新失败: ${e}`);
        console.error('[App] Failed to update shortcut:', e);
      });
  }, [settings.shortcut, isSettingsWindow]);

  // Listen for tray menu events (engine/language changes)
  useEffect(() => {
    const unlistenEngine = listen<string>('tray:set-engine', (event) => {
      const engineId = event.payload;
      updateSettings({ engine: engineId as any });
      // Only the floating window drives the backend switch_engine check; the
      // settings window runs the same App component and would otherwise
      // double-invoke it (duplicate engine:switched + asr:status events).
      if (!isSettingsWindow) {
        invoke('switch_engine', { engine: engineId })
          .then((result) => {
            const status = result as any;
            if (status.is_local && status.status === 'ready') {
              useAppStore.getState().showToast('SenseVoice 本地引擎已就绪');
            } else if (status.is_local && status.status === 'model_missing') {
              useAppStore.getState().showToast('需要安装本地模型文件');
            }
          })
          .catch(() => {
            useAppStore.getState().showToast('引擎状态检查失败');
          });
      }
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
