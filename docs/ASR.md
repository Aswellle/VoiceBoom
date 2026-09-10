# VoiceBoom AI — ASR 提供商文档

> 生成阶段：Phase 18 — 文档同步

---

## 1. 提供商概览

| 提供商 | 文件 | 传输 | 类型 |
|---|---|---|---|
| 本地 SenseVoice | `adapters/local.rs` | 进程内 (sherpa-onnx) | 离线 |
| OpenAI Realtime | `adapters/openai_realtime.rs` | WebSocket | 云端 |
| Deepgram Streaming | `adapters/deepgram.rs` | WebSocket | 云端 |

### 引擎路由

| 前端 ID | Rust 枚举 | 实现 |
|---|---|---|
| `local_sense_voice` | `LocalSenseVoice` | sherpa-onnx SenseVoice |
| `openai_realtime` | `OpenAIRealtimeTranscription` | OpenAI WebSocket |
| `deepgram_streaming` | `DeepgramStreaming` | Deepgram WebSocket |

---

## 2. 本地 SenseVoice

### 架构

```
send_audio(samples)
  │  buffer.extend_from_slice(samples)
  ▼
receive_result() (每帧调用)
  │  1. 按 VAD_WINDOW_SIZE(512) 喂入 Silero VAD
  │  2. 检测 speech onset
  │  3. 每 200ms interim decode（仅最近 3s 窗口）
  │  4. VAD segment 完成 → final decode
  ▼
flush()
  │  vad.flush() → 处理剩余 segment
  │  fallback: 解码原始 buffer
```

### 配置参数 (`LocalAsrTuning`)

| 参数 | 默认值 | 含义 |
|---|---|---|
| `vad_window_size` | 512 samples | VAD 处理窗口 (32ms @16kHz) |
| `min_silence_ms` | 300ms | 判定语音结束的静默时长 |
| `min_speech_ms` | 250ms | 触发语音检测的最短时长 |
| `max_speech_ms` | 8000ms | 最大连续语音时长 |
| `partial_interval_ms` | 200ms | interim 解码间隔 |
| `max_working_buffer_ms` | 3000ms | 有界工作缓冲区 |

### 性能特点
- 离线运行，无需网络
- 首次加载 ~240MB 模型（复用避免重载）
- Interim 解码仅处理最近 3s 音频（分块感知）

---

## 3. OpenAI Realtime Transcription

### 协议

- **URL**: `wss://api.openai.com/v1/realtime?model=gpt-4o-transcribe`
- **认证**: `Authorization: Bearer <key>` + `OpenAI-Beta: realtime=v1`

### 客户端事件（发送）

| 事件 | 用途 |
|---|---|
| `session.update` | 配置 model, audio format, language, VAD |
| `input_audio_buffer.append` | 发送 base64 PCM16 音频 |
| `input_audio_buffer.commit` | 提交缓冲区 |
| `response.create` | 请求生成响应 |

### 服务端事件（接收）

| 事件 | 映射 |
|---|---|
| `session.created` | 会话建立 |
| `response.output_text.delta` | `AsrEvent::Partial` |
| `response.text.done` | `AsrEvent::UtteranceFinal` |
| `response.done` | 响应完成 |
| `error` | `AsrEvent::Error` |

---

## 4. Deepgram Streaming

### 协议

- **URL**: `wss://api.deepgram.com/v1/listen?model=nova-3&encoding=linear16&...`
- **认证**: `Authorization: Token <key>` header

### URL 参数

| 参数 | 值 |
|---|---|
| model | nova-3 |
| encoding | linear16 |
| sample_rate | 16000 |
| channels | 1 |
| interim_results | true |
| endpointing | 800 (ms) |
| utterance_end_ms | 1000 |
| vad_events | true |
| smart_format | true |

### 事件解析

| Deepgram 事件 | 映射 |
|---|---|
| `Results` (is_final=false) | `AsrEvent::Partial` |
| `Results` (is_final=true, speech_final=false) | `AsrEvent::SegmentFinal` |
| `Results` (speech_final=true) | `AsrEvent::UtteranceFinal` |
| `Error` | `AsrEvent::Error` |

### KeepAlive
- 每 30s 发送 `{"type": "KeepAlive"}` 文本帧
- **不是** WebSocket Ping 控制帧

---

## 5. 统一事件模型

所有 provider 输出映射到：

```rust
pub enum AsrEvent {
    Partial { text: String, language: Option<String> },
    SegmentFinal { text: String, language: Option<String>, confidence: Option<f64> },
    UtteranceFinal { text: String, language: Option<String>, confidence: Option<f64> },
    Error { code: String, message: String, retryable: bool },
}
```

### 事件语义

| 事件 | 含义 | 触发注入？ |
|---|---|---|
| **Partial** | 中间结果，文本仍在变化 | ❌ |
| **SegmentFinal** | 当前片段稳定，说话人可能继续 | ❌ |
| **UtteranceFinal** | 一次完整讲话结束 | ✅ |
| **Error** | 识别出错 | ❌ |

---

## 6. Transcript Aggregator

解决 partial/final 重复、覆盖和注入问题：

```
Partial → replace currentPartial
SegmentFinal → commit segment, clear currentPartial
UtteranceFinal → commit utterance, mark injection-ready
```

**关键规则**：只有 `UtteranceFinal` 才允许自动注入（Architecture Lock F）。
