// VoiceBoom AI — Global state management with Zustand
// Manages recording state, recognition results, settings, and UI state

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { LogicalPosition } from '@tauri-apps/api/window';

/// Recognition result segment
export interface RecognitionSegment {
  id: string;
  text: string;
  isFinal: boolean;
  language?: string;
  confidence?: number;
  timestamp: number;
}

/// Application status
export type AppStatus = 'idle' | 'listening' | 'result';

/// ASR engine type
export type AsrEngineType = 'openai_whisper' | 'deepgram' | 'whisper_cpp' | 'funasr';

/// Application settings
export interface AppSettings {
  language: string;
  maxChars: number;
  engine: AsrEngineType;
  apiKey: string;
  endpoint: string;
  shortcut: string;
  fontSize: number;
  opacity: number;
  theme: 'auto' | 'light' | 'dark';
  reduceMotion: boolean;
  autoStart: boolean;
  vadSensitivity: number;
  /// Selected input device name. Empty string = system default.
  selectedDevice: string;
  /// How transcribed text is injected into the focused input field.
  injectionMode: 'clipboard' | 'typing';
}

const DEFAULT_SETTINGS: AppSettings = {
  language: 'auto',
  maxChars: 80,
  // Default to the bundled local engine, not a cloud API that needs a key —
  // the product promise is "works out of the box", so a first launch that
  // silently defaults to an unconfigured cloud engine breaks that promise.
  engine: 'funasr',
  apiKey: '',
  endpoint: '',
  shortcut: 'Ctrl+Space',
  fontSize: 22,
  opacity: 1,
  theme: 'auto',
  reduceMotion: false,
  autoStart: false,
  vadSensitivity: 50,
  // Empty string = use the system default audio input device.
  selectedDevice: '',
  // delivers text directly to the focused field like WeChat/iOS dictation,
  // without manual copy-paste.
  injectionMode: 'clipboard',
}

/// Application state interface
interface AppState {
  // Status
  status: AppStatus;
  setStatus: (status: AppStatus) => void;

  // Recognition results
  segments: RecognitionSegment[];
  currentPartial: string;
  addSegment: (segment: RecognitionSegment) => void;
  updatePartial: (text: string) => void;
  clearSegments: () => void;
  applyMaxChars: (maxChars: number) => void;

  // Settings
  settings: AppSettings;
  updateSettings: (partial: Partial<AppSettings>) => void;
  loadSettings: () => Promise<void>;
  resetSettings: () => void;

  // UI state
  isSettingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  windowPosition: { x: number; y: number };
  /// Whether the window position has been explicitly persisted. Avoids the
  /// false-negative where (0, 50) is mistaken for "never saved".
  windowPositionPersisted: boolean;
  setWindowPosition: (pos: { x: number; y: number }) => void;
  /// Persist the current window position to SQLite (debounced by the caller).
  persistWindowPosition: () => void;
  /// Restore the persisted window position to the Tauri webview window.
  /// Called once after loadSettings resolves.
  restoreWindowPosition: () => void;

  // Audio level (for waveform visualization)
  audioLevel: number;
  setAudioLevel: (level: number) => void;

  // Whether initial settings have been loaded from the database (avoids running the
  // engine-readiness check against defaults before loadSettings resolves).
  settingsLoaded: boolean;
  // Whether the global shortcut is currently registered (set by App.tsx).
  shortcutRegistered: boolean;
  setShortcutRegistered: (registered: boolean) => void;

  // Toast notification (m7 fix)
  toastMessage: string;
  showToast: (message: string) => void;



  /// Inject a finalized ASR transcript into the currently focused input field.
  /// Called by useAsr when a final result arrives.
  injectFinalText: (text: string) => void;

  // Recognition history (persisted in SQLite). Loaded on demand.
  // Toggle OS auto-start at boot (persists setting + registers with OS).
  setAutoStart: (enabled: boolean) => Promise<void>;

  history: HistoryRecord[];
  historyLoaded: boolean;
  loadHistory: () => Promise<void>;
  clearHistory: () => Promise<void>;
  isHistoryOpen: boolean;
  setHistoryOpen: (open: boolean) => void;
}


/// A single recognition history record, mirroring the SQLite `history` table.
export interface HistoryRecord {
  id: number;
  text: string;
  language: string | null;
  engine: string | null;
  confidence: number | null;
  created_at: number;
}


// m5 fix: Monotonic counter for unique IDs (avoids millisecond collision)
let idCounter = 0;
function generateId(): string {
  idCounter += 1;
  return Date.now().toString(36) + '-' + idCounter.toString(36);
}

/// Pending toast dismissal timer, so a new toast can cancel the previous one's
/// countdown rather than being cut short by it.
let toastTimer: ReturnType<typeof setTimeout> | null = null;

