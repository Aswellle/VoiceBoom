# VoiceBoom AI — 架构基线 (Architecture Baseline)

> 生成阶段：Phase 0 — 源码逆向与锁定基线
> 基线分支：`main` (`a2fd6a7`)
> 状态：只读分析，不含代码修改

---

## 1. 系统分层概览

```
┌───────────────────────────────────────────────────────────┐
│                  React 19 + TypeScript UI                  │
│                                                           │
│  FloatingWindow ─── useAsr ─── useGlobalShortcut          │
│       │                │                                   │
│       ▼                ▼                                   │
│  Zustand Store (useAppStore) ──── Event Listeners          │
└──────────────────────┬────────────────────────────────────┘
                       │ invoke() / listen()
                       ▼
┌───────────────────────────────────────────────────────────┐
│                Tauri 2.x Command Layer                     │
│                                                           │
│  start_recording / stop_recording / inject_text            │
│  register_shortcut / get_settings / save_api_key ...      │
└──────────────────────┬────────────────────────────────────┘
                       │
                       ▼
┌───────────────────────────────────────────────────────────┐
│                   Rust Backend Core                        │
│                                                           │
│  AppState                                                 │
│   ├── AudioCapture (CPAL thread)                          │
│   ├── AsrManager (engine lifecycle)                       │
│   ├── GlobalShortcutManager                               │
│   ├── Database (SQLite)                                   │
│   └── ResourceManager (ONNX paths)                        │
│                                                           │
│  Bridge Task (tokio::spawn)                               │
│   └── audio_rx → asr.send_audio → asr.receive_result      │
│                   → emit 'asr:result'                     │
└───────────────────────────────────────────────────────────┘
```

---

## 2. 文件职责映射 (File-to-Responsibility Map)

| 文件 | 职责 | 层 |
|---|---|---|
| `src/main.tsx` | React 19 启动，ErrorBoundary | UI bootstrap |
| `src/App.tsx` | 按 window label 路由；注册 shortcut + tray 监听 | UI root |
| `src/stores/useAppStore.ts` | **唯一** Zustand 全局状态；settings/segments/status/toast | State |
| `src/hooks/useAsr.ts` | ASR 生命周期；订阅 `asr:result`/`asr:error`/`asr:status`/`audio:level` | State/Bridge |
| `src/hooks/useGlobalShortcut.ts` | 按住说话 (push-to-talk)；`shortcut:pressed/released` → start/stop | State/Bridge |
| `src/components/FloatingWindow/index.tsx` | 主转写界面；持有共享 useAsr 实例 | UI |
| `src/components/Settings/index.tsx` | 7 标签设置面板；执行 switch_engine、轮询 resource_status | UI |
| `src-tauri/src/lib.rs` | `AppState` 定义；command 注册；setup 初始化；file logger | Backend entry |
| `src-tauri/src/commands/mod.rs` | **所有 14 个** `#[tauri::command]` handler + RAII guards | Command layer |
| `src-tauri/src/audio/capture.rs` | CPAL 麦克风采集；重采样到 16kHz mono f32；**unbounded channel** | Audio |
| `src-tauri/src/asr/engine_trait.rs` | `StreamingAsrEngine` trait；`AsrConfig`/`AsrResult`/`AsrEngineType` | ASR abstraction |
| `src-tauri/src/asr/streaming.rs` | `AsrManager`：engine 生命周期；本地 adapter 复用 | ASR lifecycle |
| `src-tauri/src/asr/adapters/local.rs` | sherpa-onnx SenseVoice + Silero VAD | ASR provider |
| `src-tauri/src/asr/adapters/openai_whisper.rs` | OpenAI WebSocket adapter（**协议不可靠**） | ASR provider |
| `src-tauri/src/asr/adapters/deepgram.rs` | Deepgram WebSocket adapter（**语义不完整**） | ASR provider |
| `src-tauri/src/inject.rs` | 跨平台文本注入；win-text-inject / enigo | Injection |
| `src-tauri/src/db/mod.rs` | SQLite：settings/history/model_config 表 | Persistence |
| `src-tauri/src/shortcut/mod.rs` | 全局快捷键注册/注销（**非事务化**） | Shortcut |
| `src-tauri/src/resources/mod.rs` | ONNX 模型路径解析 | Resource |
| `src-tauri/src/tray/mod.rs` | 系统托盘图标 + 菜单 | Tray |

---

## 3. 调用图 (Call Graph)

### 3.1 录音启动链

