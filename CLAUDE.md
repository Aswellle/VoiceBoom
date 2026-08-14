# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project Identity

**VoiceBoom AI** — a real-time streaming voice input method (实时流式智能语音输入法) for Windows / macOS. Push-to-talk global hotkey → microphone capture → streaming ASR → text displayed in a glassmorphism floating window. Built with **Tauri 2.0 + Rust** (backend) and **React 19 + TypeScript** (frontend).

---

## Commands

```bash
# Prerequisites: Rust 1.80+, Node.js 20+ or Bun 1.2+, Tauri CLI 2.0+

# Install frontend dependencies
bun install

# Development (hot-reload frontend + compiles Rust, launches app)
bun run tauri:dev

# Build production .exe / .msi / .dmg
bun run tauri:build

# Frontend-only dev server (browser, no Tauri shell)
bun run dev

# Type-check frontend (tsc --noEmit)
bun run build          # runs tsc -b && vite build; use for type-check + bundle

# Type-check Rust backend
cd src-tauri && cargo check

# Preview production frontend build
bun run preview
```

> **Note:** There is no test framework configured — no test files exist in the project yet.
>
> **Note:** Always build release artifacts with `bun run tauri:build`. `cargo build --release` (or `cargo check`) alone bypasses the Tauri CLI — it skips `beforeBuildCommand` (frontend never bundles) and bakes `devUrl` into the EXE, producing a build that tries to reach `127.0.0.1:1420` and shows `ERR_CONNECTION_REFUSED` (white screen). `cargo build --release` / `cargo check` are fine for verifying Rust compiles, but their EXE is **not distributable**. Release output lands in `src-tauri/target/release/bundle/{msi,nsis}/`.

---

## Architecture

### Two-Layer Design

```
┌─────────────────────────────────────────────────────────────┐
│  Frontend (src/) — React 19 SPA, bundled by Vite            │
│  Renders into Tauri webview windows                         │
│  Communicates via Tauri invoke() + listen() events          │
├─────────────────────────────────────────────────────────────┤
│  Backend (src-tauri/) — Rust + Tokio async runtime          │
│  Audio capture, ASR engines, SQLite, shortcuts, tray        │
└─────────────────────────────────────────────────────────────┘
```

### Window Model

The app uses **two separate Tauri webview windows**, distinguished by their `label`:

- **`floating`** — the always-on-top, transparent, borderless glassmorphism window. Defined in `tauri.conf.json`. This is the default window.
- **`settings`** — **pre-declared in `tauri.conf.json`** with `visible: false` (not created at runtime). The `open_settings` command only `show()`/`set_focus()`es the existing window. Reuses the same `index.html` but renders `<SettingsPanel>`.

The frontend routes on window label in `App.tsx` via `getCurrentWindow().label`. A `?window=` URL param fallback also exists.

> **Critical:** Do NOT switch `open_settings` back to runtime `WebviewWindowBuilder` creation. On Windows, creating a second WebView while the transparent `alwaysOnTop` floating window is live can fail to initialize and crash the whole WebView2 process (blank/unclosable window, floating window shortcuts/animation/toasts all die). New windows must be pre-declared in `tauri.conf.json`; commands only show/hide/focus. The settings window's `CloseRequested` is intercepted in `lib.rs` with `prevent_close()` + `hide()` so it persists and `get_webview_window("settings")` always resolves.

### Frontend → Backend Communication

Two mechanisms, both via `@tauri-apps/api`:

1. **Commands** (`invoke`) — request/response. Defined in `src-tauri/src/commands/mod.rs` and registered in `lib.rs`. Full set: `start_recording`, `stop_recording`, `get_settings`, `save_settings`, `get_history`, `clear_history`, `register_shortcut`, `unregister_shortcut`, `get_audio_devices`, `open_settings`, `get_resource_status`, `get_resource_endpoint`, `install_model`, `switch_engine`.
2. **Events** (`listen` / `emit`) — async push from Rust to React. Key event channels:
   - `asr:result` — recognition text (partial + final)
   - `asr:error`, `asr:status` — engine errors and status messages
   - `audio:level` — mic level for waveform visualization
   - `shortcut:pressed` / `shortcut:released` — push-to-talk hotkey state
   - `recording:started` / `recording:stopped`
   - `tray:set-engine` / `tray:set-language` — system tray menu actions

