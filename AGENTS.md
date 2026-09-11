# Repository Guidelines

## Project Overview

**VoiceBoom AI** — a real-time streaming voice input method (智能语音输入法) for Windows/macOS. Push a global hotkey → microphone capture → streaming ASR → text rendered in a glassmorphism floating window → **injected directly into the focused input field** (WeChat/iOS dictation style). Built with **Tauri 2.0 + Rust** (backend) and **React 19 + TypeScript** (frontend). Default engine is the bundled offline **sherpa-onnx SenseVoice** model (works out of the box, no API key).

---

## Architecture & Data Flow

### Recording Pipeline

```
[Global Hotkey Press]
       │
       ▼
tauri-plugin-global-shortcut  (src-tauri/src/shortcut/mod.rs)
       │  emits "shortcut:pressed" / "shortcut:released"
       ▼
useGlobalShortcut hook        (src/hooks/useGlobalShortcut.ts)
       │  startListening / stopListening (push-to-talk via isPressedRef)
       ▼
useAsr hook                   (src/hooks/useAsr.ts)
       │  invoke('start_recording', {engine, language, apiKey, endpoint})
       ▼
start_recording command       (src-tauri/src/commands/mod.rs)
       │
       ├─ parse_engine_type() → AsrEngineType enum
       ├─ ResourceManager resolves model paths; builds sherpa-onnx endpoint
       │   (format: "vad_path\x1Emodel_path\x1Etokens_path", \x1E = ASCII RS)
       ├─ AsrManager.initialize(config) → Box<dyn AsrSession>
       ├─ AudioCapture.start_recording() → bounded audio channel
       └─ spawn bridge task
              │
              ├─ audio_rx.recv() → asr.push_audio()
              ├─ asr.next_event() → TranscriptAggregator → emit 'asr:result'
              └─ on channel close: asr.finalize() → emit final 'asr:result'
                     │
                     ▼
              useAsr 'asr:result' listener (src/hooks/useAsr.ts)
                     │  is_final=true → addSegment + injectFinalText
                     ▼
              useAppStore.injectFinalText (src/stores/useAppStore.ts)
                     │  invoke('inject_text', {text, mode})
                     ▼
              inject_text command → src-tauri/src/inject.rs
                     ├─ Windows + Clipboard → win_text_inject::inject (delayed-render)
                     ├─ Non-Windows + Clipboard → enigo clipboard+paste fallback
                     └─ Typing (all platforms) → enigo keystroke simulation
```

### Window Model (read carefully)

Two **separate webview windows**, distinguished by `label`, both declared in `tauri.conf.json`:

- **`floating`** — always-on-top, transparent, borderless glassmorphism window (600×140). The main transcription surface.
- **`settings`** — decorated window (860×640), `visible: false` at startup. Renders the settings panel.

Both windows load the same `index.html` and run the same `App.tsx`, which routes its subtree on `getCurrentWindow().label`. **The two windows are independent WebViews — Zustand state is NOT shared between them.** Cross-window sync is event-driven (`engine:switched` → `loadSettings`) plus SQLite persistence.

> **Do NOT create windows at runtime.** `open_settings` only `show()`/`set_focus()`es the pre-declared settings window. Runtime `WebviewWindowBuilder` creation crashes WebView2 on Windows when the transparent floating window is live. The settings window's `CloseRequested` is intercepted with `prevent_close()` + `hide()` so it persists.

### ASR Engine Abstraction

Two traits layer the ASR system:

**`StreamingAsrEngine`** (`src-tauri/src/asr/engine_trait.rs`) — the legacy async trait:

```rust
#[async_trait]
pub trait StreamingAsrEngine: Send + Sync {
    async fn initialize(&mut self, config: AsrConfig) -> Result<()>;
    async fn send_audio(&mut self, audio_data: &[f32]) -> Result<()>;
    async fn receive_result(&mut self) -> Result<Option<AsrResult>>;
    async fn flush(&mut self) -> Result<Option<AsrResult>>;
    async fn close(&mut self) -> Result<()>;
    fn name(&self) -> &str;
    fn is_ready(&) -> bool;
}
```

**`AsrSession`** (`src-tauri/src/asr/session.rs`) — the normalized event-driven model (Architecture Lock D):

