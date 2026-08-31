# Repository Guidelines

## Project Overview

**VoiceBoom AI** — a real-time streaming voice input method (实时流式智能语音输入法) for Windows/macOS. Push a global hotkey → microphone capture → streaming ASR → text rendered in a glassmorphism floating window. Built with **Tauri 2.0 + Rust** (backend) and **React 19 + TypeScript** (frontend). Default engine is the bundled offline **sherpa-onnx SenseVoice** model, so it works out of the box with no API key.

---

## Development Commands

```bash
# Prerequisites: Rust 1.80+, Bun 1.2+ (preferred) or Node 20+, Tauri CLI 2.0+

bun install              # install frontend deps
bun run tauri:dev        # dev: hot-reload frontend + compile/launch Rust
bun run tauri:build      # production build → .msi/.exe (Win) / .dmg (macOS)
bun run dev              # frontend-only dev server (browser, no Tauri shell)
bun run build            # tsc -b && vite build (type-check + bundle)
bun run preview          # preview production frontend build
cd src-tauri && cargo check   # type-check Rust only (fast feedback)
```

> **Critical build rule:** Always produce release artifacts with `bun run tauri:build`. `cargo build --release` alone bypasses the Tauri CLI — it skips the frontend bundle and bakes in `devUrl`, producing an EXE that shows a white screen (`ERR_CONNECTION_REFUSED`). Use `cargo check` only to verify Rust compiles. Release output: `src-tauri/target/release/bundle/{msi,nsis}/`.

---

## Architecture & Data Flow

### Two-Layer Design

```
┌──────────────────────────────────────────────────────────────┐
│ Frontend (src/) — React 19 SPA, bundled by Vite               │
│ Renders into two Tauri webview windows                        │
│ Communicates via invoke() (commands) + listen() (events)      │
├──────────────────────────────────────────────────────────────┤
│ Backend (src-tauri/) — Rust + Tokio async runtime             │
│ Audio capture, ASR engines, SQLite, global shortcuts, tray     │
└──────────────────────────────────────────────────────────────┘
```

### Window Model (read carefully)

Two **separate webview windows**, distinguished by `label`, both declared in `tauri.conf.json`:

- **`floating`** — always-on-top, transparent, borderless glassmorphism window (600×120). The main transcription surface.
- **`settings`** — decorated window (860×640), `visible: false` at startup. Renders the settings panel.

Both windows load the same `index.html` and run the same `App.tsx`, which routes its subtree on `getCurrentWindow().label`. **The two windows are independent WebViews — Zustand state is NOT shared between them.** Cross-window sync is event-driven (`engine:switched` → `loadSettings`) plus SQLite persistence.

> **Do NOT create windows at runtime.** `open_settings` only `show()`/`set_focus()`es the pre-declared settings window. Runtime `WebviewWindowBuilder` creation crashes WebView2 on Windows when the transparent floating window is live. The settings window's `CloseRequested` is intercepted with `prevent_close()` + `hide()` so it persists.

### Recording Pipeline

```
Global hotkey (Rust) → emit "shortcut:pressed"
  → useGlobalShortcut hook → useAsr.startListening()
    → invoke "start_recording" {engine, language, apiKey, endpoint}
      → Rust: init ASR engine → CPAL audio capture (dedicated thread)
        → samples through tokio channel → bridge task
          → ASR engine.send_audio() → receive_result()
            → emit "asr:result" (partial/final)
              → useAsr hook → Zustand store → FloatingWindow re-renders
```

---

## Key Directories

