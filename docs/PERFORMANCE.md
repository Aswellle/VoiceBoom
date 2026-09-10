# VoiceBoom AI — 性能文档

> 生成阶段：Phase 18 — 文档同步

---

## 1. 延迟测量点

```
t0 麦克风捕获 → t1 队列入队 → t2 队列出队 → t3 提供商发送
    → t4 提供商中间结果 → t5 前端事件 → t6 注入开始 → t7 注入完成
```

### 关键指标

| 指标 | 描述 |
|---|---|
| capture → partial | 麦克风到第一个中间结果 |
| capture → final | 麦克风到最终结果 |
| capture → injection | 麦克风到注入完成 |

---

## 2. 性能指标

### 音频管线

| 参数 | 值 |
|---|---|
| 采样率 | 16kHz mono f32 |
| 队列容量 | ~8 frames ≈ 500ms |
| 队列策略 | 满时丢弃最旧帧（实时优先） |
| CPAL callback | 永不阻塞 |

### 本地 SenseVoice

| 参数 | 默认值 |
|---|---|
| VAD 窗口 | 512 samples (32ms) |
| Interim 间隔 | 200ms |
| 工作缓冲区 | 3s（有界） |
| 最大语音段 | 8s |

---

## 3. 验收目标

> 这些数字是工程验收目标，不是对具体硬件的绝对保证。
> 必须在目标硬件上进行实际测试后再写入 README。

| 指标 | 目标 |
|---|---|
| P95 capture → partial | < 500ms |
| P95 capture → injection | < 1200ms |
| 队列深度 | bounded |
| 30s 连续口述 | 无持续延迟增长 |

---

## 4. 运行性能诊断

```bash
# 获取性能指标（需先运行录音）
invoke('get_performance_metrics')
```

### 实现位置

- `src-tauri/src/asr/latency.rs` — 延迟追踪器
- `src-tauri/src/commands/mod.rs::get_performance_metrics` — Tauri 命令
