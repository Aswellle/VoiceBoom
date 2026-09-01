// useASR hook — manages ASR engine lifecycle and recognition flow
// Connects to Tauri backend for audio capture and streaming recognition

import { useEffect, useRef, useCallback, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
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
  const [isListening, setIsListening] = useState(false);
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

    return () => {
      unlistenResult.then((f) => f());
      unlistenError.then((f) => f());
      unlistenStatus.then((f) => f());
      unlistenLevel.then((f) => f());
    };
  }, [addSegment, updatePartial]);

  // Start listening
  const startListening = useCallback(async () => {
    if (isListeningRef.current) return;
    isListeningRef.current = true;
    setIsListening(true);
    setStatus('listening');

    try {
      // Invoke Tauri command to start recording with ASR config
      await invoke('start_recording', {
        engine: settings.engine,
        language: settings.language,
        apiKey: settings.apiKey,
        endpoint: settings.endpoint,
      });

      // Audio level comes from Rust via 'audio:level' events
    } catch (error) {
      console.error('Failed to start recording:', error);
      isListeningRef.current = false;
      setIsListening(false);
      setStatus('idle');
      // Show the error to the user — a silent failure looks like nothing happened
      useAppStore.getState().showToast(
        typeof error === 'string' ? error : '启动语音识别失败，请检查设置'
      );
    }
  }, [setStatus, settings]);

  // Stop listening
  const stopListening = useCallback(async () => {
    if (!isListeningRef.current) return;
    isListeningRef.current = false;
    setIsListening(false);

    if (audioLevelInterval.current) {
      clearInterval(audioLevelInterval.current);
      audioLevelInterval.current = null;
    }

    setStatus('result');
    useAppStore.getState().setAudioLevel(0);

    try {
      await invoke('stop_recording');
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
    isListening,
  };
}
