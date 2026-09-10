# VoiceBoom AI — 逆向工程主报告

> 生成阶段：Phase 0 — 源码逆向与锁定基线
> 基线提交：`a2fd6a7` (main)
> 范围：全仓只读分析，零代码修改

---

## 1. 执行摘要

VoiceBoom AI 是一个功能完整的 Tauri 2 语音输入法 MVP，实现了"按住快捷键 → 录音 → 转写 → 注入"主链路。但在**架构层面**存在多个系统性缺陷，导致核心链路不可靠、不可测试、不可观测。

**最关键的 4 个事实：**

1. **录音状态没有单一权威源** — 5 个独立标志分散在 React/Rust 边界，任何异常顺序都导致 UI 与后端不一致。
2. **OpenAI 适配器协议层错误** — 把 HTTP REST API 的 URL 当作 WebSocket endpoint，假设的响应格式与现代 Realtime Transcription 协议不匹配。
3. **音频管线使用无界队列** — 当 ASR 解码慢于音频采集时，内存和延迟无限增长。
4. **flush 基于固定 500ms sleep** — 而非 provider completion event，导致结果丢失或延迟不确定。

---

## 2. Gate 0 — 准入问答

> 只有能回答以下问题，才允许进入 Phase 1。

### Q1: 录音由谁真正启动？

**答**：`commands::start_recording()` (Rust)。前端 `useAsr.startListening()` 通过 `invoke('start_recording')` 触发。实际启动序列：
1. `RecordingClaim::try_acquire` 获取原子锁
2. 等待 `bridge_active == false`
3. `AsrManager.initialize(config)` 初始化 ASR
4. `AudioCapture.start_recording()` 启动 CPAL 线程
5. `bridge_active.store(true)`
6. `tokio::spawn` bridge task

**但**：`is_recording=true` 在 CPAL 线程**启动前**设置（capture.rs:218），此时录音可能尚未真正开始。

### Q2: 音频由谁产生？

**答**：`AudioCapture` 的 CPAL 回调线程（capture.rs:58-216）。回调函数：
1. 从设备读取原始 PCM（F32/I16/U16）
2. downmix 到 mono
3. `resample_linear()` 重采样到 16kHz
4. `audio_tx.send(resampled)` 发送到 unbounded channel

### Q3: 谁消费音频？

**答**：Bridge Task（commands/mod.rs:242-322）。`tokio::spawn` 的异步任务：
1. `audio_rx.recv()` 接收 PCM 帧
2. `asr.send_audio(&samples)` 送入 ASR
3. `asr.receive_result()` 轮询结果
4. `emit 'asr:result'` 推送到前端

### Q4: ASR final 由谁定义？

**答**：**取决于 provider，且定义不一致：**

| Provider | final 定义 | 问题 |
|---|---|---|
| 本地 SenseVoice | VAD segment 完成 → `is_final=true` | 每个 segment 都是 final，一段话多个 final |
| OpenAI | 假设 `json["type"]=="final"` | 协议不匹配，实际不会收到此格式 |
| Deepgram | `channel["is_final"]==true` | segment 稳定 ≠ 讲话结束 |

**没有统一的 utterance-final 概念。** 前端把任何 `is_final=true` 都当成注入时机。

### Q5: 谁决定 flush 完成？

**答**：Bridge Task 的 `None` 分支（commands/mod.rs:287-318）。当 audio channel 关闭时：
1. 调用 `asr.flush()`
2. 本地：`vad.flush()` + 解码剩余 buffer → 返回 final
3. 云端：发送空 Vec 信号 → **sleep(500ms)** → 读一个 result

**问题**：flush 完成基于**时间常数**（500ms），不是 provider completion event。

### Q6: 谁决定文本可以注入？

**答**：`useAsr` 的 `asr:result` 监听器（useAsr.ts:38-54）。当 `is_final==true` 时：
1. `addSegment()` 添加到 store
2. `injectFinalText(text)` 立即注入

**问题**：
- 本地引擎每个 VAD segment 都触发注入（多次注入一段话）
- 无 session 隔离（旧 session 的 final 可能注入到新 session）
- 注入是 fire-and-forget，无结果确认

### Q7: 应用退出时谁负责停止？

**答**：**没有人显式负责。** `lib.rs:215-217` 的 run handler 是空函数：
```rust
.run(|_app_handle, _event| {
    // No explicit cleanup needed
});
```

依赖：
- `AudioCapture::Drop` 停止 stream
- `BridgeActiveGuard::Drop` 清除 bridge_active
- 进程退出时 CPAL thread 被 OS 终止
- WS task 被 tokio runtime 丢弃

**问题**：没有优雅关闭，可能导致：
- 最后一次 flush 未完成
- 音频 channel 中数据丢失
- WS 连接未正常关闭

---

## 3. 核心数据链路图

