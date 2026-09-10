# VoiceBoom AI — ASR 提供商基线 (ASR Provider Baseline)

> 生成阶段：Phase 0 — 源码逆向与锁定基线
> 核心发现：两个云端适配器都存在**协议层缺陷**，不能作为生产实现。

---

## 1. 提供商概览

| 提供商 | 文件 | 传输 | 当前状态 |
|---|---|---|---|
| 本地 SenseVoice | `adapters/local.rs` | 进程内 (sherpa-onnx) | ⚠ 可用但有性能问题 |
| OpenAI Whisper | `adapters/openai_whisper.rs` | WebSocket | ❌ 协议不可靠 (P0-A) |
| Deepgram | `adapters/deepgram.rs` | WebSocket | ⚠ 语义不完整 (P0-B) |

### 引擎路由 (`parse_engine_type`)

```rust
fn parse_engine_type(engine: &str) -> AsrEngineType {
    match engine {
        "openai_whisper" => AsrEngineType::OpenaiWhisper,
        "deepgram"      => AsrEngineType::Deepgram,
        "whisper_cpp"   => AsrEngineType::Funasr,   // ← 实际走本地 SenseVoice
        "funasr"        => AsrEngineType::Funasr,   // ← 实际走本地 SenseVoice
        _               => AsrEngineType::OpenaiWhisper,
    }
}
```

**问题**：`whisper_cpp` 和 `funasr` 名称与真实实现不一致（都走 sherpa-onnx SenseVoice）。

---

## 2. 统一结果模型

### 2.1 当前 `AsrResult`

```rust
pub struct AsrResult {
    pub text: String,
    pub is_final: bool,              // ← 仅二元
    pub language: Option<String>,
    pub confidence: Option<f64>,
}
```

### 2.2 ⚠ 关键缺陷：`is_final` 二元模型不足

当前模型只有 `is_final: bool`，无法区分：

| 语义 | 描述 | 应触发注入？ |
|---|---|---|
| **Partial** | 中间结果，仍在变化 | ❌ 否 |
| **SegmentFinal** | 当前片段稳定，但说话人可能继续 | ❌ 否 |
| **UtteranceFinal** | 一次完整讲话结束 | ✅ 是 |

**后果**：
- 本地引擎每个 VAD segment 都输出 `is_final=true` → 一段话被**多次注入**
- 云端 `is_final` 语义混用 → 可能把 "segment 稳定" 当成 "讲话结束"

---

## 3. 本地 SenseVoice 适配器

### 3.1 架构

```
send_audio(samples)
  │  buffer.extend_from_slice(samples)
  ▼
receive_result() (每帧调用)
  │
  ├─ 1. 按 VAD_WINDOW_SIZE(512) 喂入 Silero VAD
  ├─ 2. 检测 speech onset
  ├─ 3. 每 0.2s interim decode → is_final=false
  └─ 4. VAD segment 完成 → final decode → is_final=true
  ▼
flush()
  │  vad.flush() → 处理剩余 segment
  │  fallback: 解码原始 buffer
```

### 3.2 配置参数 (`cfg` 模块)

| 参数 | 值 | 含义 |
|---|---|---|
| `VAD_WINDOW_SIZE` | 512 samples | VAD 处理窗口 (32ms @16kHz) |
| `VAD_MIN_SILENCE` | 0.3s | 判定语音结束的静默时长 |
| `VAD_MIN_SPEECH` | 0.25s | 触发语音检测的最短时长 |
| `VAD_MAX_SPEECH` | 8.0s | 最大连续语音时长 |
| `INTERIM_INTERVAL` | 0.2s | interim 解码间隔 |

### 3.3 问题 P1-A：interim 全 buffer 重解码

```rust
// local.rs:219-243
if self.speech_active && self.last_interim.elapsed() > INTERIM_INTERVAL {
    // 每 0.2s 重新解码整个 buffer
    let stream = recognizer.create_stream();
    stream.accept_waveform(16000, &self.buffer);  // ← 整个 buffer
    recognizer.decode(&stream);
    ...
}
```

**问题**：随着语音持续，`buffer` 线性增长。每 200ms 对整个 buffer 做一次 OfflineRecognizer decode，计算量持续上升：

```
语音时长  │ buffer 大小 │ decode 成本
  2s     │   32000     │   1x
  5s     │   80000     │   2.5x
 10s     │  160000     │   5x
 30s     │  480000     │  15x  ← 明显延迟
```

