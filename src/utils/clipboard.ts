// Clipboard helper — unified copy-to-clipboard with graceful fallback.
//
// In the Tauri production build the app loads via the `asset://` protocol, where
// `navigator.clipboard.writeText` is often unavailable. In that case we fall
// back to a hidden textarea + `document.execCommand('copy')`, which works in
// every environment. Callers decide how to report success/failure.

/**
 * Copy text to the clipboard.
 * @returns `true` if the copy succeeded, `false` otherwise.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Fall through to the textarea fallback below.
    }
  }

  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    // Keep the textarea invisible but part of the layout so `.select()` works.
    ta.style.cssText = 'position:fixed;opacity:0;pointer-events:none';
    document.body.appendChild(ta);
    ta.select();
    document.execCommand('copy');
    document.body.removeChild(ta);
    return true;
  } catch {
    return false;
  }
}
