// useASR hook — manages ASR engine lifecycle and recognition flow
// Connects to Tauri backend for audio capture and streaming recognition

import { useEffect, useRef, useCallback } from 'react';
import { listen, emit } from '@tauri-apps/api/event';
import { useAppStore } from '../stores/useAppStore';

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
  const { setStatus, addSegment, updatePartial, settings } = useAppStore();
  const isListeningRef = useRef(false);
  const audioLevelInterval = useRef<ReturnType<typeof setInterval> | null>(null);

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
      } else {
        updatePartial(text);
      }
    });

    const unlistenLevel = listen<number>('audio:level', (event) => {
      useAppStore.getState().setAudioLevel(event.payload);
    });

    return () => {
      unlistenResult.then((f) => f());
      unlistenLevel.then((f) => f());
    };
  }, [addSegment, updatePartial]);

  // Start listening
  const startListening = useCallback(async () => {
    if (isListeningRef.current) return;
    isListeningRef.current = true;
    setStatus('listening');

    try {
      // Emit event to trigger Rust audio capture
      await emit('recording:start', {
        engine: settings.engine,
        language: settings.language,
        apiKey: settings.apiKey,
        endpoint: settings.endpoint,
      });

      // Simulate audio level updates (in production, this comes from Rust)
      audioLevelInterval.current = setInterval(() => {
        const level = Math.random() * 0.7 + 0.3;
        useAppStore.getState().setAudioLevel(level);
      }, 100);
    } catch (error) {
      console.error('Failed to start recording:', error);
      isListeningRef.current = false;
      setStatus('idle');
    }
  }, [setStatus, settings]);

  // Stop listening
  const stopListening = useCallback(async () => {
    if (!isListeningRef.current) return;
    isListeningRef.current = false;

    if (audioLevelInterval.current) {
      clearInterval(audioLevelInterval.current);
      audioLevelInterval.current = null;
    }

    setStatus('result');
    useAppStore.getState().setAudioLevel(0);

    try {
      await emit('recording:stop', {});
    } catch (error) {
      console.error('Failed to stop recording:', error);
    }

    // Return to idle after showing result
    setTimeout(() => {
      if (useAppStore.getState().status === 'result') {
        setStatus('idle');
      }
    }, 2000);
  }, [setStatus]);

  return {
    startListening,
    stopListening,
    isListening: isListeningRef.current,
  };
}