**注意**：VAD segment 完成时会 `buffer.clear()`，所以实际增长受限于 VAD segment 长度（最大 8s）。但 8s 的 buffer = 128000 samples，每 200ms 重解码仍有延迟。

### 3.4 问题：interim 替换策略

当前 interim 是**替换**（replace）而非**追加**（append），这是正确的（避免 "世世界" 问题）。但 VAD final 会覆盖 interim，所以最终结果正确。

### 3.5 优点

- VAD 端点检测在 adapter 内部，bridge 不重复检测
- flush 有 `vad.flush()` + raw buffer 双重保障
- 模型复用（不重载 240MB）

---

## 4. OpenAI Whisper 适配器 (P0-A)

### 4.1 当前实现

```rust
// openai_whisper.rs:43-45
let endpoint = config.endpoint.clone().unwrap_or_else(|| {
    "wss://api.openai.com/v1/audio/transcriptions".to_string()
});
```

### 4.2 ⚠ 协议问题

| 问题 | 详情 |
|---|---|
| **错误的 endpoint** | `wss://api.openai.com/v1/audio/transcriptions` 是 **HTTP REST API** 的 URL 格式，不是 WebSocket realtime endpoint |
| **缺少 session 配置** | 现代 OpenAI Realtime Transcription 需要：创建 session → 发送 session.update → 发送 audio → 接收 delta → 接收 completion |
| **直接写 PCM Binary** | 代码直接向 socket 写 PCM16 binary，没有 session 握手 |
| **假设响应格式** | 假设响应是 `{"type":"final","text":"..."}`，这不是 OpenAI 的协议 |
| **缺少 audio_realtime.delta 事件** | OpenAI realtime 通过 `conversation.item.input_audio_transmission` / `transcription` 事件返回结果 |

### 4.3 当前响应解析

```rust
// openai_whisper.rs:98-109
let is_final = json["type"] == "final";
let text_content = json["text"].as_str().unwrap_or("").to_string();
```

**问题**：OpenAI Realtime Transcription 不使用 `{"type":"final"}` 格式。实际事件包括：
- `session.created`
- `session.updated`
- `conversation.item.input_audio_transmission`
- `conversation.item.transcript.partial`
- `conversation.item.transcript.completed`
- `response.completion`

### 4.4 当前 flush

```rust
// openai_whisper.rs:151-157
async fn flush(&mut self) -> anyhow::Result<Option<AsrResult>> {
    sender.send(Vec::new()).ok();           // 空 Vec = flush 信号
    sleep(500ms).await;                       // ⚠ 固定时延
    self.receive_result().await
}
```

**问题**：
- 固定 500ms 太短（可能丢最后结果）或太长（增加延迟）
- 只读取一个 result，可能有多条 pending
- 没有发送 session completion 信号

### 4.5 认证

```rust
.header("Authorization", format!("Bearer {}", api_key))
.header("OpenAI-Beta", "realtime-v1")
```

认证方式正确（Bearer <REDACTED>），但 `OpenAI-Beta: realtime-v1` 是旧版头。

### 4.6 结论

**当前 OpenAI 适配器不能作为生产实现。** 需要完全重写为：
1. 创建 Realtime Transcription Session
2. 发送 session 配置
3. 发送 audio delta
4. 接收 partial/delta/completed 事件
5. 正确 finalize

---

## 5. Deepgram 适配器 (P0-B)

### 5.1 当前实现

```rust
// deepgram.rs:43-45
let endpoint = config.endpoint.clone().unwrap_or_else(|| {
    "wss://api.deepgram.com/v1/listen".to_string()
});
```

### 5.2 ⚠ 协议问题

| 问题 | 详情 |
|---|---|
| **URL 参数不完整** | 当前只有 `encoding`、`sample_rate`、`channels`、`language`、`token` |
| **缺少关键参数** | `model`、`interim_results`、`endpointing`、`utterance_end_ms`、`vad_events` |
| **token 在 URL 中** | API key 拼接进 URL (`&token=...`)，可能出现在日志中 |
| **is_final / speech_final 未区分** | 当前只读 `channel.is_final`，未处理 `channel.speech_final` |

### 5.3 当前 URL 构建

```rust
// deepgram.rs:52-63
let mut url_str = format!(
    "{}?encoding=linear16&sample_rate={}&channels=1",
    endpoint, sample_rate
);
if !api_key.is_empty() {
    url_str.push_str(&format!("&token={}", api_key));  // ← key 在 URL
}
```

