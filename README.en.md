# 🎙️ VoiceBoom AI

[![Tests](https://img.shields.io/badge/tests-143%20passing-brightgreen)](./src/test)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-9C27F0?logo=tauri)](https://v2.tauri.app)
[![Rust](https://img.shields.io/badge/Rust-180%2B-EA5800?logo=rust)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react)](https://react.dev)
[![License](https://img.shields.io/badge/license-MIT%20%2B%20non--commercial-important)](./LICENSE)

**[简体中文](./README.md) | English**

**Real-time Streaming Voice Input Method**

> Like WeChat voice-to-text or iOS dictation: hold a hotkey, speak, release — text **appears directly in the focused input field**. No manual copy-paste.

A low-latency real-time voice-to-text input tool for Windows / macOS. Hold a global hotkey to activate the microphone, speak naturally, and watch your words appear instantly in a glassmorphism floating window — then **auto-inject into whatever field has focus**.

---

## Core Features

- 🎯 **Direct Input Injection** — Transcribed text appears at the cursor position (WeChat/iOS dictation experience), not just in a floating window
- ⚡ **Real-time Streaming ASR** — Streaming ASR with real-time results (latency depends on engine and network)
- 🎨 **System HUD Style** — Low-presence design, three-state display (Idle/Listening/Result), newest content first
- 🔌 **Pluggable ASR Engines** — Local offline SenseVoice (built-in, works out of the box) / OpenAI Realtime / Deepgram Streaming
- ⌨️ **Global Hotkey** — Push-to-talk: hold to speak, release to stop
- 🔒 **Safe Injection** — Windows delayed-rendering technology: no clipboard history leaks, no clipboard corruption, UIPI auto-fallback
- 🌐 **Multi-language** — Chinese/English/Japanese/Korean auto-detection and switching
- 🧪 **Dual-layer Testing** — 124 Rust tests + 19 Vitest frontend tests
- 🪶 **Lightweight** — Tauri 2.0, Rust backend with no Node.js dependency
- 🎛️ **Flexible Input Policy** — Direct / Confirm / Recognize-only modes, configurable per scenario

---

## Tech Stack

| Module | Choice |
|------|------|
| Desktop Framework | Tauri 2.0 (Rust) |
| Frontend UI | React 19 + TypeScript + Tailwind CSS |
| Animation | Framer Motion |
| State Management | Zustand |
| Audio Capture | CPAL (Rust) |
| Local ASR | sherpa-onnx (SenseVoice + Silero VAD) |
| Cloud ASR | OpenAI Whisper / Deepgram (WebSocket) |
| Text Injection | win-text-inject (Windows) / enigo (cross-platform) |
| Database | SQLite (rusqlite) |

---

## Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.80+
- [Bun](https://bun.sh/) 1.2+ (preferred) or Node.js 20+
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) 2.0+

### Install Dependencies

```bash
bun install
```

### Development

```bash
bun run tauri:dev        # Full dev: frontend hot-reload + Rust compile/launch
bun run dev              # Frontend-only dev server (browser, no Tauri shell)
```

### Build Production

```bash
bun run tauri:build      # → src-tauri/target/release/bundle/{msi,nsis}/
```

---

## Testing

The project uses a two-layer testing strategy. Since the frontend relies heavily on Tauri APIs and cannot render in a pure browser, **unit/component tests run in jsdom with a mocked Tauri layer**, while **end-to-end tests drive a real desktop window via tauri-driver**.

### Unit & Component Tests (Vitest)

```bash
bun run test                # Run once (19 tests)
bun run test:watch          # Watch mode
bun run test:ui             # Web UI interface
bun run coverage            # Coverage report
```

Coverage:
- `useAppStore` — Re-entrant guards, settings persistence, maxChars budget trimming, toast timers, draft settings
- `SegmentItem` — Rendering, accessibility semantics (role/aria-label/keyboard), clipboard copy + textarea fallback
- `FloatingWindow` — Control rendering, recording state toggle, auto-resize by content, newest-first display

### Rust Unit Tests

```bash
cargo test --lib            # 124 tests
```

Coverage:
- Recording session state machine, audio pipeline, ASR adapters, aggregator, flush finalization
- Injection controller, InjectionResult, session-bound deduplication
- Transactional shortcut registration, settings persistence, SQLite operations

### End-to-End Tests (tauri-driver)

```bash
bun run tauri:build:test    # Build single-window test variant (floating window only)
bun run test:e2e            # Launch tauri-driver + msedgedriver, connect to real app
```

E2E covers: app launch, engine label, start/stop button, settings button.

> **Not covered** (requires real mic / system message loop / WebView2): global hotkey, actual recording, ASR transcription, desktop drag.

---

## Project Structure

```
VoiceBoom/
├── src/                        # React 19 frontend (TypeScript + Tailwind CSS)
│   ├── components/
│   │   ├── FloatingWindow/     # Floating window core (HUD three-state, newest-first, auto-resize)
│   │   ├── Settings/           # Settings panel (5 task-oriented sections)
│   │   └── Waveform/           # Audio waveform visualization (Canvas rendering)
│   ├── hooks/
│   │   ├── useAsr.ts           # ASR recording lifecycle + injection calls
│   │   └── useGlobalShortcut.ts # Global hotkey push-to-talk
│   ├── stores/
│   │   └── useAppStore.ts      # Zustand global state (settings/results/UI/injection)
│   ├── test/                   # Vitest test suite
│   │   ├── setup.ts            # Tauri API mock + jsdom patches
│   │   ├── store.test.ts       # Store logic tests
│   │   └── components.test.tsx # Component rendering & interaction tests
│   ├── utils/                  # Utility functions
│   ├── styles/                 # Global styles + semantic design tokens
│   ├── App.tsx                 # Root routing (by window label) + shortcut error banner
│   └── main.tsx                # React 19 entry + ErrorBoundary
├── src-tauri/                  # Tauri 2.0 Rust backend
│   ├── src/
│   │   ├── asr/                # ASR engine abstraction + adapters
│   │   │   ├── adapters/       # local(SenseVoice) / openai_whisper / deepgram
│   │   │   ├── engine_trait.rs # StreamingAsrEngine trait
│   │   │   └── streaming.rs    # AsrManager (engine reuse)
│   │   ├── audio/              # CPAL audio capture + resampling
│   │   ├── commands/           # Tauri command handlers (incl. inject_text)
│   │   ├── inject.rs           # Cross-platform text injection dispatch
│   │   ├── injection/          # Injection controller + adapters
│   │   ├── shortcut/           # Global hotkey (platform defaults)
│   │   ├── db/                 # SQLite (settings/history/shortcuts)
│   │   ├── resources/          # ONNX model path resolution
│   │   ├── tray/               # System tray
│   │   ├── lib.rs              # AppState + command registration + tray
│   │   └── main.rs             # Entry (windows_subsystem)
│   ├── vendor/                 # Vendored crate source (no external deps)
│   │   ├── win-text-inject/    # Windows delayed-render clipboard injection
│   │   └── enigo/              # Cross-platform keystroke simulation
│   ├── capabilities/           # Tauri permissions
│   ├── gen/schemas/            # Generated ACL schema
│   ├── icons/                  # App icons
│   ├── tools/                  # asr_debug debugging tool
│   ├── tauri.conf.json         # Window definitions + build config
│   └── tauri.test.conf.json    # Single-window E2E test config
├── scripts/
│   └── e2e_smoke.mjs           # tauri-driver end-to-end smoke test
├── docs/
│   └── DEVELOPMENT.md          # Development guide
├── public/                     # Static assets
├── index.html                  # Vite entry HTML
├── verify_asr_integration.sh   # ASR integration verification script
├── package.json                # Dependencies + scripts
├── vite.config.ts              # Vite + Vitest config
├── tailwind.config.js          # Tailwind config
└── tsconfig.json               # TypeScript config
```

---

## Text Injection Technology

After voice transcription completes, text is automatically injected into the currently focused input field. Technical implementation:

| Platform | Default Mode (Clipboard) | Fallback Mode (Typing) |
|---|---|---|
| **Windows** | `win-text-inject` delayed-render clipboard injection | `enigo` keystroke simulation |
| **macOS / Linux** | `enigo` clipboard+paste | `enigo` keystroke simulation |

### Why Windows Injection is Special

Naive approaches (save clipboard → overwrite → Ctrl+V → sleep → restore) have 4 structural flaws that `win-text-inject` systematically fixes:

1. **Clipboard History Leak** — Appends 4 opt-out formats to bypass Windows clipboard history/cloud clipboard
2. **Modifier Key Interference** — Releases all held modifier keys before injection to prevent Ctrl+V distortion
3. **UIPI Silent Failure** — Integrity level detection; on failure, text stays in clipboard with user notification
4. **Clipboard Restore Race** — Delayed rendering (`WM_RENDERFORMAT`), restore strictly ordered after target read, no constant delays

> Injection mode can be toggled in Settings (`injectionMode`: `clipboard` / `typing`).

---

## Version Roadmap

- **V1.0 (current)** — Floating window HUD + local offline ASR + global hotkey + text injection + dual-layer testing + flexible input policy
- **V1.1** — Auto punctuation, filler removal, backtrack correction, personal dictionary, app profiles
- **V1.5** — AI polish modes, tone/style modes, translation
- **V2.0** — Voice workflow platform, meeting mode, plugin API

---

## License

MIT License with Commercial Use Restriction — see [LICENSE](./LICENSE).

This software adds a **commercial use restriction** to the MIT License: personal learning, research, and non-commercial use are free to use and distribute; **any commercial use (sales, licensing, embedding in commercial products, etc.) requires prior written permission from the author (wellerlee820@163.com)**.

---

*Built with ❤️ using Tauri, React, and Rust.*