```
用户动作 (快捷键按下 / 手动按钮)
  │
  ▼
useGlobalShortcut / FloatingWindow
  │  startListening()
  ▼
useAsr.startListening()
  │  setStatus('listening')
  │  invoke('start_recording', {engine, language, apiKey, endpoint, device, vadSensitivity})
  ▼
commands::start_recording()
  │
  ├─ RecordingClaim::try_acquire(&state.starting)     ← 原子防双启动
  ├─ wait for bridge_active == false (max 10s)
  ├─ check audio.is_recording()
  ├─ parse_engine_type(engine)
  │
  ├─ [local] ResourceManager 解析 vad/model/tokens 路径
  │       emit 'asr:status'
  │
  ├─ AsrManager.initialize(config)
  │       ├─ [local + 同配置] 复用 resident adapter
  │       └─ [cloud / 配置变化] 重建 adapter
  │
  ├─ AudioCapture.start_recording(device)
  │       └─ spawn CPAL thread → unbounded_channel< Vec<f32> >
  │
  ├─ state.bridge_active.store(true)
  ├─ spawn Bridge Task
  │
  └─ emit 'recording:started'
```

### 3.2 Bridge Task 数据流

```
CPAL callback (audio thread)
  │  downmix to mono → resample to 16kHz → audio_tx.send(resampled)
  ▼
audio_rx (UnboundedReceiver< Vec<f32> >)
  │
  ▼
Bridge Task (tokio::spawn)
  │
  ├─ loop audio_rx.recv()
  │     ├─ Some(samples):
  │     │    ├─ asr.send_audio(&samples)
  │     │    ├─ asr.receive_result() → emit 'asr:result' {text, is_final, language, confidence}
  │     │    └─ emit 'asr:heartbeat' (每 500ms)
  │     │
  │     └─ None (channel closed):
  │          └─ asr.flush() → emit 'asr:result' {is_final: true}
  │
  └─ Drop → BridgeActiveGuard → bridge_active.store(false)
```

### 3.3 前端结果消费

```
emit 'asr:result' {text, is_final, language, confidence}
  │
  ▼
useAsr listener
  ├─ is_final == true:
  │    ├─ addSegment({...isFinal: true})
  │    └─ useAppStore.injectFinalText(text)  ← 立即注入
  │
  └─ is_final == false:
       └─ updatePartial(text)
```

### 3.4 录音停止链

```
用户动作 (快捷键释放 / 手动按钮)
  │
  ▼
useAsr.stopListening()
  │  setStatus('result')
  │  invoke('stop_recording')
  ▼
commands::stop_recording()
  │  AudioCapture.stop_recording()     ← 关闭 audio_tx
  │  emit 'recording:stopped'
  ▼
Bridge Task 收到 None → flush → 退出
  │
  ▼
useAsr.stopListening() 继续：
  │  setTimeout(2000) → setStatus('idle')
```

### 3.5 文本注入链

```
injectFinalText(text)
  │  invoke('inject_text', {text, mode})
  ▼
commands::inject_text()
  │  crate::inject::inject(&text, &mode)
  ▼
inject.rs::inject()
  ├─ [Windows] InjectionMode::Clipboard:
  │    └─ win_text_inject::inject()  ← delayed-render clipboard
  ├─ [Windows] InjectionMode::Typing:
  │    └─ enigo 按键模拟
  └─ [macOS/Linux]:
       └─ enigo clipboard + paste
```

---

## 4. 数据流 (Data Flow)

### 4.1 音频数据流

```
麦克风硬件
  │  CPAL callback (设备原生采样率, 可能多通道)
  ▼
AudioCapture CPAL thread
  │  1. downmix to mono (channels average)
  │  2. resample_linear() → 16kHz
  │  3. audio_tx.send(Vec<f32>)
  ▼
tokio::sync::mpsc::unbounded_channel < Vec<f32> >
  │  ⚠ 无界队列 — ASR 慢于采集时无限增长
  ▼
Bridge Task: audio_rx.recv()
  │  asr.send_audio(&samples)
  ▼
ASR Engine (provider-specific)
  ├─ Local: buffer.extend → VAD window → decode
  ├─ OpenAI: f32→PCM16→WS Binary
  └─ Deepgram: f32→PCM16→WS Binary
```

### 4.2 结果数据流

```
ASR Engine
  │  AsrResult {text, is_final, language, confidence}
  ▼
Bridge Task
  │  emit 'asr:result' (JSON)
  ▼
useAsr listener
  ├─ is_final=true  → addSegment + injectFinalText
  └─ is_final=false → updatePartial
  ▼
Zustand store → FloatingWindow 渲染
```

### 4.3 设置/状态持久化流