```rust
pub trait AsrSession: Send + Sync {
    async fn start(&mut self) -> Result<()>;
    async fn push_audio(&mut self, audio_data: &[f32]) -> Result<()>;
    async fn next_event(&mut self) -> Result<Option<AsrEvent>>;
    async fn finalize(&mut self) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}
```

`AsrEvent` enum: `Partial / SegmentFinal / UtteranceFinal / Error` — a unified model all adapters produce. `LegacySessionAdapter` wraps `StreamingAsrEngine` to implement `AsrSession`.

- **`AsrManager`** (`streaming.rs`) holds `Arc<Mutex<Box<dyn AsrSession>>>`. It **reuses the resident local adapter** across recordings to avoid reloading the ~240MB SenseVoice model; cloud adapters are rebuilt per-recording.
- **Engine routing** (`parse_engine_type`): both `"whisper_cpp"` and `"funasr"` → local SenseVoice; `"openai_whisper"` / `"deepgram"` → cloud WebSocket.
- **Local endpoint string** is NOT a URL — it's three file paths joined by ASCII record separator `\x1E`: `"<vad_path>\x1E<model_path>\x1E<tokens_path>"`.
- **VAD lives inside the ASR adapter** (Silero VAD in `local.rs`), not the bridge task. The bridge only pushes samples in and forwards events out.
- **TranscriptAggregator** (`aggregator.rs`, Architecture Lock F): only `UtteranceFinal` triggers injection. `Partial` replaces current, `SegmentFinal` commits, `UtteranceFinal` commits + marks `injection_ready`. `finalize()` promotes partial for providers lacking utterance-final.
- **LatencyTracker** (`latency.rs`): 8 pipeline timestamps (t0–t7), P50/P90/P95/P99 for capture→partial/final/injection. Instrumentation only — not yet wired into AppState.

### Cloud Provider Registry (Phase 3)

Unified registry for cloud ASR providers in `src-tauri/src/provider/`:

- **`ProviderId`** (`config.rs`): `LocalSenseVoice`, `OpenAIRealtime`, `DeepgramStreaming`, `OpenAIWhisper`, `CustomOpenAICompatible` (+ legacy aliases). Each has default endpoint + model.
- **`ProviderMode`** (`config.rs`): `Automatic` (prefer local, fall back to cloud), `Offline` (local only), `Cloud` (specific provider).
- **`ProviderConfig`** (`config.rs`): per-provider endpoint, model, `credential_ref`, enabled flag. Persisted to SQLite `settings` table.
- **`ProviderCredentialStore`** (`credential.rs`): wraps `secure_keystore` — API keys stored in OS secure storage (DPAPI/Keychain/0600), NEVER plaintext in SQLite. SQLite only holds the `credential_ref`.
- **`ProviderRegistry`** (`registry.rs`): `resolve()` implements auto-fallback — Automatic mode prefers local when available, falls back to first configured cloud provider.

The frontend exposes 3 modes to users: **Automatic** / **Local** / **Cloud** (Advanced for per-provider config).

### Offline Release (Phase 4)

- **`DistributionFlavor`** (`resources/mod.rs`): `Standard` / `Offline` / `Portable` enum. Controls first-run behavior, model provisioning, UI wording. Detected at runtime from bundled/portable resource presence.
- **`tauri.offline.conf.json`**: merges `resources/asr-bundle/**` into `bundle.resources` via `bun tauri build --config`. The offline build bundles SenseVoice + VAD into the installer.
- **Offline build is NOT built every release** (per spec). Triggered on-demand via `workflow_dispatch` with `build_offline: true`. The `build-offline` job downloads the model artifact, installs to `src-tauri/resources/asr-bundle/`, then builds with the offline config.

### Audio Pipeline

- **Capture** (`audio/capture.rs`): CPAL dedicated thread, native sample rate + linear resample to 16kHz mono f32, bounded channel, startup confirmation channel, `stop_recording` drops the `audio_tx` clone to close the channel.
- **Pipeline** (`audio/pipeline.rs`, Architecture Lock C): `AudioFrame { sequence, timestamp, samples }`, capacity 8 frames (~500ms at 64ms/frame). **Drops newest when full** (real-time priority over completeness).

### Text Injection System

