# VoiceBoom AI — 测试文档

> 生成阶段：Phase 18 — 文档同步

---

## 1. 测试策略

### 双层测试

| 层 | 工具 | 运行在 | 范围 |
|---|---|---|---|
| 单元/组件 | Vitest 4.11 + jsdom 30 | Node (mocked Tauri) | Store 逻辑、组件渲染/交互、a11y |
| 集成 | Rust `#[cfg(test)]` | Native | ASR 适配器、状态机、聚合器、故障恢复 |
| 桌面 E2E | `scripts/e2e_smoke.mjs` (tauri-driver + msedgedriver) | 真实构建应用 | App 启动、控制、文本注入 |

---

## 2. Rust 测试

### 运行

```bash
cd src-tauri && cargo test --lib
```

### 测试模块

| 模块 | 文件 | 测试数 | 覆盖 |
|---|---|---|---|
| session | `asr/session.rs` | 9 | RecordingSession 状态机 |
| pipeline | `audio/pipeline.rs` | 8 | AudioFrame + bounded channel |
| aggregator | `asr/aggregator.rs` | 13 | Transcript 去重/提交 |
| deepgram | `asr/adapters/deepgram.rs` | 13 | 事件解析 |
| openai | `asr/adapters/openai_realtime.rs` | 10 | 事件解析 |
| secure_keystore | `secure_keystore.rs` | 5 | 密钥存储 |
| streaming | `asr/streaming.rs` | 5 | AsrManager + flush |
| latency | `asr/latency.rs` | 4 | 延迟追踪 |
| integration | `asr/integration_tests.rs` | 15 | 全链路集成 |
| failure | `asr/failure_tests.rs` | 18 | 故障注入与恢复 |
| **总计** | | **100** | |

### 关键测试场景

#### 状态机 (session.rs)
- 完整生命周期 (Idle → Starting → Recording → Stopping → Finalizing → Idle)
- 重复启动拒绝
- 错误恢复
- 无效转换拒绝

#### 有界音频 (pipeline.rs)
- 队列容量计算
- 满队列时丢弃
- FIFO 顺序保持

#### 聚合器 (aggregator.rs)
- Partial 替换（无 AA 重复）
- SegmentFinal 提交
- UtteranceFinal 触发注入
- Gate 8 场景：A partial, A partial2, A segment final, B partial, B final

#### 故障恢复 (failure_tests.rs)
- 音频设备故障
- 网络断开 / API 401/429/5xx
- 模型文件缺失
- 快捷键冲突
- 权限不足
- 快速启停循环 (10x)
- 多次连续故障

---

## 3. 前端测试 (Vitest)

### 运行

```bash
bun run test        # 运行一次
bun run test:watch  # 监听模式
bun run test:ui     # Web UI
```

### 测试文件

| 文件 | 测试数 | 覆盖 |
|---|---|---|
| store.test.ts | 10 | Zustand store 逻辑 |
| components.test.tsx | 9 | FloatingWindow 渲染/交互 |

### 关键测试场景
- M8 re-entrant guard
- settings persistence + maxChars side-effect
- segment budgeting
- toast auto-dismiss
- SegmentItem render/a11y/clipboard
- FloatingWindow controls/engine hint/listening toggle
- resize-on-content
- scroll-to-bottom FAB

---

## 4. E2E 测试

### 运行

```bash
# 先构建单窗口测试变体
tauri build --config src-tauri/tauri.test.conf.json

# 运行 E2E
node scripts/e2e_smoke.mjs
```

### 前提
- `tauri-driver` 已安装 (`D:\cargo\bin\tauri-driver.exe`)
- `msedgedriver` 已安装 (`D:\msedgedriver\`)

### 覆盖范围
- App 启动
- 引擎标签
- 开始/停止按钮
- 设置按钮

### 不覆盖（需要真实麦克风/OS 循环）
- 全局热键
- 实时音频采集
- ASR 转录
- 桌面拖拽

---

## 5. 测试门禁

### Gate 0-18

每个 Phase 都有门禁条件，必须通过才能进入下一阶段。

| Gate | 条件 |
|---|---|
| Gate 0 | 7/7 架构问题已回答 |
| Gate 1 | build/test/clippy 通过 |
| Gate 2 | start/duplicate start/stop/failure 测试 |
| Gate 3 | queue bounded/callback never blocks/stop exits |
| Gate 4 | provider event → AsrEvent 映射 |
| Gate 5 | provider event fixtures + parser 测试 |
| Gate 6 | mock WS server + connect/configure/partial/final/error 测试 |
| Gate 7 | 短句/长句/连续说话测试 |
| Gate 8 | A partial/A partial2/A segment final/B partial/B final 无重复 |
| Gate 9 | no audio/short/long/slow provider/timeout 测试 |
| Gate 10 | browser/input/Electron/IDE/clipboard/elevated/no-focus 测试 |
| Gate 11 | key not in sqlite/logs/URLs, restart works, delete cleans |
| Gate 12 | valid→valid/valid→invalid/conflicting/duplicate/restart 测试 |
| Gate 13 | 0 or 1 active session at any time |
| Gate 14 | unit + integration + audio + lifecycle + injection 测试 |
| Gate 15 | latency instrumentation + P50/P90/P95/P99 |
| Gate 16 | 故障恢复后状态正确 |
| Gate 17 | 命名一致（Frontend = Rust = UI = resource = history = docs） |
| Gate 18 | README 不声称 ≤500ms（除非测试支持） |