export const useAppStore = create<AppState>((set, get) => ({
  // Status
  status: 'idle',
  setStatus: (status) => set({ status }),

  // Recognition results
  segments: [],
  currentPartial: '',
  toastMessage: '',
  addSegment: (segment) => {
    set((state) => ({
      segments: [...state.segments, segment],
      currentPartial: segment.isFinal ? '' : state.currentPartial,
    }));
    // Apply max chars limit after adding
    get().applyMaxChars(get().settings.maxChars);
  },
  updatePartial: (text) => set({ currentPartial: text }),
  clearSegments: () => set({ segments: [], currentPartial: '' }),

  applyMaxChars: (maxChars) => {
    set((state) => {
      // Calculate total character count
      const allText = state.segments.map((s) => s.text).join('');
      if (allText.length <= maxChars) return {};

      // m6 fix: Remove oldest segments until under limit, but always keep at
      // the most recent segment so the transcription area never goes blank.
      const newSegments = [...state.segments];
      let currentLength = allText.length;
      while (currentLength > maxChars && newSegments.length > 1) {
        const removed = newSegments.shift();
        if (removed) currentLength -= removed.text.length;
      }
      return { segments: newSegments };
    });
  },

  // Settings
  settings: DEFAULT_SETTINGS,
  updateSettings: (partial) => {
    set((state) => ({ settings: { ...state.settings, ...partial } }));
    const newSettings = get().settings;
    // Persist each changed key. API keys bypass the generic settings table
    // and go to the dedicated model_config table instead.
    Object.entries(partial).forEach(([key, value]) => {
      if (key === 'apiKey') {
        invoke('save_api_key', { apiKey: String(value) }).catch((e) => {
          console.error(`Failed to save apiKey:`, e);
        });
      } else {
        invoke('save_settings', { key, value: String(value) }).catch((e) => {
          console.error(`Failed to save setting ${key}:`, e);
        });
      }
    });
    // Apply max chars if changed
    if (partial.maxChars !== undefined) {
      get().applyMaxChars(partial.maxChars);
    }
  },
  // M8 fix: guard against re-entrant calls. The engine:switched listener also
  // calls loadSettings; without this guard, loadSettings -> set(settings) ->
  // engine readiness effect -> switch_engine -> engine:switched -> loadSettings
  // loops forever (React error #185, "Maximum update depth exceeded").
  loadSettings: async () => {
    if (get().settingsLoaded) return;
    try {
      const stored = await invoke<Record<string, string>>('get_settings');
      const partial: Partial<AppSettings> = {};
      // windowPosition lives on the store root, not inside AppSettings, so it is
      // collected separately and applied with its own set() after the loop.
      let loadedPos: { x: number; y: number } | null = null;
      // #5 fix: track whether the position was explicitly persisted, instead of
      // inferring from coordinates (which collides with the real (0, 50) position).
      let hasPersistedPosition = false;
      for (const [key, value] of Object.entries(stored)) {
        switch (key) {
          case 'language': partial.language = value; break;
          case 'maxChars': partial.maxChars = parseInt(value, 10) || 80; break;
          case 'engine': partial.engine = value as AsrEngineType; break;
          // apiKey is intentionally NOT loaded from the generic settings
          // table — it lives in model_config (loaded below).
          case 'endpoint': partial.endpoint = value; break;
          case 'shortcut': partial.shortcut = value; break;
          case 'fontSize': partial.fontSize = parseInt(value, 10) || 22; break;
          case 'opacity': partial.opacity = parseFloat(value) || 1; break;
          case 'theme': partial.theme = value as 'auto' | 'light' | 'dark'; break;
          case 'reduceMotion': partial.reduceMotion = value === 'true'; break;
          case 'autoStart': partial.autoStart = value === 'true'; break;
          case 'vadSensitivity': partial.vadSensitivity = parseInt(value, 10) || 50; break;
          case 'selectedDevice': partial.selectedDevice = value; break;
          case 'windowPosX': {
            const x = parseInt(value, 10) || 0;
            loadedPos = { ...(loadedPos ?? { x: 0, y: 50 }), x };
            hasPersistedPosition = true;
            break;
          }
          case 'windowPosY': {
            const y = parseInt(value, 10) || 0;
            loadedPos = { ...(loadedPos ?? { x: 0, y: 50 }), y };
            hasPersistedPosition = true;
            break;
          }
        }
      }
      if (Object.keys(partial).length > 0) {
        set((state) => ({ settings: { ...state.settings, ...partial } }));
      }
      if (loadedPos) {
        set({ windowPosition: loadedPos });
      }
      // #4 fix: load API key from the dedicated model_config table BEFORE
      // flipping settingsLoaded, so the engine-ready effect sees the key.
      try {
        const storedKey = await invoke<string | null>('get_api_key');
        if (storedKey) {
          set((state) => ({ settings: { ...state.settings, apiKey: storedKey } }));
        }
      } catch {
        // Non-fatal: key simply stays empty.
      }
      set({ windowPositionPersisted: hasPersistedPosition });
    } catch (e) {
      console.error('Failed to load settings:', e);
      useAppStore.getState().showToast('加载设置失败，将使用默认配置');
    } finally {
      set({ settingsLoaded: true });
    }
  },
  resetSettings: () => set({ settings: DEFAULT_SETTINGS }),
  // UI state
  isSettingsOpen: false,
  setSettingsOpen: (open) => set({ isSettingsOpen: open }),
  windowPosition: { x: 0, y: 50 },
  windowPositionPersisted: false,
  setWindowPosition: (pos) => set({ windowPosition: pos }),
  persistWindowPosition: () => {
    const pos = get().windowPosition;
    // Fire-and-forget: position is non-critical; a failed write just means the
    // next launch falls back to the default position.
    invoke('save_settings', { key: 'windowPosX', value: String(pos.x) }).catch(() => {});
    invoke('save_settings', { key: 'windowPosY', value: String(pos.y) }).catch(() => {});
  },
  restoreWindowPosition: () => {
    // Only restore if the position was explicitly persisted — avoids the
    // false-negative where (0, 50) collides with a real saved position.
    if (!get().windowPositionPersisted) return;
    const pos = get().windowPosition;
    getCurrentWebviewWindow()
      .setPosition(new LogicalPosition(pos.x, pos.y))
      .catch(() => {});
  },
  // Audio level
  audioLevel: 0,
  setAudioLevel: (level) => set({ audioLevel: level }),

  settingsLoaded: false,
  shortcutRegistered: false,
  setShortcutRegistered: (registered) => set({ shortcutRegistered: registered }),

  // History
  history: [],
  historyLoaded: false,
  isHistoryOpen: false,
  setHistoryOpen: (open) => set({ isHistoryOpen: open }),
  loadHistory: async () => {
    try {
      const rows = await invoke<HistoryRecord[]>('get_history', { limit: 200 });
      set({ history: rows, historyLoaded: true });
    } catch (e) {
      console.error('Failed to load history:', e);
      get().showToast('加载历史记录失败');
      set({ historyLoaded: true });
    }
  },
  clearHistory: async () => {
    try {
      await invoke('clear_history');
      set({ history: [] });
      get().showToast('历史记录已清空');
    } catch (e) {
      console.error('Failed to clear history:', e);
      get().showToast('清空历史记录失败');
    }
  },
  injectFinalText: (text) => {
    const mode = get().settings.injectionMode;
    // Fire-and-forget: injection runs async; errors surface as toasts.
    invoke('inject_text', { text, mode })
      .then(() => {
        console.info(`injectFinalText: injected ${text.length} chars via '${mode}'`);
      })
      .catch((e) => {
        const msg = typeof e === 'string' ? e : '文本注入失败';
        console.error('injectFinalText failed:', msg);
        get().showToast(msg);
      });
  },
  showToast: (message) => {
    // Cancel any pending dismissal so a new toast gets its full duration
    // instead of inheriting the previous one's timer.
    if (toastTimer !== null) {
      clearTimeout(toastTimer);
      toastTimer = null;
    }
    set({ toastMessage: message });
    // Scale duration with message length — 2s was too short to finish reading
    // longer errors. ~60ms per character, clamped to 4-10s.
    const duration = Math.min(10000, Math.max(4000, message.length * 60));
    toastTimer = setTimeout(() => {
      set({ toastMessage: '' });
      toastTimer = null;
    }, duration);
  },
  /// Toggle OS-level auto-start at boot. Persists the setting and calls the
  /// backend to register/unregister the app with the OS.
  setAutoStart: async (enabled: boolean) => {
    const previous = get().settings.autoStart;
    // Optimistically update so the UI reflects the intent immediately.
    set((state) => ({ settings: { ...state.settings, autoStart: enabled } }));
    invoke('save_settings', { key: 'autoStart', value: String(enabled) }).catch(() => {});
    try {
      await invoke('set_auto_start', { enabled });
      get().showToast(enabled ? '已开启开机自启' : '已关闭开机自启');
    } catch (e) {
      // Roll back the optimistic update so the toggle matches OS state.
      set((state) => ({ settings: { ...state.settings, autoStart: previous } }));
      const msg = typeof e === 'string' ? e : '开机自启设置失败';
      get().showToast(msg);
    }
  },
}));
