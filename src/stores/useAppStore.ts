// VoiceBoom AI — Global state management with Zustand
// Manages recording state, recognition results, settings, and UI state

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

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
};

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
  setWindowPosition: (pos: { x: number; y: number }) => void;

  // Audio level (for waveform visualization)
  audioLevel: number;
  setAudioLevel: (level: number) => void;

  // Whether initial settings have been loaded from the DB (avoids running the
  // engine-readiness check against defaults before loadSettings resolves).
  settingsLoaded: boolean;
  // Whether the global shortcut is currently registered (set by App.tsx).
  shortcutRegistered: boolean;
  setShortcutRegistered: (registered: boolean) => void;

  // Toast notification (m7 fix)
  toastMessage: string;
  showToast: (message: string) => void;
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

      // m6 fix: Remove oldest segments until under limit (allow removing all but keep at least 1)
      const newSegments = [...state.segments];
      let currentLength = allText.length;
      while (currentLength > maxChars && newSegments.length > 0) {
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
    // M3 fix: Persist settings to database
    const newSettings = get().settings;
    Object.entries(partial).forEach(([key, value]) => {
      invoke('save_settings', { key, value: String(value) }).catch((e) => {
        console.error(`Failed to save setting ${key}:`, e);
      });
    });
    // Apply max chars if changed
    if (partial.maxChars !== undefined) {
      get().applyMaxChars(partial.maxChars);
    }
  },
  // M3 fix: Load settings from database on startup
  loadSettings: async () => {
    try {
      const stored = await invoke<Record<string, string>>('get_settings');
      const partial: Partial<AppSettings> = {};
      for (const [key, value] of Object.entries(stored)) {
        switch (key) {
          case 'language': partial.language = value; break;
          case 'maxChars': partial.maxChars = parseInt(value, 10) || 80; break;
          case 'engine': partial.engine = value as AsrEngineType; break;
          case 'apiKey': partial.apiKey = value; break;
          case 'endpoint': partial.endpoint = value; break;
          case 'shortcut': partial.shortcut = value; break;
          case 'fontSize': partial.fontSize = parseInt(value, 10) || 22; break;
          case 'opacity': partial.opacity = parseFloat(value) || 1; break;
          case 'theme': partial.theme = value as 'auto' | 'light' | 'dark'; break;
          case 'reduceMotion': partial.reduceMotion = value === 'true'; break;
          case 'autoStart': partial.autoStart = value === 'true'; break;
          case 'vadSensitivity': partial.vadSensitivity = parseInt(value, 10) || 50; break;
        }
      }
      if (Object.keys(partial).length > 0) {
        set((state) => ({ settings: { ...state.settings, ...partial } }));
      }
    } catch (e) {
      console.error('Failed to load settings:', e);
    } finally {
      set({ settingsLoaded: true });
    }
  },
  resetSettings: () => set({ settings: DEFAULT_SETTINGS }),

  // UI state
  isSettingsOpen: false,
  setSettingsOpen: (open) => set({ isSettingsOpen: open }),
  windowPosition: { x: 0, y: 50 },
  setWindowPosition: (pos) => set({ windowPosition: pos }),

  // Audio level
  audioLevel: 0,
  setAudioLevel: (level) => set({ audioLevel: level }),

  settingsLoaded: false,
  shortcutRegistered: false,
  setShortcutRegistered: (registered) => set({ shortcutRegistered: registered }),

  // Toast notification (m7 fix)
  toastMessage: '',
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
}));
