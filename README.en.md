# 🎙️ VoiceBoom

[![Release](https://img.shields.io/github/v/release/Aswellle/VoiceBoom)](https://github.com/Aswellle/VoiceBoom/releases/latest)
[![CI](https://github.com/Aswellle/VoiceBoom/actions/workflows/ci.yml/badge.svg)](https://github.com/Aswellle/VoiceBoom/actions/workflows/ci.yml)
[![Tauri](https://img.shields.io/badge/Tauri-2.11-9C27F0?logo=tauri)](https://v2.tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.88-EA5800?logo=rust)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react)](https://react.dev)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-0078D6)](https://github.com/Aswellle/VoiceBoom/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20%2B%20non--commercial-important)](./LICENSE)

**[简体中文](./README.md) | English**

**Real-time Streaming Voice Input Method**

> Like WeChat voice-to-text or iOS dictation: hold a hotkey, speak, release — text **appears directly in the focused input field**. No manual copy-paste.

A low-latency real-time voice-to-text input tool for Windows / macOS. Hold a global hotkey to activate the microphone, speak naturally, and watch your words appear instantly in a glassmorphism floating window — then **auto-inject into whatever field has focus**. The default engine is a bundled offline SenseVoice model, so it **works out of the box with no API key**.

---

## Download

Get installers from [Releases](https://github.com/Aswellle/VoiceBoom/releases/latest):

- **Windows** — `.msi` installer, or the portable ZIP (requires the WebView2 Runtime)
- **macOS** — Universal `.dmg` (Apple Silicon + Intel), plus an app `.zip`

Every release ships a `SHA256SUMS.txt` for verification.

| Platform | Requirement |
|---|---|
| Windows | Windows 10/11 + WebView2 Runtime |
| macOS | macOS 12 (Monterey) or newer |

---

## Core Features

- 🎯 **Direct Input Injection** — Transcribed text appears at the cursor position (WeChat/iOS dictation experience), not just in a floating window
- ⚡ **Real-time Streaming ASR** — Streaming ASR with real-time results (latency depends on engine and network)
- 🔌 **Pluggable ASR Engines** — Local offline SenseVoice (built-in) / OpenAI Realtime / Deepgram Streaming, with **Auto / Offline / Cloud** engine modes
- 🔁 **Cloud Auto-Fallback** — In Auto mode, prefer the local engine and fall back to the first configured cloud provider when it is unavailable
- 📦 **Model Management** — Multiple versions side by side, resumable downloads, SHA-256 verification, atomic install (an interrupted install cannot damage the working version)
- 🎨 **System HUD Style** — Low-presence design, three-state display (Idle / Listening / Result), adjustable density (compact / standard / expanded)
- ⌨️ **Global Hotkey** — Push-to-talk: hold to speak, release to stop; Ctrl / Alt / Shift / Cmd combinations supported
- 🎛️ **Input Policy** — Direct / Confirm / Recognize-only modes, configurable per scenario
- 🔒 **Safe Injection** — Windows delayed-rendering technology: no clipboard history leaks, no clipboard corruption, UIPI auto-fallback
- 🗂️ **History Panel** — Searchable recognition history with copy, select-all and clear
- 🌐 **Multi-language** — Chinese/English/Japanese/Korean auto-detection and switching
- 🧪 **Dual-layer Testing** — 149 Rust tests + 19 Vitest frontend tests
- 🪶 **Lightweight** — Tauri 2, Rust backend with no Node.js runtime dependency

---

## Tech Stack

| Module | Choice |
|------|------|
| Desktop Framework | Tauri 2.11 (Rust) |
| Frontend UI | React 19 + TypeScript + Tailwind CSS v3 |
| Animation | Framer Motion (with `prefers-reduced-motion` support) |
| State Management | Zustand |
| Audio Capture | CPAL (native rate + resample to 16 kHz mono f32) |
| Local ASR | sherpa-onnx 1.13.8 (SenseVoice + Silero VAD) |
| Cloud ASR | OpenAI Realtime / OpenAI Whisper / Deepgram (WebSocket) |
| Text Injection | win-text-inject (Windows) / enigo (cross-platform) |
| Database | SQLite (rusqlite, bundled) |
| Credential Storage | DPAPI (Windows) / 0600 file (macOS, Linux) |

> Versions are pinned by `rust-toolchain.toml` (Rust 1.88.0) and `config/sherpa-onnx.json` (sherpa-onnx 1.13.8, with SHA-256); builds run with `--locked` throughout.

---

## Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) **1.88.0** (pinned by `rust-toolchain.toml`; rustup installs it automatically)
- [Bun](https://bun.sh/) 1.2+ (preferred) or Node.js 20+
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) 2.11+
- Platform build dependencies: see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

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

> **About the sherpa-onnx native library**: the first build **downloads** the archive for your platform automatically. For offline or reproducible builds, pre-fetch and verify it instead (no network dependency):
>
> ```bash
> # Windows
> powershell -ExecutionPolicy Bypass -File scripts/prepare-sherpa-onnx.ps1
> # macOS / Linux / CI
> python3 scripts/fetch-sherpa-archive.py
> ```
>
> Both scripts pin the version from `config/sherpa-onnx.json` and verify its SHA-256. `bun run tauri:build:windows` already chains the Windows prefetch step.

---

## Testing

The project uses a two-layer testing strategy. Since the frontend relies heavily on Tauri APIs and cannot render in a pure browser, **unit/component tests run in jsdom with a mocked Tauri layer**, while **end-to-end tests drive a real desktop window via tauri-driver**.

### Unit & Component Tests (Vitest)

```bash
bun run test                # Run once (19 tests)
bun run test:watch          # Watch mode
bun run test:ui             # Web UI interface
```

Coverage (`src/test/`):
- `useAppStore` — Re-entrant guards, settings persistence and migration, maxChars budget trimming, toast timers
- `SegmentItem` — Rendering, accessibility semantics (role/aria-label), clipboard copy + textarea fallback
- `FloatingWindow` — Control rendering, recording state toggle, auto-resize by content, newest-first display

### Rust Tests

```bash
cargo test --locked         # 149 tests
cargo test --locked --lib   # Same (lib target only)
```

Coverage:
- **`asr/integration_tests.rs`** — Session lifecycle, Deepgram/OpenAI event-parsing contracts, flush and finalization, aggregator (empty / partial / multi-utterance / out-of-order / duplicate prevention)
- **`asr/failure_tests.rs`** — Failure injection and recovery: error releases resources, duplicate-start prevention, rapid 10× start-stop, audio device failure, network disconnect, 401/429, missing model, shortcut conflict, malformed responses
- **`asr/pipeline_tests.rs`** — Audio pipeline driven end to end by a fake audio source
- Inline tests — Injection controller and target validation, model verification/install, provider resolution and auto-fallback, distribution flavor detection, database operations

### End-to-End Tests (tauri-driver)

```bash
bun run tauri:build:test    # Build single-window test variant (floating window only)
bun run test:e2e            # Launch tauri-driver + msedgedriver, connect to real app
```

E2E covers: app launch, engine label, start/stop button, settings button.

> **Not covered** (requires real mic / system message loop / WebView2): global hotkey, actual recording, ASR transcription, desktop drag.

### CI

`.github/workflows/ci.yml` runs four jobs on every push and pull request:

| Job | Contents |
|---|---|
| Frontend | Type check + Vitest |
| Rust | `cargo fmt --check` + clippy (`-D warnings`) + tests + offline build check |
| macOS | `cargo check --all-targets` (compiles the macOS-only `cfg` branches) |
| Windows | clippy + tests + Tauri JS/Rust version alignment check + offline build check |

---

## Project Structure

```
VoiceBoom/
├── src/                          # React 19 frontend (TypeScript + Tailwind CSS)
│   ├── components/
│   │   ├── FloatingWindow/       # Floating window core (HUD three-state, newest-first, auto-resize)
│   │   ├── Settings/             # Settings panel (6 sections, incl. models and cloud providers)
│   │   │   ├── index.tsx
│   │   │   └── controls.tsx      # Shared form primitives (Slider/Select/TextInput)
│   │   ├── HistoryPanel/         # Recognition history (search, copy, clear)
│   │   └── Waveform/             # Audio waveform visualization (Canvas, 12 bars)
│   ├── hooks/
│   │   ├── useAsr.ts             # ASR recording lifecycle + result subscriptions
│   │   └── useGlobalShortcut.ts  # Global hotkey push-to-talk
│   ├── stores/
│   │   └── useAppStore.ts        # Zustand global state (settings/results/models/providers/UI)
│   ├── constants/
│   │   └── engines.ts            # Single source of truth for engine metadata
│   ├── utils/                    # clipboard and other utilities
│   ├── styles/                   # Global styles + semantic design tokens
│   ├── test/                     # Vitest suite + Tauri mocks
│   ├── App.tsx                   # Root routing (by window label) + tray/hotkey events
│   └── main.tsx                  # React 19 entry + ErrorBoundary
├── src-tauri/                    # Tauri 2 Rust backend
│   ├── src/
│   │   ├── asr/                  # ASR abstraction + adapters
│   │   │   ├── adapters/         # local(SenseVoice) / openai_realtime / openai_whisper / deepgram
│   │   │   ├── engine_trait.rs   # StreamingAsrEngine trait
│   │   │   ├── session.rs        # AsrSession trait + unified AsrEvent model
│   │   │   ├── streaming.rs      # AsrManager (resident local engine reuse)
│   │   │   ├── aggregator.rs     # TranscriptAggregator (only UtteranceFinal triggers injection)
│   │   │   └── *_tests.rs        # Integration / failure-injection / pipeline tests
│   │   ├── audio/                # CPAL capture + bounded real-time pipeline + fake source (tests)
│   │   ├── commands/             # 31 Tauri commands + session state machine
│   │   ├── models/               # Model management: registry, downloader, verifier, atomic installer
│   │   ├── provider/             # Cloud providers: config, credentials, registry, auto-fallback
│   │   ├── injection/            # Injection controller + platform adapters + fake target (tests)
│   │   ├── inject.rs             # Cross-platform text injection dispatch
│   │   ├── secure_keystore.rs    # Credential storage (DPAPI / 0600 file)
│   │   ├── session.rs            # Recording session state machine
│   │   ├── shortcut/             # Global hotkey + platform defaults
│   │   ├── db/                   # SQLite (settings/history/shortcuts + schema migrations)
│   │   ├── resources/            # Model path resolution + distribution flavor detection
│   │   ├── tray/                 # System tray menu
│   │   ├── lib.rs                # AppState + command registration + tray + file logger
│   │   └── main.rs               # Entry (windows_subsystem)
│   ├── vendor/                   # Vendored crate source (path dependencies)
│   │   ├── win-text-inject/      # Windows delayed-render clipboard injection
│   │   └── enigo/                # Cross-platform keystroke simulation
│   ├── capabilities/             # Tauri permissions
│   ├── tools/asr_debug.rs        # ASR debugging tool
│   ├── tauri.conf.json           # Window definitions + build config
│   ├── tauri.offline.conf.json   # Offline bundling config (bundles asr-bundle)
│   └── tauri.test.conf.json      # Single-window E2E test config
├── config/
│   └── sherpa-onnx.json          # Native library version, archive names and SHA-256 (single source)
├── models/
│   └── registry.json             # Model registry (embedded at compile time)
├── scripts/
│   ├── prepare-sherpa-onnx.ps1   # Windows: prefetch + verify the native archive
│   ├── fetch-sherpa-archive.py   # macOS/Linux/CI: same
│   ├── prepare-models.py         # CI: build model release archives
│   ├── verify-models.py          # CI: verify model archive integrity
│   ├── install-model-pack.py     # CI: install a model pack for offline builds
│   ├── merge-macos-universal.sh  # lipo-merge arm64 + x64 into a Universal bundle
│   └── e2e_smoke.mjs             # tauri-driver end-to-end smoke test
├── docs/                         # ARCHITECTURE / ASR / DEVELOPMENT / PERFORMANCE / SECURITY / TESTING
├── .github/workflows/            # ci.yml / release.yml / model-release.yml
├── AGENTS.md                     # Architecture and change constraints (for contributors)
├── rust-toolchain.toml           # Pins Rust 1.88.0
└── package.json                  # Dependencies + scripts
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

> Injection mode can be toggled in Settings (`injectionMode`: `clipboard` / `typing`). `InjectionController` handles deduplication, session binding and target validation; the target window is snapshotted before injection, so switching windows no longer injects into the wrong field.

---

## Version Roadmap

- **v0.4.x (current)** — Floating window HUD, local offline SenseVoice, global hotkey, text injection, input policy, model management and cloud provider auto-fallback, three-platform CI
- **v0.5** — Auto punctuation, filler removal, backtrack correction, personal dictionary, app profiles
- **v1.0** — AI polish modes, tone/style modes, translation
- **v2.0** — Voice workflow platform, meeting mode, plugin API

---

## License

MIT License with Commercial Use Restriction — see [LICENSE](./LICENSE).

This software adds a **commercial use restriction** to the MIT License: personal learning, research, and non-commercial use are free to use and distribute; **any commercial use (sales, licensing, embedding in commercial products, etc.) requires prior written permission from the author (wellerlee820@163.com)**.

---

*Built with ❤️ using Tauri, React, and Rust.*
