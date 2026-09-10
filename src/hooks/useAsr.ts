// useASR hook — manages ASR engine lifecycle and recognition flow
// Connects to Tauri backend for audio capture and streaming recognition

import { useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore, RecordingSessionState } from '../stores/useAppStore';

/// ASR event payload from Rust backend
interface AsrEvent {
  text: string;
  is_final: boolean;
  language?: string;
  confidence?: number;
}

/// Hook return type
interface UseAsrReturn {
  startListening: () => Promise<void>;
  stopListening: () => Promise<void>;
  isListening: boolean;
}

export function useAsr(): UseAsrReturn {
  const { addSegment, updatePartial, sessionState } = useAppStore();
  const engine = useAppStore((s) => s.settings.engine);
  const language = useAppStore((s) => s.settings.language);
  const apiKey = useAppStore((s) => s.settings.apiKey);
  const endpoint = useAppStore((s) => s.settings.endpoint);
  const selectedDevice = useAppStore((s) => s.settings.selectedDevice);
  const vadSensitivity = useAppStore((s) => s.settings.vadSensitivity);
  const audioLevelInterval = useRef<ReturnType<typeof setInterval> | null>(null);

  // Phase 13: Derive isListening from authoritative sessionState.
  // Do NOT maintain independent isListening state — backend is single source of truth.
  const isListening = sessionState === 'starting' || sessionState === 'recording' || sessionState === 'stopping' || sessionState === 'finalizing';

  // Listen for ASR results from Rust backend
  useEffect(() => {
    const unlistenResult = listen<AsrEvent>('asr:result', (event) => {
      const { text, is_final, language, confidence } = event.payload;
      if (is_final) {
        addSegment({
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 8),
          text,
          isFinal: true,
          language,
          confidence,
          timestamp: Date.now(),
        });
        // 语音转写完成后，自动注入到当前焦点输入框（如微信/iOS 听写）
        useAppStore.getState().injectFinalText(text);
      } else {
        updatePartial(text);
      }
    });

    // Listen for ASR errors
    const unlistenError = listen<string>('asr:error', (event) => {
      console.error('ASR Error:', event.payload);
      useAppStore.getState().showToast(event.payload);
    });

    // Listen for ASR status messages — show as a toast, not as recognition
    // partial text (status like "模型已就绪" was previously rendered into the
    // live transcription area as fake output).
    const unlistenStatus = listen<string>('asr:status', (event) => {
      useAppStore.getState().showToast(event.payload);
    });

    const unlistenLevel = listen<number>('audio:level', (event) => {
      useAppStore.getState().setAudioLevel(event.payload);
    });

    // Phase 13: Listen for timeout events from backend flush.
    const unlistenTimeout = listen<{ message: string }>('asr:timeout', (event) => {
      console.warn('ASR Timeout:', event.payload.message);
      useAppStore.getState().showToast(event.payload.message);
    });

    return () => {
      unlistenResult.then((f) => f());
      unlistenError.then((f) => f());
      unlistenStatus.then((f) => f());
      unlistenLevel.then((f) => f());
      unlistenTimeout.then((f) => f());
    };
  }, [addSegment, updatePartial]);

  // Start listening
  const startListening = useCallback(async () => {
    // Phase 13: Don't track isListening independently — backend will emit recording:state.
    try {
      // Invoke Tauri command to start recording with ASR config
      await invoke('start_recording', {
        engine,
        language,
        apiKey,
        endpoint,
        device: selectedDevice,
        vadSensitivity,
      });

      // Audio level comes from Rust via 'audio:level' events
    } catch (error) {
      console.error('Failed to start recording:', error);
      // Show the error to the user — a silent failure looks like nothing happened
      useAppStore.getState().showToast(
        typeof error === 'string' ? error : '启动语音识别失败，请检查设置'
      );
    }
  }, [engine, language, apiKey, endpoint, selectedDevice, vadSensitivity]);

  // Stop listening
  const stopListening = useCallback(async () => {
    // Phase 13: Don't track isListening independently — backend will emit recording:state.

    if (audioLevelInterval.current) {
      clearInterval(audioLevelInterval.current);
      audioLevelInterval.current = null;
    }

    useAppStore.getState().setAudioLevel(0);

    try {
      await invoke('stop_recording');
    } catch (error) {
      console.error('Failed to stop recording:', error);
    }
  }, []);

  return {
    startListening,
    stopListening,
    isListening,
  };
}
