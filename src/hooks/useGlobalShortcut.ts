// useGlobalShortcut hook — listens for global hotkey events from Rust
// Implements push-to-talk: hold to record, release to stop

import { useEffect, useCallback, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAsr } from './useAsr';

export function useGlobalShortcut() {
  const { startListening, stopListening } = useAsr();
  const isPressedRef = useRef(false);

  useEffect(() => {
    // Listen for shortcut press/release events from Rust
    const unlistenPressed = listen<string>('shortcut:pressed', () => {
      if (!isPressedRef.current) {
        isPressedRef.current = true;
        startListening();
      }
    });

    const unlistenReleased = listen<string>('shortcut:released', () => {
      if (isPressedRef.current) {
        isPressedRef.current = false;
        stopListening();
      }
    });

    return () => {
      unlistenPressed.then((f) => f());
      unlistenReleased.then((f) => f());
    };
  }, [startListening, stopListening]);

  return { isPressed: isPressedRef.current };
}