Cross-platform text injection in `src-tauri/src/inject.rs` + `src-tauri/src/injection/` (`InjectionController` with 6 Architecture Locks). **Vendored crates** in `src-tauri/vendor/` (no external crate references — path dependencies only):

| Platform | Default (Clipboard mode) | Fallback (Typing mode) |
|---|---|---|
| **Windows** | `win-text-inject` — delayed-render clipboard injection (fixes: clipboard-history privacy, held-modifier corruption, UIPI silent failure, clipboard-restore race) | `enigo` keystroke simulation |
| **macOS/Linux** | `enigo` clipboard+paste (best-effort) | `enigo` keystroke simulation |

The frontend picks the strategy from `settings.injectionMode` (`"clipboard"` default / `"typing"`).

---

## Key Directories

| Path | Purpose |
|---|---|
| `src/` | React frontend (components, hooks, stores, styles) |
| `src/components/FloatingWindow/` | Main transcription surface (owns shared `useAsr` instance) |
| `src/components/Settings/` | 5-tab settings panel (基本/AI/外观/个性化/高级) |
| `src/components/Waveform/` | Canvas-based audio level equalizer (12 bars, ~30fps) |
| `src/components/HistoryPanel/` | History overlay (search, copy, select-all, clear with confirm) |
| `src/components/Animation/`, `src/components/Shared/` | Shared UI primitives |
| `src/hooks/useAsr.ts` | ASR lifecycle: start/stop + event subscriptions |
| `src/hooks/useGlobalShortcut.ts` | Push-to-talk via `shortcut:pressed/released` |
| `src/stores/useAppStore.ts` | Single Zustand store — all client state |
| `src/constants/engines.ts` | `ENGINES` array — single source of truth for engine list |
| `src/utils/` | `clipboard.ts` (copy with textarea fallback), `debounce`, `isTauri()` |
| `src/styles/index.css` | Tailwind directives + glassmorphism design tokens |
| `src/test/` | Vitest tests + setup (Tauri API mocks) |
| `src-tauri/src/` | Rust backend |
| `src-tauri/src/commands/mod.rs` | All 26 `#[tauri::command]` handlers + session state machine |
| `src-tauri/src/asr/` | `StreamingAsrEngine` + `AsrSession` traits, `AsrManager`, adapters, aggregator, latency |
| `src-tauri/src/provider/` | `ProviderRegistry`, `ProviderConfig`, `ProviderCredentialStore` (Phase 3) |
| `src-tauri/src/audio/capture.rs` + `pipeline.rs` | CPAL capture + bounded real-time pipeline |
| `src-tauri/src/shortcut/` | Global hotkey manager + platform defaults |
| `src-tauri/src/tray/` | System tray icon + menu |
| `src-tauri/src/resources/` | ONNX model path resolution |
| `src-tauri/src/db/` | SQLite (settings/history/shortcuts/model_config) |
| `src-tauri/src/secure_keystore.rs` | API key storage (DPAPI / Keychain / File 0600) |
| `src-tauri/src/models/` | `ModelManager`, `ModelDownloader`, `ModelVerifier`, `ModelInstaller` (Phase 2) |
| `src-tauri/vendor/` | Vendored `win-text-inject` + `enigo` crates (path deps) |
| `docs/` | Architecture, ASR, testing, security, performance docs |

---

## Frontend → Backend Communication

### Commands (`invoke`) — request/response

26 commands registered in `lib.rs`, defined in `src-tauri/src/commands/mod.rs`:

| Command | Purpose |
|---|---|
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
| `inject_text` | Inject transcribed text into focused field |
| `set_auto_start` / `get_auto_start` | Windows startup registration |
| `save_api_key` / `get_api_key` | Secure API key storage |
| `get_performance_metrics` | Latency tracker readout |
| `list_models` / `get_model_status` | List models / get model runtime status |
| `download_model` / `cancel_model_download` | Download model / cancel in-flight download |
| `delete_model_version` / `set_active_model` | Delete version / set active version |
| `get_model_registry` | Get raw registry.json content |
| `list_providers` | List cloud providers with status |
| `get_provider_config` / `save_provider_config` | Read/write provider config |
| `save_provider_credential` / `delete_provider_credential` | Store/delete provider API key |
| `test_provider_connection` | Test provider credential |
| `resolve_provider` | Resolve provider via auto-fallback |
### Events (`listen` / `emit`) — backend pushes to frontend

