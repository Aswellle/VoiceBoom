// Utility functions for VoiceBoom AI

/// Format timestamp to readable time string
export function formatTime(timestamp: number): string {
  const date = new Date(timestamp);
  return date.toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
  });
}

/// Truncate text with ellipsis
export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength - 1) + '…';
}

/// Calculate total character count across segments
export function totalCharCount(segments: { text: string }[]): number {
  return segments.reduce((sum, s) => sum + s.text.length, 0);
}

/// Debounce function
export function debounce<T extends (...args: any[]) => void>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), delay);
  };
}

/// Detect if running in Tauri environment
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/// Get platform-specific default shortcut
export function getDefaultShortcut(): string {
  const platform = navigator.platform.toLowerCase();
  if (platform.includes('mac')) return 'Cmd+Space';
  return 'Ctrl+Space';
}