### Core Data Flow (Recording)

```
Global hotkey (Rust) → emit "shortcut:pressed"
  → useGlobalShortcut hook → useAsr.startListening()
    → invoke "start_recording" (engine, language, apiKey, endpoint)
      → Rust: init ASR engine → start CPAL audio capture (dedicated thread)
        → audio samples flow through tokio channel → bridge task
          → VAD processes frames → ASR engine.send_audio()
            → ASR results polled → emit "asr:result"
              → useAsr hook → Zustand store → FloatingWindow re-renders
```

### ASR Engine Abstraction

All ASR backends implement the `StreamingAsrEngine` trait (`src-tauri/src/asr/engine_trait.rs`):

```rust
#[async_trait]
pub trait StreamingAsrEngine: Send + Sync {
    async fn initialize(&mut self, config: AsrConfig) -> Result<()>;
    async fn send_audio(&mut self, audio_data: &[f32]) -> Result<()>;
    async fn receive_result(&mut self) -> Result<Option<AsrResult>>;
    async fn flush(&mut self) -> Result<Option<AsrResult>>;
    async fn close(&mut self) -> Result<()>;
    fn name(&self) -> &str;
    fn is_ready(&self) -> bool;
}
```

Adapters live in `src-tauri/src/asr/adapters/`:
- `openai_whisper.rs` — OpenAI Realtime WebSocket API
- `deepgram.rs` — Deepgram WebSocket API
- `local.rs` — in-process sherpa-onnx SenseVoice + Silero VAD
- `local_old_websocket.rs` — **orphaned/dead**; not listed in `adapters/mod.rs`, so it is not compiled. Leftover from the pre-sherpa-onnx server approach; ignore it.

`AsrManager` (`streaming.rs`) wraps the active engine in `Arc<Mutex<Box<dyn StreamingAsrEngine>>>` and is held in Tauri state.

### Local ASR (Offline Engine — sherpa-onnx SenseVoice)

The local engine is **in-process sherpa-onnx**, not an external server. It runs Alibaba's **SenseVoice** ONNX model for transcription and a **Silero VAD** ONNX model for endpoint detection, both statically linked via the `sherpa-onnx` crate (`features = ["static"]`).

- `src-tauri/src/asr/adapters/local.rs` (`LocalAsrAdapter`) owns the VAD, recognizer, and audio buffer. It mirrors the official `sense_voice_simulate_streaming_microphone.rs` example: feed audio into Silero VAD, interim-decode the accumulated buffer every ~0.2s while speech is active, final-decode each VAD-detected segment.
- `src-tauri/src/resources/mod.rs` (`ResourceManager`) only **resolves model file paths** — it does not spawn processes. Model files: `model.int8.onnx` (SenseVoice), `tokens.txt`, `silero_vad.onnx`.
- Model files live in `asr-bundle/` (bundled via `tauri.conf.json` `bundle.resources`). Resolution order: app-data `resources/<engine>/models/`, then the bundle (`<exe>/asr-bundle/` or `resources/asr-bundle/`), then a portable `models/` dir next to the EXE. `install_model` copies `.onnx`/`.bin`/`.gguf` files into the app-data models dir.
- The `start_recording` / `switch_engine` commands build the local engine's **endpoint string**, which is NOT a URL — it is three file paths joined by the ASCII record separator `\x1E`: `"<vad_path>\x1E<model_path>\x1E<tokens_path>"`. `LocalAsrAdapter` splits on `\x1E` and loads each.
- Engine-string mapping (`parse_engine_type` in `commands/mod.rs`): both `"whisper_cpp"` and `"funasr"` route to `LocalAsrAdapter` (SenseVoice). The frontend default engine is `"funasr"` (local), so the app works out of the box with no API key.