| Event | Payload | When |
|---|---|---|
| `asr:result` | `{text, is_final, language, confidence}` | Partial or final recognition |
| `tray:set-engine` / `tray:set-language` | string | Tray menu selection |
| `show-about` | — | Tray about clicked |
| `model:download_progress` | `{model_id, downloaded, total}` | Model download progress |
| `model:download_complete` | `{model_id, success, error?}` | Model download finished |
| `audio:level` | number | Mic level for waveform |
| `shortcut:pressed` / `shortcut:released` | shortcut string | Hotkey state |
| `recording:started` / `recording:stopped` | — | Lifecycle |
| `engine:switched` | JSON result | After `switch_engine` |
| `tray:set-engine` / `tray:set-language` | string | Tray menu selection |
| `show-about` | — | Tray about clicked |

---

## Development Commands

```bash
# Prerequisites: Rust 1.80+, Bun 1.2+ (preferred) or Node 20+, Tauri CLI 2.0+

bun install              # install frontend deps
bun run tauri:dev        # dev: hot-reload frontend + compile/launch Rust
bun run dev              # frontend-only dev server (browser, no Tauri shell)
bun run build            # tsc -b && vite build (type-check + bundle)
bun run test             # run Vitest unit/component tests once
bun run test:watch       # Vitest watch mode
bun run test:ui          # Vitest with Web UI
bun run tauri:build      # production build → .msi/.exe (Win) / .dmg (macOS)
bun run tauri:build:test # production build using single-window test config
bun run test:e2e         # E2E smoke test (scripts/e2e_smoke.mjs)
```

> **Critical build rule:** Always produce release artifacts with `bun run tauri:build`. `cargo build --release` alone bypasses the Tauri CLI — it skips the frontend bundle and bakes in `devUrl`, producing an EXE that shows a white screen (`ERR_CONNECTION_REFUSED`). Use `cargo check` only to verify Rust compiles. Release output: `src-tauri/target/release/bundle/{msi,nsis}/`.

---

## Testing & QA

### Two-Layer + E2E Strategy

| Layer | Tool | Runs in | Scope |
|---|---|---|---|
| **Unit/Component** | Vitest 4.11 + jsdom 30 | Node (mocked Tauri) | Store logic, component render/interaction, a11y |
| **Rust integration** | `#[cfg(test)]` + `cargo test` | Native | ASR pipeline, aggregator, session state-machine, failure injection |
| **Desktop E2E** | `scripts/e2e_smoke.mjs` (tauri-driver + msedgedriver) | Real built app | App launch, controls, text injection into focused field |

### Vitest

- **Setup** (`src/test/setup.ts`): mocks `@tauri-apps/api/core` (`invoke` → in-memory `fakeStore`), `webviewWindow` (`fakeWebviewWindow` singleton), `event` (no-op). Stubs `matchMedia`, `scrollHeight`/`clientHeight`/`scrollTo`, `execCommand`. Exposes `globalThis.__setEngineResult` to flip engine readiness per test.
- **store.test.ts**: M8 re-entrant guard, settings persistence + `maxChars` side-effect, segment budgeting, toast auto-dismiss.
- **components.test.tsx**: `SegmentItem` (render, a11y role/aria-label, clipboard copy + textarea fallback), `FloatingWindow` (controls render, engine hint, listening toggle, resize-on-content, scroll-to-bottom FAB).
- Run: `bun run test` / `bun run test:watch`.

### Rust Tests

- **`asr/integration_tests.rs`**: `FakeAsrSession`-driven lifecycle, Deepgram/OpenAI event parser contracts, flush/finalization, `TranscriptAggregator` (empty/partial/multi-utterance/out-of-order/duplicate-prevention), `AsrManager` send/receive/close.
- **`asr/failure_tests.rs`**: state-machine recovery (error releases resources, duplicate-start prevented, rapid 10× start-stop), audio device failure, network disconnect / API 401 / 429, model missing, shortcut conflict, permission denied, malformed response, multiple-failure recovery.
- **`models/verifier.rs`**: SHA256 computation + file verification roundtrip.
- **`models/installer.rs`**: atomic extract→validate→rename roundtrip.
- **`provider/registry.rs`**: provider resolution + auto-fallback (offline forces local, automatic prefers local then falls back to cloud, cloud uses preferred).
- **`resources/mod.rs`**: `DistributionFlavor` detection + display.
- Run: `cargo test`.

