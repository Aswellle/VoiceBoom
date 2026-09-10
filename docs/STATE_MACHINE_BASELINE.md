# VoiceBoom AI — 状态机基线 (State Machine Baseline)

> 生成阶段：Phase 0 — 源码逆向与锁定基线
> 核心发现：**当前不存在任何显式、统一的状态机。** 录音状态分散在 5 个独立标志中，靠开发者的心智模型维系一致性。

---

## 1. 当前录音状态：分散标志而非状态机

### 1.1 全部录音相关标志

```
┌─────────────────────────────────────────────────────────────┐
│                     React 层                                │
│                                                             │
│  useAsr.isListeningRef : useRef<bool>                       │
│  useAsr.isListening    : useState<bool>                     │
│  useAppStore.status    : 'idle' | 'listening' | 'result'    │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                     Rust 后端层                              │
│                                                             │
│  AppState.starting     : AtomicBool  (start_recording 锁)    │
│  AppState.bridge_active: Arc<AtomicBool> (bridge task 运行中)│
│  AudioCapture.is_recording : Arc<AtomicBool> (CPAL 线程状态) │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 无统一枚举

整个项目**没有** `RecordingState` 枚举。状态转换隐含在分散的 `setStatus()`、`is_recording.store()`、`bridge_active.store()` 调用中。

---

## 2. 隐式录音状态转换（逆向还原）

### 2.1 正常路径

```
事件: 用户按下快捷键 / 点击录音按钮
  │
  ▼
┌──────────────────────────────────────────────────────────────┐
│ 状态 A: 空闲 (Idle)                                          │
│                                                              │
│   isListeningRef  = false                                    │
│  isListening     = false                                     │
│  status          = 'idle'                                    │
│  is_recording    = false                                     │
│  bridge_active   = false                                     │
│  starting        = false                                     │
└──────────────────────────────────────────────────────────────┘
  │
  │ startListening():
  │   isListeningRef = true
  │   isListening = true
  │   setStatus('listening')
  │   invoke('start_recording')
  ▼
┌──────────────────────────────────────────────────────────────┐
│ 状态 B: 启动中 (Starting) — 隐含，无显式标志                   │
│                                                              │
│  starting = true (RecordingClaim 持有)                       │
│  [等待 bridge_active == false]                               │
│  [ASR initialize]                                            │
│  [AudioCapture.start_recording → is_recording = true]        │
│  bridge_active = true                                        │
└──────────────────────────────────────────────────────────────┘
  │
  ▼
┌──────────────────────────────────────────────────────────────┐
│ 状态 C: 录音中 (Recording)                                   │
│                                                              │
│  isListeningRef  = true                                      │
│  isListening     = true                                      │
│  status          = 'listening'                               │
│  is_recording    = true                                      │
│  bridge_active   = true                                      │
└──────────────────────────────────────────────────────────────┘
  │
  │ 用户释放快捷键:
  │   isListeningRef = false
  │   isListening = false
  │   setStatus('result')
  │   invoke('stop_recording')
  │     → audio.stop_recording()
  │     → is_recording = false
  ▼
┌──────────────────────────────────────────────────────────────┐
│ 状态 D: 收尾中 (Finalizing) — 隐含                            │
│                                                              │
│  status        = 'result'                                    │
│  is_recording  = false                                       │
│  bridge_active = true  ← bridge 仍在 flush                   │
└──────────────────────────────────────────────────────────────┘
  │
  │ bridge 完成: bridge_active = false
  │ setTimeout(2000):
  ▼
┌──────────────────────────────────────────────────────────────┐
│ 状态 E: 结果展示 (Result) — 持续 2s                          │
│                                                              │
│  status = 'result'                                           │
└──────────────────────────────────────────────────────────────┘
  │
  │ setTimeout 到期
  ▼
  状态 A (Idle)
```

### 2.2 问题：状态不同步场景

```
场景 1: CPAL 线程启动失败
  ─────────────────────────
  is_recording    = true   ← 线程启动前已设置
  bridge_active   = false  ← ASR 未启动 / audio 失败
  status          = 'listening' ← 前端显示录音中
  
  → UI 显示"正在录音"，但实际没有音频，没有 bridge

场景 2: 快速 start/stop
  ─────────────────────────
  start → is_recording=true, bridge_active=true
  stop  → is_recording=false, status='result'
  但前一次 bridge 仍在运行 (bridge_active=true)
  新 start 会等待 bridge_active (max 10s) 或 timeout
  
  → 用户看到 10s 错误提示