### State Management

Single Zustand store: `src/stores/useAppStore.ts`. Holds:
- `status` — `'idle' | 'listening' | 'result'`
- `segments[]` — finalized recognition results
- `currentPartial` — in-progress text
- `settings` — all user config (engine, language, apiKey, shortcut, theme, etc.)
- `audioLevel`, `windowPosition`, `isSettingsOpen`, `toastMessage`

Settings are **persisted to SQLite** on every change (`updateSettings` → `invoke('save_settings')`) and loaded on startup (`loadSettings` → `invoke('get_settings')`).

### Audio Capture

`src-tauri/src/audio/capture.rs` uses **CPAL** on a **dedicated OS thread** (cpal::Stream is not Send). Samples flow through a tokio unbounded channel to the ASR bridge task. Supports F32/I16/U16 sample formats, downmixes to mono, targets 16kHz.

### Voice Activity Detection

Endpoint detection lives **inside the ASR adapter**, not the bridge task. For the local engine, `LocalAsrAdapter` uses sherpa-onnx's **Silero VAD** (tuning constants in the `cfg` module of `local.rs`). The bridge task's only job is to push samples in and forward whatever the adapter reports out (partial vs final). `src-tauri/src/audio/vad.rs` still exists (an older energy-based VAD) but is no longer wired into the recording path — do not assume it drives endpoint detection.

### Database (SQLite via rusqlite)

`src-tauri/src/db/mod.rs` — tables: `settings`, `history`, `shortcuts`, `model_config`. Stored at `%APPDATA%\com.voiceboom.app\voiceboom.db`.

### System Tray

`src-tauri/src/tray/mod.rs` — tray icon with menu: show/hide window, ASR engine submenu, language submenu, settings, about, quit. Left-click toggles window, double-click opens settings.

---

## Key Conventions

- **Path alias:** `@/` maps to `src/` (configured in both `tsconfig.json` and `vite.config.ts`).
- **Bug-fix markers:** Comments like `// m5 fix:`, `// C2 fix:`, `// M11 fix:` mark specific bug fixes. Read them before modifying the surrounding code — they encode the reason the code looks the way it does.
- **UI language:** All user-facing strings are in **Simplified Chinese**; code comments and identifiers are in **English**.
- **No test suite exists.** If adding tests, you'll be establishing the first one.
- **Settings window is pre-declared, not dynamic.** Both windows (floating + settings) are declared in `tauri.conf.json`; `open_settings` only shows/focuses the settings window. Runtime `WebviewWindowBuilder` creation crashes WebView2 on Windows (see Window Model above).
- **The two windows are separate WebViews** — Zustand store state is **not shared** between them. Each window runs its own copy of `App.tsx`; a toast shown in one window is invisible in the other. Global-shortcut registration is short-circuited with `if (isSettingsWindow) return` so the two windows don't fight over the same hotkey.
- **File-based logging:** `lib.rs::init_file_logger` writes to `%TEMP%\voiceboom_debug.log`. In the released GUI app stderr is invisible, so this is the primary debugging channel — search there for runtime errors.
- **Plugin version matching:** When adding a Tauri plugin, the npm package and Rust crate must match on major.minor (else the Tauri CLI refuses to build with a version-mismatch error), and you must add the permission in `src-tauri/capabilities/default.json`.

---

## Tech Stack Summary

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri 2.0 (Rust) |
| Frontend | React 19 + TypeScript + Vite 6 |
| Styling | Tailwind CSS 3 + glassmorphism (backdrop-filter) |
| Animation | Framer Motion 11 |
| State | Zustand 5 |
| Audio capture | CPAL (Rust, dedicated thread) |
| ASR transport | WebSocket (tokio-tungstenite) |
| Database | SQLite (rusqlite, bundled) |
| Global shortcuts | tauri-plugin-global-shortcut |
| Settings persistence | tauri-plugin-store + custom SQLite |
| Packaging | Inno Setup (.msi/.exe for Windows, .dmg for macOS) |
