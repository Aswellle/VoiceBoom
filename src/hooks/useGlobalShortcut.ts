// useGlobalShortcut hook — listens for global hotkey events from Rust
// Implements push-to-talk: hold to record, release to stop

import { useEffect, useCallback, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAsr } from './useAsr';

export function useGlobalShortcut() {
  const { startListening, stopListening } = useAsr();
  const isPressedRef = useRef(false);
  const [isPressed, setIsPressed] = useState(false);
  // Use refs for callbacks to avoid re-subscribing listeners on every settings change (M9)
  const startRef = useRef(startListening);
  const stopRef = useRef(stopListening);
  startRef.current = startListening;
  stopRef.current = stopListening;

  useEffect(() => {
    // Listen for shortcut press/release events from Rust
    const unlistenPressed = listen<string>('shortcut:pressed', () => {
      if (!isPressedRef.current) {
        isPressedRef.current = true;
        setIsPressed(true);
        startRef.current();
      }
    });

    const unlistenReleased = listen<string>('shortcut:released', () => {
      if (isPressedRef.current) {
        isPressedRef.current = false;
        setIsPressed(false);
        stopRef.current();
      }
    });

    return () => {
      unlistenPressed.then((f) => f());
      unlistenReleased.then((f) => f());
    };
  }, []); // Stable: uses refs for callbacks, no re-subscribe on settings change

  return { isPressed };
}