```
[快捷键按下]
    │ shortcut:pressed
    ▼
useGlobalShortcut → useAsr.startListening()
    │ setStatus('listening')
    │ invoke('start_recording')
    ▼
┌─ commands::start_recording ─────────────────────────────────┐
│  RecordingClaim::try_acquire                                │
│  wait bridge_active == false                                │
│  AsrManager.initialize(config)                              │
│  AudioCapture.start_recording() → spawn CPAL thread         │
│  bridge_active = true                                       │
│  tokio::spawn bridge_task                                   │
│  emit 'recording:started'                                   │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ CPAL Thread ───────────────────────────────────────────────┐
│  callback: downmix → resample → audio_tx.send()              │
│  ⚠ unbounded_channel                                       │
│  ⚠ is_recording=true 在 thread 启动前设置                    │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ Bridge Task ───────────────────────────────────────────────┐
│  loop {                                                     │
│    audio_rx.recv() →                                       │
│      asr.send_audio() →                                    │
│      asr.receive_result() →                                │
│        emit 'asr:result' {text, is_final, language, conf}   │
│  }                                                          │
│  None → asr.flush() → emit final                            │
│  Drop → bridge_active = false                               │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ useAsr listener ──────────────────────────────────────────┐
│  asr:result {is_final=true} →                              │
│    addSegment() + injectFinalText()                         │
│  asr:result {is_final=false} →                             │
│    updatePartial()                                          │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ injectFinalText ──────────────────────────────────────────┐
│  invoke('inject_text', {text, mode})                        │
│  → inject.rs::inject()                                      │
│  → [Windows] win_text_inject 或 enigo                       │
│  → [macOS/Linux] enigo clipboard+paste                      │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. 问题优先级与影响

### P0 — 必须立即修复（核心链路不可靠）

| ID | 问题 | 影响 | Phase |
|---|---|---|---|
| P0-A | OpenAI 适配器协议错误 | OpenAI 完全不可用 | Phase 6 |
| P0-B | Deepgram 语义不完整 | 注入时机错误 | Phase 5 |
| P0-C | AudioCapture 假录音 | UI 显示录音但实际没有 | Phase 2 |
| P0-D | 音频 unbounded channel | 内存/延迟无限增长 | Phase 3 |
| P0-E | API Key 明文存储 | 安全漏洞 | Phase 11 |

### P1 — 架构质量（可维护性/可测试性）

| ID | 问题 | 影响 | Phase |
|---|---|---|---|
| P1-A | 本地 interim 全 buffer 重解码 | 长句 CPU/延迟增长 | Phase 7 |
| P1-B | flush 固定 sleep(500ms) | 结果丢失或延迟 | Phase 9 |
| P1-C | ASR task 无生命周期控制 | 无法取消/超时 | Phase 4 |
| P1-D | 快捷键注册非事务化 | 快捷键丢失 | Phase 12 |
| P1-E | 多录音状态源 | UI/后端不一致 | Phase 2 |
| P1-F | 测试不覆盖核心链路 | 回归风险 | Phase 14 |

---

## 5. 推荐重构顺序

```
Phase 0  ██ 源码逆向（当前）
Phase 1  ██ 建立基线（build/test/diagnostics）
Phase 2  ██ Recording Session State Machine ← 收敛状态
Phase 3  ██ Bounded Audio Pipeline ← 消除无界队列
Phase 4  ██ ASR Session Abstraction ← 统一接口
Phase 5  ██ Deepgram 重写 ← 修复 P0-B
Phase 6  ██ OpenAI 重写 ← 修复 P0-A
Phase 7  ██ SenseVoice 优化 ← 修复 P1-A
Phase 8  ██ Transcript Aggregator ← 解决重复注入
Phase 9  ██ Flush/Finalization ← 修复 P1-B
Phase 10 ██ 注入安全重构
Phase 11 ██ API Key 安全存储 ← 修复 P0-E
Phase 12 ██ 快捷键事务化 ← 修复 P1-D
Phase 13 ██ 状态与 UI 收敛
Phase 14 ██ 测试体系升级 ← 修复 P1-F
Phase 15 ██ 性能验证
Phase 16 ██ 异常恢复测试
Phase 17 ██ 清理旧架构
Phase 18 ██ 文档同步
```

---

## 6. 架构锁（Architecture Locks）

以下规则一旦确定，后续不允许推翻：

| 锁 | 规则 |
|---|---|
| **A** | Recording Session 是录音生命周期唯一权威状态源 |
| **B** | Audio callback 永远不等待网络/ASR |
| **C** | 音频队列必须 bounded |
| **D** | Provider event 必须先映射到统一 AsrEvent |
| **E** | UI 不直接理解 provider-specific event |
| **F** | 只有 utterance final 才允许自动注入 |
| **G** | flush 必须基于 completion/event，不基于固定 sleep |
| **H** | API Key 必须使用 OS secure storage |
| **I** | 所有 session 必须拥有唯一 session_id |
| **J** | 每个后台任务必须可取消、可结束、可观察 |

---

## 7. 风险与未知项

| 风险 | 说明 | 缓解 |
|---|---|---|
| OpenAI 协议变更 | Realtime Transcription API 可能随版本变化 | 实现时查询最新官方文档 |
| Deepgram 版本选择 | nova-2 / enhanced / 不同模型参数不同 | 实现时确认目标模型 |
| sherpa-onnx API 稳定性 | 静态链接 1.13，升级需重新绑定 | 当前版本锁定 |
| win-text-inject 限制 | 某些 elevated app 可能无法注入 | 保留 typing fallback |
| CPAL 设备热插拔 | 录音中拔出麦克风行为未定义 | Phase 16 异常测试 |

---

## 8. 基线度量

| 指标 | 值 |
|---|---|
| 总代码行数 (Rust) | ~2500 (src-tauri/src) |
| 总代码行数 (TS/TSX) | ~3500 (src) |
| Tauri commands | 14 |
| ASR providers | 3 (1 local + 2 cloud) |
| 状态源数量（录音） | 5 (分散) |
| 显式状态机数量 | 0 |
| 单元测试 | 19 (仅 UI) |
| 核心链路测试 | 0 |
| Rust warnings | 5 (non_snake_case) |
| Rust errors | 0 |
