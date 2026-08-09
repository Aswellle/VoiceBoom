// VoiceBoom AI — Main application component
// Renders the floating window and manages window routing

import { useEffect } from 'react';
import { FloatingWindow } from './components/FloatingWindow';
import { SettingsPanel } from './components/Settings';
import { useAppStore } from './stores/useAppStore';

/// Window label detection — Tauri assigns labels to windows
function getWindowLabel(): string {
  // In Tauri 2.0, the window label is available via window.__TAURI_INTERNALS__
  // For now, we detect based on URL or default to 'floating'
  if (typeof window !== 'undefined') {
    const params = new URLSearchParams(window.location.search);
    return params.get('window') || 'floating';
  }
  return 'floating';
}

export default function App() {
  const label = getWindowLabel();
  const isSettingsWindow = label === 'settings';

  // Apply theme to document
  const theme = useAppStore((s) => s.settings.theme);
  useEffect(() => {
    const root = document.documentElement;
    if (theme === 'dark') {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }
  }, [theme]);

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