```
updateSettings(partial)
  │  set settings
  │  invoke('save_settings' / 'save_api_key')
  ▼
Database
  ├─ settings table (key-value)
  └─ model_config table (API key, encrypted=1 flag)
```

---

## 5. 录音生命周期 (Recording Lifecycle)

### 当前实际状态（分散，无统一状态机）

```
[用户按下快捷键]
     │
     ▼
useAsr.isListeningRef = true, isListening = true
Zustand status = 'listening'
     │
     ▼
start_recording 执行：
     │  RecordingClaim 获取
     │  bridge_active 等待
     │  ASR initialize
     │  AudioCapture.start_recording → is_recording = true ⚠ (线程启动前设置)
     │  bridge_active = true
     │  spawn bridge task
     │
     ▼
[录音中] — 多个独立标志：
     │  isListeningRef = true (React ref)
     │  Zustand status = 'listening'
     │  AudioCapture.is_recording = true (AtomicBool)
     │  bridge_active = true (Arc<AtomicBool>)
     │
     ▼
[用户释放快捷键]
     │
     ▼
useAsr.isListeningRef = false, isListening = false
Zustand status = 'result'
AudioCapture.stop_recording() → is_recording = false
     │
     ▼
Bridge Task 检测到 channel close → flush → bridge_active = false
     │
     ▼
setTimeout(2000) → Zustand status = 'idle'
```

### ⚠ 关键问题：5 个独立状态源

| 状态源 | 类型 | 位置 |
|---|---|---|
| `isListeningRef` | `useRef<bool>` | useAsr.ts |
| `isListening` | `useState<bool>` | useAsr.ts |
| `status` | `'idle'\|'listening'\|'result'` | Zustand store |
| `is_recording` | `Arc<AtomicBool>` | AudioCapture |
| `bridge_active` | `Arc<AtomicBool>` | AppState |

**任何异常顺序都可能导致 UI 显示与后端实际不一致。**

---

## 6. ASR 生命周期 (ASR Lifecycle)

### 6.1 引擎创建与复用

```
start_recording → AsrManager.initialize(config)
     │
     ├─ [本地 + 前次配置相同] → 复用 resident adapter（避免重载 240MB 模型）
     │
     └─ [云端 / 配置变化] → 重建 adapter
          ├─ 创建新 engine
          ├─ engine.initialize(config)
          └─ 关闭旧 engine（如有）
```

### 6.2 本地 SenseVoice 生命周期

```
initialize()
     │  ensure_models() → 加载 VAD + recognizer（仅首次）
     │  vad.reset() / buffer.clear() / speech_active = false
     ▼
send_audio(samples)
     │  buffer.extend_from_slice(samples)
     ▼
receive_result() (每帧调用)
     │  1. 按 VAD_WINDOW_SIZE(512) 喂入 VAD
     │  2. speech onset 检测
     │  3. 每 0.2s interim decode（整个 buffer 重解码）
     │  4. VAD segment 完成 → final decode → 重置 buffer
     ▼
flush()
     │  vad.flush() → 处理剩余 segment
     │  fallback: 解码原始 buffer
     ▼
close()
     │  buffer.clear / ready = false
```

### 6.3 云端 Adapter 生命周期（OpenAI / Deepgram 同构）

```
initialize()
     │  创建 unbounded_channel (audio) + unbounded_channel (result)
     │  tokio::spawn WS task
     │  ready = true
     ▼
WS task loop
     │  select! { audio recv / ping / ws msg }
     ▼
send_audio(samples) → ws_sender.send(Vec<f32>)
     │  WS task: f32→PCM16→Binary
     ▼
receive_result() → try_recv from result channel
     │
     ▼
flush()
     │  发送空 Vec 作为 flush 信号
     │  sleep(500ms) ⚠ 固定时延
     │  receive_result()
     ▼
close()
     │  drop ws_sender → WS task 退出
```

---

## 7. 注入生命周期 (Injection Lifecycle)

```
asr:result {is_final: true}
     │
     ▼
useAsr listener → injectFinalText(text)
     │
     ▼
Zustand injectFinalText(text)
     │  invoke('inject_text', {text, mode})  ← fire-and-forget
     ▼
commands::inject_text()
     │  inject::inject(&text, &mode)
     ▼
inject.rs
     ├─ [Windows + Clipboard] → win_text_inject::inject()
     │    │  delayed-render clipboard
     │    │  解决：剪贴板历史隐私 / 修饰键 / UIPI / 恢复竞争
     │    └─ 合成事件带 INJECT_TAG (dwExtraInfo)
     │
     ├─ [Windows + Typing] → enigo 按键
     │
     └─ [macOS/Linux] → enigo clipboard + paste
```