| Path | Purpose |
|------|---------|
| `src/` | React frontend (components, hooks, stores, styles) |
| `src/components/FloatingWindow/` | Main transcription surface |
| `src/components/Settings/` | 7-tab settings panel (语音/AI 模型/本地资源/快捷键/显示/高级/关于) |
| `src/components/Waveform/` | Animated audio level equalizer |
| `src/hooks/useAsr.ts` | ASR lifecycle: start/stop + event subscriptions |
| `src/hooks/useGlobalShortcut.ts` | Push-to-talk via `shortcut:pressed/released` |
| `src/stores/useAppStore.ts` | Single Zustand store — all client state |
| `src/styles/index.css` | Tailwind directives + glassmorphism design tokens |
| `src-tauri/src/` | Rust backend |
| `src-tauri/src/commands/mod.rs` | All 13 `#[tauri::command]` handlers + RAII guards |
| `src-tauri/src/asr/` | `StreamingAsrEngine` trait + `AsrManager` + adapters |
| `src-tauri/src/asr/adapters/local.rs` | sherpa-onnx SenseVoice + Silero VAD (active local engine) |
| `src-tauri/src/asr/adapters/openai_whisper.rs` | OpenAI Realtime WebSocket |
| `src-tauri/src/asr/adapters/deepgram.rs` | Deepgram WebSocket |
| `src-tauri/src/audio/capture.rs` | CPAL mic capture + resampling → 16kHz mono f32 |
| `src-tauri/src/shortcut/` | Global hotkey manager + platform defaults |
| `src-tauri/src/tray/mod.rs` | System tray icon + menu |
| `src-tauri/src/resources/mod.rs` | ONNX model path resolution |
| `src-tauri/src/db/mod.rs` | SQLite (settings/history/shortcuts/model_config) |
| `src-tauri/asr-bundle/` | Bundled ONNX models shipped with installer |
| `models/` | Runtime GGUF/ggml model files (portable layout) |

---

## Frontend → Backend Communication

### Commands (`invoke`) — request/response

Defined in `src-tauri/src/commands/mod.rs`, registered in `lib.rs`:

| Command | Purpose |
|---------|---------|
| `start_recording` | Start capture + ASR pipeline, spawn bridge task |
| `stop_recording` | Stop audio; bridge flushes for final result |
| `get_settings` / `save_settings` | Read/write settings (persisted to SQLite) |
| `get_history` / `clear_history` | Recognition history |
| `register_shortcut` / `unregister_shortcut` | Global push-to-talk hotkey |
| `get_audio_devices` | List input devices |
| `open_settings` | Show/focus the pre-declared settings window |
| `get_resource_status` | Model package readiness (for UI) |
| `get_resource_endpoint` | Build sherpa-onnx endpoint string |
| `install_model` | Copy ONNX/txt/bin/gguf into models dir |
| `switch_engine` | Check model availability, emit `engine:switched` |

### Events (`listen` / `emit`) — backend pushes to frontend

| Event | Payload | When |
|-------|---------|------|
| `asr:result` | `{text, is_final, language, confidence}` | Partial or final recognition |
| `asr:heartbeat` | `{frames, samples}` | Every 500ms (diagnostic) |
| `asr:status` | string | Engine ready / model missing |
| `asr:error` | string | Model/init/flush failures |
| `audio:level` | number | Mic level for waveform |
| `shortcut:pressed` / `shortcut:released` | shortcut string | Hotkey state |
| `recording:started` / `recording:stopped` | — | Lifecycle |
| `engine:switched` | JSON result | After `switch_engine` |
| `tray:set-engine` / `tray:set-language` | string | Tray menu selection |
| `show-about` | — | Tray about clicked |

---

## ASR Engine Abstraction

All backends implement `StreamingAsrEngine` (`src-tauri/src/asr/engine_trait.rs`):

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

- **`AsrManager`** (`streaming.rs`) wraps the active engine in `Arc<Mutex<Box<dyn StreamingAsrEngine>>>`. It **reuses the resident local adapter** across recordings to avoid reloading the ~240MB SenseVoice model; cloud adapters are rebuilt per-recording.
- **Engine routing** (`parse_engine_type`): both `"whisper_cpp"` and `"funasr"` → local SenseVoice; `"openai_whisper"` / `"deepgram"` → cloud WebSocket.
- **Local endpoint string** is NOT a URL — it's three file paths joined by ASCII record separator `\x1E`: `"<vad_path>\x1E<model_path>\x1E<tokens_path>"`.
- **VAD lives inside the ASR adapter** (Silero VAD in `local.rs`), not the bridge task. The bridge only pushes samples in and forwards results out.

---

## Code Conventions & Common Patterns

