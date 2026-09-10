# VoiceBoom AI — 架构文档

> 生成阶段：Phase 18 — 文档同步
> 基线提交：`main`

---

## 1. 系统概览

**VoiceBoom AI** 是一个跨平台实时流式语音输入法。核心链路：

```
[按住快捷键] → [麦克风采集] → [流式 ASR] → [实时转写] → [稳定最终结果] → [注入焦点输入框]
```

### 技术栈

| 层 | 技术 |
|---|---|
| 桌面壳 | Tauri 2.0 + Rust |
| 前端 | React 19 + TypeScript + Zustand |
| 音频采集 | CPAL (16kHz mono f32) |
| 本地 ASR | sherpa-onnx SenseVoice + Silero VAD |
| 云端 ASR | OpenAI Realtime Transcription / Deepgram Streaming |
| 持久化 | SQLite (settings/history) |
| 安全存储 | OS Credential Manager (Win) / Keychain (macOS) / File 0600 (Linux) |
| 文本注入 | win-text-inject (Windows) / enigo (macOS/Linux) |

---

## 2. 分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                  React 19 + TypeScript UI                    │
│                                                             │
│  FloatingWindow ─── useAsr ─── useGlobalShortcut            │
│       │                │                                    │
│       ▼                ▼                                    │
│  Zustand Store (useAppStore) ──── Event Listeners            │
└──────────────────────┬──────────────────────────────────────┘
                       │ invoke() / listen()
                       ▼
┌─────────────────────────────────────────────────────────────┐
│                Tauri 2.x Command Layer                       │
│                                                             │
│  start_recording / stop_recording / inject_text              │
│  register_shortcut / get_settings / save_api_key ...        │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│                   Rust Backend Core                          │
│                                                             │
│  AppState                                                   │
│   ├── RecordingSession (idle→starting→recording→stopping→finalizing→idle/error) │
│   ├── AudioCapture (CPAL thread → bounded channel)           │
│   ├── AsrManager (engine lifecycle)                          │
│   ├── GlobalShortcutManager (transactional registration)     │
│   ├── Database (SQLite)                                      │
│   └── ResourceManager (ONNX paths)                           │
│                                                             │
│  Bridge Task (tokio::spawn)                                 │
│   └── audio_rx → asr.send_audio → asr.receive_result        │
│                   → emit 'asr:result'                       │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. 核心数据链路

```
[麦克风硬件]
    │  CPAL callback (设备原生采样率, 可能多通道)
    ▼
[AudioCapture CPAL thread]
    │  1. downmix to mono (channels average)
    │  2. resample_linear() → 16kHz
    │  3. audio_tx.send(AudioFrame { sequence, timestamp, samples })
    ▼
[bounded mpsc channel < AudioFrame >]  ← 有界队列 (~8 frames ≈ 500ms)
    │
    ▼
[Bridge Task: audio_rx.recv()]
    │  asr.send_audio(&samples)
    │  asr.receive_result() → emit 'asr:result'
    ▼
[ASR Engine (provider-specific)]
    ├─ Local: buffer → VAD window → decode
    ├─ OpenAI: base64 PCM16 → WS → delta/done
    └─ Deepgram: PCM16 → WS → Results
    ▼
[Normalized AsrEvent]
    ├─ Partial { text, language }
    ├─ SegmentFinal { text, language, confidence }
    ├─ UtteranceFinal { text, language, confidence }
    └─ Error { code, message, retryable }
    ▼
[TranscriptAggregator]
    ├─ Partial → replace currentPartial
    ├─ SegmentFinal → commit segment, clear partial
    └─ UtteranceFinal → commit utterance, mark injection-ready
    ▼
[Text Injection]
    ├─ [Windows] win-text-inject (delayed-render clipboard)
    └─ [macOS/Linux] enigo clipboard + paste
    ▼
[Focused Application]
```

---

## 4. 状态机

### Recording Session State Machine

```
[用户按下快捷键]
    │
    ▼
┌──────────────────────────────────────────────────────────────┐
│  Idle ──(begin_start)──→ Starting ──(mark_recording)──→ Recording ──(begin_stop)──→ Stopping
│                                                             │
│  Error ◄──(fail)──┘                                         │
│  │                                                         │
│  └──(reset)──→ Idle ◄──(complete)──┘                       │
└──────────────────────────────────────────────────────────────┘
```

**转换规则**：
- `begin_start`: Idle/Error → Starting
- `mark_recording`: Starting → Recording
- `begin_stop`: Recording → Stopping
- `mark_finalizing`: Stopping → Finalizing
- `complete`: Finalizing → Idle
- `fail`: Any → Error
- `reset`: Error → Idle

---

## 5. 目录结构

```
src/
├── components/
│   ├── FloatingWindow/   # 毛玻璃悬浮窗（核心 UI）
│   ├── Waveform/         # 音频波形可视化
│   ├── Settings/         # 设置面板（7 标签页）
│   └── Shared/           # 通用 UI 组件
├── hooks/
│   ├── useAsr.ts         # ASR 生命周期 + 事件订阅
│   └── useGlobalShortcut.ts # 按住说话 (push-to-talk)
├── stores/
│   └── useAppStore.ts    # Zustand 全局状态
└── test/                 # Vitest 测试 + setup

src-tauri/
├── src/
│   ├── asr/
│   │   ├── adapters/     # Deepgram / OpenAI / Local 适配器
│   │   ├── session.rs    # RecordingSession 状态机
│   │   ├── streaming.rs  # AsrManager
│   │   ├── aggregator.rs # TranscriptAggregator
│   │   ├── latency.rs    # 延迟追踪
│   │   └── engine_trait.rs # StreamingAsrEngine (legacy)
│   ├── audio/
│   │   ├── capture.rs    # CPAL 音频采集
│   │   └── pipeline.rs   # AudioFrame + bounded channel
│   ├── commands/         # Tauri command handlers
│   ├── db/               # SQLite 数据库
│   ├── inject.rs         # 跨平台文本注入
│   ├── secure_keystore.rs # OS 安全密钥存储
│   ├── shortcut/         # 全局快捷键管理
│   └── resources/        # ONNX 模型路径解析
└── vendor/               # win-text-inject + enigo (path deps)
```

---

## 6. 关键设计决策

### 有界音频队列
- 容量 = 500ms / 64ms ≈ 8 frames
- Overflow 策略：丢弃最旧帧（实时优先）
- CPAL callback 永不阻塞

### 统一 ASR 事件模型
- 所有 provider 输出映射到 `AsrEvent`
- `Partial` / `SegmentFinal` / `UtteranceFinal` / `Error`
- UI 不直接理解 provider-specific 事件

### 事务化快捷键注册
- 先注册新快捷键 → 成功后再注销旧快捷键
- 失败时旧快捷键保持活动（rollback）

### 安全密钥存储
- API Key 存储在 OS 安全存储（DPAPI/Keychain/File 0600）
- SQLite 仅保存非敏感配置
- 密钥不出现在日志、URL、错误消息中
