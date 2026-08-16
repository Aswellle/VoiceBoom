// useGlobalShortcut hook — listens for global hotkey events from Rust
// Implements push-to-talk: hold to record, release to stop

import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';

/// Callbacks are injected by the caller (FloatingWindow) so the global-hotkey
/// path and the manual start/stop button share a single useAsr instance.
/// Otherwise each useAsr() call gets its own isListeningRef and the two paths
/// desync (stuck recording / swallowed hotkey).
export function useGlobalShortcut(
  startListening: () => Promise<void>,
  stopListening: () => Promise<void>,
) {
  const isPressedRef = useRef(false);
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
        startRef.current();
      }
    });

    const unlistenReleased = listen<string>('shortcut:released', () => {
      if (isPressedRef.current) {
        isPressedRef.current = false;
        stopRef.current();
      }
    });

    return () => {
      unlistenPressed.then((f) => f());
      unlistenReleased.then((f) => f());
    };
  }, []); // Stable: uses refs for callbacks, no re-subscribe on settings change
}