场景 3: bridge 异常退出
  ─────────────────────────
  BridgeActiveGuard 保证 bridge_active=false
  但 status 可能仍为 'listening'（前端未收到 stop）
  isListeningRef 可能仍为 true
  
  → 前端认为还在录音，实际 pipeline 已死

场景 4: 2s 超时竞争
  ─────────────────────────
  stop → status='result' → 2s → status='idle'
  若用户在 2s 内再次按下快捷键：
  start 调用时 status 仍为 'result'
  可能触发不同的代码路径
```

---

## 3. ASR 状态（无显式状态机）

### 3.1 AsrManager 隐含状态

```
AsrManager {
    engine: None              ← 未初始化
    engine: Some(engine)      ← 已初始化（可能 ready / 可能失败）
}
```

**问题**：`engine.is_some()` 不等于 "engine 能正常工作"。WS 连接可能已断开但 engine 仍在。

### 3.2 本地 Adapter 隐含状态

```
LocalAsrAdapter {
    ready: bool,                    ← 是否初始化完成
    speech_active: bool,            ← VAD 检测到语音
    vad_offset: usize,              ← VAD 消费进度
    last_interim: Instant,          ← 上次 interim 时间
}
```

**问题**：`ready=true` 不代表 VAD/recognizer 可用（`ensure_models` 可能部分失败）。

### 3.3 云端 Adapter 隐含状态

```
DeepgramAdapter {
    ready: bool,                    ← 已 spawn WS task
    ws_sender: Option<...>,         ← Some = 通道存活
    ws_receiver: Option<...>,       ← Some = 可收结果
}
```

**问题**：`ready=true` 后 WS 可能已断开，但 `ready` 不会回滚。

---

## 4. 注入状态（无状态机）

```
injectFinalText(text)
  │
  └─→ invoke('inject_text')   ← fire-and-forget
       │
       └─→ 无状态反馈（仅 toast）
```

**问题**：
- 不知道注入是否完成
- 不知道注入是否成功（除非看 toast）
- 多次 final → 多次注入
- 无 session 隔离

---

## 5. 快捷键状态（无事务保证）

```
GlobalShortcutManager {
    current_shortcut: Option<String>
}
```

### 当前注册流程（非事务化）

```
register(new_shortcut):
  1. unregister(old)          ← 先删旧的
  2. parse(new)               ← 解析新的
  3. gs.on_shortcut(new)      ← 注册新的
  4. current_shortcut = Some(new)

问题：
  若 step 2 parse 失败 → old 已丢失，new 未注册
  若 step 3 注册失败    → old 已丢失，new 未注册
```

---

## 6. 前端 UI 状态

### 6.1 Zustand 状态字段

```typescript
status: 'idle' | 'listening' | 'result'
segments: RecognitionSegment[]
currentPartial: string
audioLevel: number
toastMessage: string
settingsLoaded: boolean
shortcutRegistered: boolean
```

### 6.2 派生状态（前端自行推断）

```typescript
// FloatingWindow 中
const isListening = status === 'listening'  // 从 status 推断

// 问题：status='listening' 不代表后端真在录音
```

---

## 7. 目标状态机（Phase 2 实施方向）

### 7.1 Recording Session State Machine（推荐）

```rust
pub enum RecordingState {
    Idle,
    Starting,
    Recording,
    Stopping,
    Finalizing,
    Error,
}
```

### 7.2 转换规则

```
Idle ──(user press)──→ Starting
Starting ──(init ok)──→ Recording
Starting ──(init fail)──→ Error ──(cleanup)──→ Idle
Recording ──(user release)──→ Stopping
Stopping ──(audio stopped)──→ Finalizing
Finalizing ──(flush done)──→ Idle
Finalizing ──(flush timeout)──→ Error ──→ Idle
```

### 7.3 单一权威源

**Recording Session 状态必须是录音生命周期的唯一权威状态源。** UI 只订阅，不推断。

---

## 8. 状态一致性风险矩阵

| 场景 | isListening | status | is_recording | bridge_active | 真实状态 |
|---|---|---|---|---|---|
| 正常录音 | true | listening | true | true | ✅ 录音中 |
| CPAL 失败 | true | listening | **true** | **false** | ❌ 假录音 |
| bridge 崩溃 | **true** | **listening** | **true** | false | ❌ 管道已死 |
| flush 中 | false | result | false | **true** | ✅ 收尾中 |
| 2s 超时内 | false | result | false | false | ✅ 展示结果 |
| 快速重按 | true | result→listening | true | **true(旧)** | ⚠️ 竞争 |

**结论：没有任何单一变量能反映真实录音状态。**