- **Path alias:** `@/` → `src/` (configured in `tsconfig.json` + `vite.config.ts`).
- **Bug-fix markers:** Comments like `// m5 fix:`, `// C2 fix:`, `// M11 fix:` encode why code looks the way it does. Read them before modifying surrounding code.
- **UI language:** All user-facing strings are **Simplified Chinese**; code comments and identifiers are **English**.
- **State shape:** Single Zustand store (`useAppStore`) holds `status` (`'idle'|'listening'|'result'`), `segments[]`, `currentPartial`, `settings`, `audioLevel`, `toastMessage`. `updateSettings` auto-persists via `save_settings`; `loadSettings` reads `get_settings` once on mount.
- **Styling:** Tailwind v3 + glassmorphism token layer in `index.css` (`--glass-bg`, `--glass-blur: 30px`, `--glass-radius: 20px`, `.glass`/`.glass-dark` utilities). Framer Motion for animation, with `reduceMotion` escape hatch throughout.
- **No test suite exists.** No linter or formatter is configured.
- **File-based logging:** `lib.rs::init_file_logger` writes to `%TEMP%\voiceboom_debug.log` — the primary debugging channel for the release GUI app (stderr is invisible).
- **Plugin version matching:** When adding a Tauri plugin, the npm package and Rust crate must match on major.minor, and you must add the permission in `src-tauri/capabilities/default.json`.

---

## Important Files

| File | Role |
|------|------|
| `src/main.tsx` | React 19 bootstrap; wraps `App` in `ErrorBoundary` (M11 fix) |
| `src/App.tsx` | Root routing by window label; registers shortcut + tray listeners |
| `src/stores/useAppStore.ts` | Single source of truth for all client state |
| `src/hooks/useAsr.ts` | ASR lifecycle + event subscriptions |
| `src/hooks/useGlobalShortcut.ts` | Push-to-talk (callback-ref pattern, M9) |
| `src/components/FloatingWindow/index.tsx` | Main transcription surface (owns shared `useAsr` instance) |
| `src/components/Settings/index.tsx` | 7-tab settings; runs `switch_engine`, polls `get_resource_status` |
| `src-tauri/src/lib.rs` | `AppState`, command registration, setup, system tray |
| `src-tauri/src/commands/mod.rs` | All command handlers + `RecordingClaim` / `BridgeActiveGuard` RAII guards |
| `src-tauri/src/asr/engine_trait.rs` | `StreamingAsrEngine` trait, `AsrConfig`, `AsrResult` |
| `src-tauri/src/asr/streaming.rs` | `AsrManager` (engine lifecycle + reuse) |
| `src-tauri/src/asr/adapters/local.rs` | sherpa-onnx SenseVoice + Silero VAD (active local engine) |
| `src-tauri/tauri.conf.json` | Window definitions, bundle resources, CSP |
| `src-tauri/capabilities/default.json` | Tauri permissions |

---

## Runtime / Tooling Preferences

- **Runtime:** Bun (preferred) — `bun.lock` is the lockfile. Node also works.
- **Package manager:** Bun (`bun install`, `bun run`).
- **Frontend bundler:** Vite 6 with `@vitejs/plugin-react`.
- **Dev server:** `127.0.0.1:1420` (strict port, configured in `vite.config.ts`).
- **Rust toolchain:** edition 2021, async via Tokio 1.43 (full features).
- **Key Rust deps:** `tauri 2.2.5`, `sherpa-onnx 1.13 (static)`, `cpal 0.16`, `rusqlite 0.32 (bundled)`, `tokio-tungstenite 0.24`.
- **Windows release:** `main.rs` sets `windows_subsystem=windows` (no console window).

---

## Modification Safety Notes

- **RAII guards are load-bearing:** `RecordingClaim` (double-start guard via `AtomicBool`) and `BridgeActiveGuard` (clears `bridge_active` on task exit) prevent races. Don't bypass them.
- **Bridge task owns flush:** Never flush from `stop_recording` — it races the bridge's channel-close flush. `stop_recording` only stops audio capture.
- **Local adapter reuse:** Changing engine/endpoint/language triggers a rebuild; identical config reuses the resident model.
- **Model path resolution:** ResourceManager searches app-data dir → `asr-bundle/` → portable `models/` next to EXE. All 3 files (model, tokens, VAD) are required for readiness.
- **Global shortcut registration** is short-circuited with `if (isSettingsWindow) return` so the two windows don't fight over the same hotkey.