### E2E

- **Prerequisites**: build single-window test variant (`bun run tauri:build:test`), `msedgedriver` on PATH.
- **Driver chain**: selenium → tauri-driver (port 4444) → msedgedriver (port 4445, WebView2) → `voiceboom.exe`.
- **Single-window config** (`src-tauri/tauri.test.conf.json`): only the floating window (avoids WebDriver attaching to settings window).
- **Covers**: app launch, engine label, start/stop button, settings button.
- **Does NOT cover** (needs real mic/OS loop): global hotkey, live audio capture, ASR transcription, desktop drag.

---

## Code Conventions & Common Patterns

- **Path alias:** `@/` → `src/` (configured in `tsconfig.json` + `vite.config.ts`).
- **Bug-fix markers:** Comments like `// m5 fix:`, `// C2 fix:`, `// M11 fix:`, `// M8 fix:`, `// P0 fix:` encode why code looks the way it does. Read them before modifying surrounding code.
- **UI language:** All user-facing strings are **Simplified Chinese**; code comments and identifiers are **English**.
- **State shape:** Single Zustand store (`useAppStore`) holds `sessionState`, `status` (`'idle'|'listening'|'result'`), `segments[]`, `currentPartial`, `settings`, `audioLevel`, `toastMessage`, `injectionMode` (`'clipboard'|'typing'`). `updateSettings` auto-persists via `save_settings`; `loadSettings` reads `get_settings` once on mount (M8 re-entrancy guard). `injectFinalText(text)` → invokes `inject_text`.
- **Styling:** Tailwind v3 + glassmorphism token layer in `index.css` (`--glass-bg`, `--glass-blur: 30px`, `--glass-radius: 20px`, `.glass`/`.glass-dark` utilities). Framer Motion for animation, with `reduceMotion` escape hatch throughout.
- **No linter or formatter is configured.** No `rustfmt`/`clippy` enforcement.
- **File-based logging:** `lib.rs::init_file_logger` writes to `%TEMP%\voiceboom_debug.log` — the primary debugging channel for the release GUI app (stderr is invisible).
- **Plugin version matching:** When adding a Tauri plugin, the npm package and Rust crate must match on major.minor, and you must add the permission in `src-tauri/capabilities/default.json`.
- **Cross-window sync:** All via Tauri events (`engine:switched`, `tray:set-engine`, `tray:set-language`, `shortcut:pressed/released`, `recording:started`, `asr:result/error/status/timeout`, `audio:level`). Never assume shared memory between windows.

## Commit Message Discipline (mandatory)

**禁止在提交消息中出现任何 AI 联合作者署名。** 每次提交前、提交后、推送前，必须主动检查并清除以下形式的 trailer：

- `Co-Authored-By:` （含模型名、邮箱等任何变体）
- `Generated-by:` / `AI-generated:` 等模型/工具署名行

**执行流程（每次提交必须执行）：**

1. 构造提交消息时，不写入任何署名 trailer。
2. 提交前用 `git log -1 --format='%B'`（或暂存前用草稿文件 `grep -i`）检查，确认不存在上述 trailer。
3. 若发现已写入，立即用 `git commit --amend` 或重写消息文件移除，再提交。
4. 推送前再次核对 `git log origin/main..HEAD --format='%B---'` 中是否含署名；若有，一律修正后再推。

本规则为不可跳过的强约束，与代码风格、测试等规则同级。提交消息只承载意图、约束、验证等决策记录，不附加任何作者身份标记。

---

## Important Files