### ⚠ 注入触发问题

- **每次 `is_final=true` 都触发注入** — 本地引擎每个 VAD segment 都是 final，导致一段话被多次注入
- **无 session 绑定** — 旧 session 的 final 可能注入到新 session
- **无注入结果反馈** — fire-and-forget，前端只得到 toast

---

## 8. 错误流 (Error Flow)

### 8.1 错误源与处理

| 错误场景 | 处理方式 | 问题 |
|---|---|---|
| 无输入设备 | CPAL thread `return`，`is_recording` 仍为 true | ⚠ 假录音状态 |
| build_input_stream 失败 | CPAL thread `return`，`is_recording` 仍为 true | ⚠ 假录音 |
| stream.play() 失败 | CPAL thread `return`，`is_recording` 仍为 true | ⚠ 假录音 |
| ASR 初始化失败 | emit 'asr:error'，return Err | ✅ 正确处理 |
| send_audio 失败 | `log::warn` | ⚠ 静默 |
| receive_result 失败 | `log::error` | ⚠ 静默 |
| flush 失败 | emit 'asr:error' | ✅ |
| flush 返回空 + 无 partial | emit 'asr:error' "没有识别到语音" | ✅ |
| inject 失败 | toast | ✅ |

### 8.2 ⚠ CPAL 线程失败不回滚

`AudioCapture::start_recording()` 在 `spawn` 线程**之后**立即 `is_recording.store(true)`。线程内部任何失败（设备、config、stream）都直接 `return`，但 `is_recording` 永远为 true，直到下次 `stop_recording`。

---

## 9. 状态源矩阵 (State Source Matrix)

| 状态概念 | 权威源 | 其他副本/推导 | 一致性风险 |
|---|---|---|---|
| 是否正在录音 | ❌ 无单一权威 | isListeningRef / status / is_recording / bridge_active | **高** |
| 当前 ASR 引擎 | `settings.engine` (Zustand) | AsrManager.config | 中 |
| 当前快捷键 | `GlobalShortcutManager.current_shortcut` | settings.shortcut | 中 |
| API Key | model_config 表 | settings.apiKey (内存) | 低 |
| 转写结果 | segments[] / currentPartial (Zustand) | — | 低 |
| 设置 | settings (Zustand) + settings 表 | — | 低 |
| 模型可用状态 | ResourceManager 路径检查 | asr:status 事件 | 中 |

---

## 10. 已确认架构缺陷索引

| ID | 问题 | 位置 | 本文章节 |
|---|---|---|---|
| P0-A | OpenAI 适配器协议不可靠 | openai_whisper.rs | §6.3 |
| P0-B | Deepgram 语义不完整 | deepgram.rs | §6.3 |
| P0-C | AudioCapture 假录音状态 | capture.rs:218 | §8.2 |
| P0-D | 音频 unbounded channel | capture.rs:53 | §4.1 |
| P0-E | API Key "encrypted=1" 非加密 | db/mod.rs | — |
| P1-A | 本地 interim 每 200ms 全 buffer 重解码 | local.rs:219-243 | §6.2 |
| P1-B | flush() 固定 sleep(500ms) | openai/deepgram.rs | §6.3 |
| P1-C | ASR task 无 JoinHandle/CancellationToken | streaming.rs | §6 |
| P1-D | 快捷键注册非事务化 | shortcut/mod.rs:22-51 | — |
| P1-E | 前后端多录音状态源 | useAsr + store + capture | §5 |
| P1-F | 测试不覆盖核心链路 | src/test/ | — |

---

## 11. 运行时依赖

| 组件 | 版本 | 用途 |
|---|---|---|
| Tauri | 2.2.5 | 桌面壳 |
| CPAL | 0.16 | 音频采集 |
| sherpa-onnx | 1.13 (static) | 本地 ASR |
| tokio-tungstenite | 0.24 | 云端 WS |
| rusqlite | 0.32 (bundled) | 持久化 |
| enigo | 0.3.0 (vendored) | 注入/按键 |
| win-text-inject | 0.1.1 (vendored) | Windows 注入 |
| Zustand | — | 前端状态 |
| Framer Motion | — | 动画 |

---

## 12. 构建基线

| 检查项 | 结果 |
|---|---|
| `bun install` | ✅ 无变更 |
| `bun run test` (Vitest) | ✅ 2 files / 19 tests passed |
| `cargo check` | ✅ 编译通过，5 warnings (non_snake_case) |
| 测试覆盖率 | 仅 UI 组件，无核心链路测试 |