### 5.4 当前响应解析

```rust
// deepgram.rs:107-127
if json["type"] == "Results" {
    let channel = &json["channel"];
    let alternatives = channel["alternatives"].as_array();
    if let Some(alt) = alternatives.and_then(|a| a.first()) {
        let transcript = alt["transcript"].as_str().unwrap_or("").to_string();
        let is_final = channel["is_final"].as_bool().unwrap_or(false);  // ← 仅此字段
        ...
    }
}
```

### 5.5 ⚠ is_final vs speech_final

Deepgram 有两个不同语义的字段：

| 字段 | 含义 | 应触发注入？ |
|---|---|---|
| `is_final` | 当前结果不再变化（segment 稳定） | ❌ 不一定 |
| `speech_final` | 检测到说话人结束（utterance 结束） | ✅ 更接近注入时机 |

**当前代码只读 `is_final`，可能把 "segment 稳定" 当成 "讲话结束"。**

### 5.6 当前 flush

```rust
// deepgram.rs:168-174
async fn flush(&mut self) -> anyhow::Result<Option<AsrResult>> {
    sender.send(Vec::new()).ok();           // 空 Vec = flush 信号 → {"type":"Flush"}
    sleep(500ms).await;                       // ⚠ 固定时延
    self.receive_result().await
}
```

**问题**：同 OpenAI，固定 500ms + 只读一个 result。

### 5.7 结论

**当前 Deepgram 适配器需要重写**，至少：
1. 添加完整 URL 参数（model、interim_results、endpointing）
2. 区分 `is_final` 与 `speech_final`
3. 使用 Authorization header 而非 URL token
4. 基于 completion event 的 finalize（非 sleep）

---

## 6. 云端适配器共同问题

### 6.1 无界内部通道

两个云端 adapter 都使用：
```rust
let (tx_audio, mut rx_audio) = mpsc::unbounded_channel::<Vec<f32>>();
let (tx_result, rx_result) = mpsc::unbounded_channel::<AsrResult>();
```

**问题**：WS 发送速度受网络限制时，audio channel 无限积压。

### 6.2 无 CancellationToken

WS task 没有 CancellationToken，只能通过 drop sender 间接触发退出。无法：
- 超时取消
- 主动中断
- 等待 task 结束

### 6.3 无 JoinHandle 管理

`tokio::spawn` 的 JoinHandle 被丢弃，无法：
- 确认 task 已退出
- 等待 task 结束
- 检测 task 异常

### 6.4 无 session_id

当前 adapter 没有 session 概念，无法追踪：
- 哪个录音 session 产生了结果
- 结果是否属于已结束的 session

---

## 7. 目标 ASR 抽象（Phase 4 方向）

### 7.1 推荐 trait

```rust
pub trait AsrSession {
    async fn start(&mut self) -> Result<()>;
    async fn push_audio(&mut self, frame: &[f32]) -> Result<()>;
    async fn next_event(&mut self) -> Result<Option<AsrEvent>>;
    async fn finalize(&mut self) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}
```

### 7.2 统一事件

```rust
pub enum AsrEvent {
    Partial { text: String, language: Option<String> },
    SegmentFinal { text: String, language: Option<String>, confidence: Option<f64> },
    UtteranceFinal { text: String, language: Option<String>, confidence: Option<f64> },
    Error { code: String, message: String, retryable: bool },
}
```

### 7.3 Provider → 统一事件映射

| Provider | Partial | SegmentFinal | UtteranceFinal |
|---|---|---|---|
| LocalSenseVoice | interim decode | VAD segment final | flush final |
| OpenAI Realtime | transcript.partial | — | transcript.completed |
| Deepgram | interim transcript | is_final=true | speech_final=true |

---

## 8. Provider 测试策略

### 8.1 需要构造的 Fixtures

```
fixtures/asr/
├── deepgram/
│   ├── partial.json
│   ├── segment_final.json
│   ├── utterance_final.json
│   ├── error.json
│   └── close.json
└── openai/
    ├── session_created.json
    ├── transcript_partial.json
    ├── transcript_completed.json
    └── error.json
```

### 8.2 Mock WebSocket Server

Phase 5/6 需要 mock WS server 测试：
- connect → configure → send audio → partial → final → error → disconnect → flush → shutdown