| File | Role |
|---|---|
| `src/main.tsx` | React 19 bootstrap; wraps `App` in `ErrorBoundary` (M11 fix) |
| `src/App.tsx` | Root routing by window label; registers shortcut + tray listeners |
| `src/stores/useAppStore.ts` | Single source of truth for all client state |
| `src/hooks/useAsr.ts` | ASR lifecycle + event subscriptions |
| `src/hooks/useGlobalShortcut.ts` | Push-to-talk (callback-ref pattern, M9) |
| `src/components/FloatingWindow/index.tsx` | Main transcription surface |
| `src/components/Settings/index.tsx` | 5-tab settings; runs `switch_engine`, polls `get_resource_status` |
| `src/test/setup.ts` | Vitest global setup (Tauri mocks + jsdom stubs) |
| `src-tauri/src/main.rs` | Windows GUI entry; `windows_subsystem=windows` |
| `src-tauri/src/lib.rs` | `AppState`, 26-command registration, setup, system tray, file logger |
| `src-tauri/src/commands/mod.rs` | All command handlers + session state machine (replaces RecordingClaim) |
| `src-tauri/src/inject.rs` | Cross-platform text injection dispatch |
| `src-tauri/src/injection/` | `InjectionController` with Architecture Locks |
| `src-tauri/src/asr/engine_trait.rs` | `StreamingAsrEngine` trait, `AsrConfig`, `AsrResult`, `AsrEngineType` |
| `src-tauri/src/asr/session.rs` | `AsrSession` trait, `AsrEvent` enum, `LegacySessionAdapter` |
| `src-tauri/src/asr/streaming.rs` | `AsrManager` (engine lifecycle + reuse) |
| `src-tauri/src/asr/adapters/local.rs` | sherpa-onnx SenseVoice + Silero VAD (active local engine) |
| `src-tauri/src/asr/adapters/openai_realtime.rs` | OpenAI Realtime WebSocket adapter |
| `src-tauri/src/asr/adapters/deepgram.rs` | Deepgram Streaming WebSocket adapter |
| `src-tauri/src/asr/aggregator.rs` | `TranscriptAggregator` (Architecture Lock F) |
| `src-tauri/src/asr/latency.rs` | `LatencyTracker` (t0–t7, P50/P90/P95/P99) |
| `src-tauri/src/audio/capture.rs` | CPAL mic capture + resample → 16kHz mono f32 |
| `src-tauri/src/audio/pipeline.rs` | Bounded real-time audio pipeline (Architecture Lock C) |
| `src-tauri/src/secure_keystore.rs` | API key storage (DPAPI / Keychain / File 0600) |
| `src-tauri/src/models/mod.rs` | `ModelManager`, `ModelState`, `ModelRegistry`, embedded registry |
| `src-tauri/src/models/downloader.rs` | `ModelDownloader` (retry, progress, resumable, cancellation) |
| `src-tauri/src/models/verifier.rs` | `ModelVerifier` (SHA256 + size validation) |
| `src-tauri/src/models/installer.rs` | `ModelInstaller` (atomic install via .staging + active.json) |
| `src-tauri/src/provider/config.rs` | `ProviderId`, `ProviderMode`, `ProviderConfig` |
| `src-tauri/src/provider/credential.rs` | `ProviderCredentialStore` (OS secure storage) |
| `src-tauri/src/provider/registry.rs` | `ProviderRegistry` (auto-fallback resolve) |
| `src-tauri/src/resources/mod.rs` | `DistributionFlavor`, ONNX model path resolution |
| `src-tauri/tauri.offline.conf.json` | Offline build config (bundles asr-bundle resources) |
| `scripts/prepare-models.py` | CI: prepare model release archives |
| `scripts/install-model-pack.py` | CI: install model pack for offline build |
| `scripts/verify-models.py` | CI: verify model archive integrity |

---

## Runtime / Tooling Preferences

