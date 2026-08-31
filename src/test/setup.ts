/**
 * Vitest global setup — runs before every test file.
 *
 * Two problems must be solved for VoiceBoom's frontend to run under jsdom:
 *
 *  1. Tauri API (`@tauri-apps/api/core`, `webviewWindow`) is only present inside
 *     the Tauri desktop webview. In jsdom it is `undefined`, and our components
 *     / store call `invoke(...)` and `getCurrentWebviewWindow()` unconditionally,
 *     which throws. We therefore install deterministic mocks here so a test can
 *     focus on the behaviour it cares about instead of bootstrapping a whole
 *     desktop runtime.
 *
 *  2. jsdom lacks `window.matchMedia` and a real layout engine — both are touched
 *     by the glassmorphism theme effect and framer-motion. Minimal stubs keep
 *     rendering synchronous and error-free.
 *
 * Import testing matchers so every test has `expect(...).toBeInTheDocument()`
 * etc. without a per-file import.
 */
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

// ------------------------------------------------------------------
// 1. Tauri API mocks
// ------------------------------------------------------------------

/** In-memory key/value stand-in for the SQLite-backed get_settings/save_settings. */
const fakeStore = new Map<string, string>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_settings": {
        // Return only keys that have been previously saved.
        return { ...Object.fromEntries(fakeStore) };
      }
      case "save_settings": {
        const { key, value } = args as { key: string; value: string };
        fakeStore.set(key, value);
        return null;
      }
      case "switch_engine": {
        // Tests can override via `__setEngineResult` if they need a
        // specific readiness shape; default = local engine, fully installed.
        return (
          globalThis.__engineResult ?? {
            is_local: true,
            model_installed: true,
            tokens_installed: true,
            vad_installed: true,
          }
        );
      }
      default:
        return null;
    }
  }),
}));

// Stable singleton so test spies on setSize track the same fn the app calls.
export const fakeWebviewWindow = {
  setSize: vi.fn().mockResolvedValue(undefined),
  startDragging: vi.fn().mockResolvedValue(undefined),
};
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => fakeWebviewWindow),
  WebviewWindowLabel: "floating",
  WebviewWindow: vi.fn(() => fakeWebviewWindow),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => vi.fn()),
  emit: vi.fn(async () => undefined),
}));

// Expose a hook so individual tests can flip engine readiness at runtime.
declare global {
  // eslint-disable-next-line no-var
  var __engineResult: Record<string, boolean | undefined> | undefined;
  var __setEngineResult:
    | ((result: Record<string, boolean | undefined>) => void)
    | undefined;
}
globalThis.__setEngineResult = (result: Record<string, boolean | undefined>) => {
  globalThis.__engineResult = result;
};

// ------------------------------------------------------------------
// 2. jsdom environment stubs
// ------------------------------------------------------------------

// matchMedia — used by the auto-theme detection.
beforeEach(() => {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });

  // jsdom has no layout engine; scrollHeight/clientHeight are always 0 and
  // scrollTo is unimplemented. Stub both so the auto-resize + auto-scroll
  // effects run without throwing.
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
    configurable: true,
    value: 0,
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    value: 0,
  });
  Object.defineProperty(HTMLElement.prototype, "scrollTo", {
    configurable: true,
    value: vi.fn(),
  });

  // document.execCommand (clipboard fallback path) is absent from jsdom at
  // runtime even though the DOM lib types it. Install a stub when missing.
  if (typeof document.execCommand !== "function") {
    document.execCommand = (() => true) as typeof document.execCommand;
  }

  // Clear the fake SQLite store between tests for isolation.
  fakeStore.clear();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  globalThis.__engineResult = undefined;
});
