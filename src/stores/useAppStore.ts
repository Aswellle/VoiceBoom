// VoiceBoom AI — Global state management with Zustand
// Manages recording state, recognition results, settings, and UI state

import { create } from 'zustand';

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
}

const DEFAULT_SETTINGS: AppSettings = {
  language: 'auto',
  maxChars: 80,
  engine: 'openai_whisper',
  apiKey: '',
  endpoint: '',
  shortcut: 'Ctrl+Space',
  fontSize: 22,
  opacity: 1,
  theme: 'auto',
  reduceMotion: false,
  autoStart: false,
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
  resetSettings: () => void;

  // UI state
  isSettingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  windowPosition: { x: number; y: number };
  setWindowPosition: (pos: { x: number; y: number }) => void;

  // Audio level (for waveform visualization)
  audioLevel: number;
  setAudioLevel: (level: number) => void;
}

/// Generate a simple unique ID (no external deps)
function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

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

      // Remove oldest segments until under limit
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
    // Apply max chars if changed
    if (partial.maxChars !== undefined) {
      get().applyMaxChars(partial.maxChars);
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
}));