- **Runtime:** Bun (preferred) — `bun.lock` is the lockfile. Node also works.
- **Package manager:** Bun (`bun install`, `bun run`).
- **Frontend bundler:** Vite 6 with `@vitejs/plugin-react`.
- **Dev server:** `127.0.0.1:1420` (strict port, configured in `vite.config.ts`).
- **Rust toolchain:** edition 2021, async via Tokio 1.43 (full features).
- **Key Rust deps:** `tauri 2.2.5`, `sherpa-onnx 1.13 (static)`, `cpal 0.16`, `rusqlite 0.32 (bundled)`, `tokio-tungstenite 0.24`.
- **Vendored (path deps):** `win-text-inject 0.1.1`, `enigo 0.3.0` in `src-tauri/vendor/`.
- **Windows release:** `main.rs` sets `windows_subsystem=windows` (no console window).
- **E2E drivers installed outside repo (not committed):** tauri-driver at `D:\cargo\bin\tauri-driver.exe`, msedgedriver at `D:\msedgedriver\`.
- **CI:** `.github/workflows/release.yml` — validate → build matrix (win-x64 + macos-universal) → optional offline build → publish. SHA256 checksums, draft release, immutable release guard. Model releases via `model-release.yml` on `models-*` tags.
- **Offline build:** triggered on-demand via `workflow_dispatch` (build_offline: true), NOT every release.
### Version Inconsistencies (known)

| File | version | Note |
|---|---|---|
| `package.json` | `0.3.0` | ✅ All version files now aligned |
| `src-tauri/Cargo.toml` | `0.3.0` | Authoritative app version |
| `src-tauri/tauri.conf.json` | `0.3.0` | ✅ Matches Cargo |
| `src-tauri/tauri.test.conf.json` | `0.3.0` | ✅ Matches Cargo |
### Plugin Version Notes

Most `@tauri-apps/plugin-*` packages are pinned to `2.2.0`, but `tauri-plugin-dialog` is `2.7` and `tauri-plugin-autostart` is `2.5.1` (independent release cycles, still Tauri 2.x ABI-compatible). When adding a plugin, pair the npm + Rust crate on major.minor.

---

## Modification Safety Notes

- **Session state machine is load-bearing:** `start_recording` uses a `RecordingSession` state machine (Starting → Recording → Stopping → Finalizing) to guarantee only one active session. The old `RecordingClaim`/`BridgeActiveGuard` RAII guards were replaced by this. Don't bypass the state transitions.

- **Bridge task owns flush:** Never flush from `stop_recording` — it races the bridge's channel-close flush. `stop_recording` only stops audio capture; the bridge task calls `finalize()`.

- **Local adapter reuse:** Changing engine/endpoint/language triggers a rebuild; identical config reuses the resident model.

- **Model path resolution:** ResourceManager searches app-data dir → `asr-bundle/` → portable `models/` next to EXE. All 3 files (model, tokens, VAD) are required for readiness.
- **Model downloads are atomic:** `ModelInstaller` extracts to `.staging/` first, validates every file's SHA256, then renames into place. If a download crashes mid-way, the `.staging/` dir can be safely cleaned — the previous version remains intact. Never write directly into the version directory.
- **Model registry is embedded at compile time:** `ModelManager::new_embedded()` uses `include_str!("../../../models/registry.json")`. Changing the registry requires a recompile. SHA256 values in the registry MUST be generated by the CI pipeline (`prepare-models.py`), never hand-written.
- **Provider credentials never touch plaintext storage:** API keys are stored ONLY in OS secure storage (DPAPI/Keychain/0600 file) via `secure_keystore`. SQLite stores only the `credential_ref` string. Never log, serialize, or persist the actual API key.
- **Cloud provider config is resolved at session start:** The `resolve_provider` command implements auto-fallback (Local → Cloud). The resolved provider + credential are bound to the recording session — changing provider config mid-recording does not affect the active session.

- **Text injection on Windows** uses `win-text-inject`'s delayed rendering — do NOT replace it with a naive clipboard+paste loop (that's the anti-pattern it exists to fix). All synthesized events carry `INJECT_TAG` in `dwExtraInfo`; the hotkey hook should skip events with this tag to avoid re-triggering.

- **Vendored crates:** `src-tauri/vendor/` contains full source for `win-text-inject` and `enigo`. They are path dependencies — no crates.io references. If updating, replace the vendored source and update `Cargo.toml` path versions.

- **PRD.md is NOT tracked by git:** The file `PC端实时流式语音输入法软件 PRD.md` is a local-only product requirements document. It is listed in `.gitignore` and must NEVER be added to git tracking. Before every `git add -A` or `git add .`, check that this file is not included. If accidentally added, use `git rm --cached` to remove it immediately.

  ## Release Safety Policy

  Never:
  - delete a GitHub Release
  - delete a release tag
  - force-push release tags
  - overwrite published release assets
  - change an existing immutable release
  - modify version fields independently

  Before release:
  1. package.json version == tauri.conf.json version
  2. git tag == version
  3. working tree must be clean
  4. CI build must pass
  5. all release artifacts must exist
  6. SHA256SUMS must be generated
  7. release starts as draft

  Never publish a release until all platform builds pass.
