# VoiceBoom AI — 开发指南

> 生成阶段：Phase 18 — 文档同步

---

## 快速开始

```bash
# 安装依赖
bun install

# 开发模式（热重载）
bun run tauri:dev

# 构建生产版本
bun run tauri:build

# 类型检查
./node_modules/.bin/tsc --noEmit

# Rust 检查
cd src-tauri && cargo check

# Rust 测试
cd src-tauri && cargo test --lib

# 前端测试
bun run test
```

---

## 项目架构

```
VoiceBoom/
├── src/                      # React 19 前端
│   ├── components/
│   │   ├── FloatingWindow/   # 毛玻璃悬浮窗（核心 UI）
│   │   ├── Waveform/         # 音频波形可视化
│   │   ├── Settings/         # 设置面板（7 标签页）
│   │   └── Shared/           # 通用 UI 组件
│   ├── stores/useAppStore.ts # Zustand 全局状态
│   ├── hooks/
│   │   ├── useAsr.ts         # ASR 生命周期 + 事件订阅
│   │   └── useGlobalShortcut.ts # 全局快捷键
│   └── test/                 # Vitest 测试 + setup
├── src-tauri/                # Tauri 2.0 + Rust 后端
│   ├── src/
│   │   ├── asr/              # ASR 引擎抽象 + 适配器
│   │   │   ├── adapters/     # Deepgram / OpenAI / Local
│   │   │   ├── session.rs    # RecordingSession 状态机
│   │   │   ├── streaming.rs  # AsrManager
│   │   │   ├── aggregator.rs # TranscriptAggregator
│   │   │   └── latency.rs    # 延迟追踪
│   │   ├── audio/            # CPAL 音频采集 + 有界管线
│   │   ├── commands/         # Tauri command handlers
│   │   ├── db/               # SQLite 数据库
│   │   ├── inject.rs         # 跨平台文本注入
│   │   ├── secure_keystore.rs # OS 安全密钥存储
│   │   ├── shortcut/         # 全局快捷键管理
│   │   └── resources/        # ONNX 模型路径解析
│   └── vendor/               # win-text-inject + enigo (path deps)
└── docs/                     # 文档
```

---

## ASR 引擎

| 引擎 | 前端 ID | 类型 | 需要 Key |
|---|---|---|---|
| 本地 SenseVoice | `local_sense_voice` | 离线 | ❌ |
| OpenAI Realtime | `openai_realtime` | 云端 | ✅ |
| Deepgram Streaming | `deepgram_streaming` | 云端 | ✅ |

### 引擎切换

前端通过 `invoke('switch_engine', { engine: 'local_sense_voice' })` 切换引擎。

---

## 开发命令

```bash
# 前端
bun install              # 安装依赖
bun run dev              # 前端开发服务器（浏览器）
bun run build            # 类型检查 + 生产构建
bun run test             # Vitest 测试

# Rust
cd src-tauri
cargo check              # 快速编译检查
cargo test --lib         # 单元/集成测试
cargo clippy             # 代码质量检查

# 完整应用
bun run tauri:dev        # 开发模式（热重载）
bun run tauri:build      # 生产构建 → .msi/.exe (Win) / .dmg (macOS)
```

---

## 测试

```bash
# 前端测试
bun run test             # 运行一次
bun run test:watch       # 监听模式
bun run test:ui          # Web UI

# Rust 测试
cd src-tauri && cargo test --lib

# E2E 测试
tauri build --config src-tauri/tauri.test.conf.json
node scripts/e2e_smoke.mjs
```

---

## 文档

| 文档 | 内容 |
|---|---|
| `docs/ARCHITECTURE.md` | 系统架构 + 数据链路 |
| `docs/ASR.md` | ASR 提供商协议 + 事件模型 |
| `docs/TESTING.md` | 测试策略 + 门禁 |
| `docs/SECURITY.md` | 安全存储 + 架构锁 |
| `docs/PERFORMANCE.md` | 延迟测量 + 验收目标 |
| `docs/AI_REVERSE_ENGINEERING.md` | 源码逆向报告 |
| `docs/ARCHITECTURE_BASELINE.md` | 架构基线 |
| `docs/STATE_MACHINE_BASELINE.md` | 状态机基线 |
| `docs/ASR_PROVIDER_BASELINE.md` | ASR 提供商基线 |
| `docs/BASELINE_REPORT.md` | 基线报告 |

---

## 快捷键

默认快捷键：`Ctrl+Space`（可在设置中修改）。

- 按住：开始录音
- 松开：停止录音 + 注入文本

---

## 已知限制

- 本地 SenseVoice 模型首次加载约 240MB
- 云端引擎需要网络连接
- Windows 上某些 elevated app 可能无法注入（UIPI 限制）
- macOS/Linux 注入依赖剪贴板
