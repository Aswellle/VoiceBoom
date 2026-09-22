# 🎙️ VoiceBoom

[![Release](https://img.shields.io/github/v/release/Aswellle/VoiceBoom)](https://github.com/Aswellle/VoiceBoom/releases/latest)
[![CI](https://github.com/Aswellle/VoiceBoom/actions/workflows/ci.yml/badge.svg)](https://github.com/Aswellle/VoiceBoom/actions/workflows/ci.yml)
[![Tauri](https://img.shields.io/badge/Tauri-2.11-9C27F0?logo=tauri)](https://v2.tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.88-EA5800?logo=rust)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react)](https://react.dev)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-0078D6)](https://github.com/Aswellle/VoiceBoom/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20%2B%20non--commercial-important)](./LICENSE)

**简体中文 | [English](./README.en.md)**

**实时流式智能语音输入法** — Real-time Streaming Voice Input Method

> 像微信语音转文本、iOS 键盘听写一样：按住热键说话，松手后文字**直接出现在光标所在的输入框**，无需手动复制粘贴。

一个面向 Windows / macOS 的低延迟实时语音转文字输入工具。按住全局快捷键唤醒麦克风，连续自然讲话，语音流实时转为文字，在毛玻璃悬浮窗中展示最新识别结果，并**自动注入到当前焦点输入框**。默认引擎为内置的离线 SenseVoice 模型，**无需 API Key 即可开箱使用**。

---

## 下载

从 [Releases](https://github.com/Aswellle/VoiceBoom/releases/latest) 获取安装包：

- **Windows**：`.msi` 安装包；或便携版 ZIP（需系统已安装 WebView2 Runtime）
- **macOS**：Universal `.dmg`（Apple Silicon 与 Intel 通用），另附 App `.zip`

每个发行版附带 `SHA256SUMS.txt` 供校验。

| 平台 | 要求 |
|---|---|
| Windows | Windows 10/11 + WebView2 Runtime |
| macOS | macOS 12 (Monterey) 及以上 |

---

## 核心特性

- 🎯 **直接注入输入框** — 转写文字自动出现在光标位置（微信/iOS 听写体验），非仅悬浮窗展示
- ⚡ **实时流式识别** — 流式 ASR 边说边出字（延迟取决于引擎和网络）
- 🔌 **可插拔 ASR 引擎** — 本地离线 SenseVoice（内置）/ OpenAI Realtime / Deepgram Streaming；设置面板提供 **自动 / 离线 / 云端** 三种引擎模式
- 🔁 **云端自动回退** — 自动模式下优先本地，本地不可用时回退到首个已配置的云端提供者
- 📦 **模型管理** — 多版本共存与切换、断点续传下载、SHA-256 校验、原子安装（中断不会破坏已装版本）
- 🎨 **系统 HUD 风格** — 低存在感设计，三态显示（Idle / Listening / Result），显示密度可调（紧凑 / 标准 / 展开）
- ⌨️ **全局快捷键** — 按住说话，松开停止，支持 Ctrl / Alt / Shift / Cmd 组合
- 🎛️ **上屏模式** — 直接上屏 / 确认上屏 / 仅识别，可按场景配置
- 🔒 **注入安全** — Windows 延迟渲染技术：不泄露剪贴板历史、不破坏用户剪贴板、UIPI 自动降级
- 🗂️ **历史记录** — 可搜索的历史面板，支持复制、全选与清空
- 🌐 **多语言支持** — 中/英/日/韩 自动检测与切换
- 🧪 **双层测试** — 149 项 Rust 测试 + 19 项 Vitest 前端测试
- 🪶 **轻量级** — Tauri 2，Rust 后端无 Node.js 运行时依赖

---

## 技术栈

| 模块 | 选型 |
|------|------|
| 桌面框架 | Tauri 2.11 (Rust) |
| 前端 UI | React 19 + TypeScript + Tailwind CSS v3 |
| 动画 | Framer Motion（含 `prefers-reduced-motion` 支持） |
| 状态管理 | Zustand |
| 音频采集 | CPAL（原生采样率 + 重采样至 16 kHz 单声道 f32） |
| 本地 ASR | sherpa-onnx 1.13.8 (SenseVoice + Silero VAD) |
| 云端 ASR | OpenAI Realtime / OpenAI Whisper / Deepgram (WebSocket) |
| 文本注入 | win-text-inject (Windows) / enigo (跨平台) |
| 数据库 | SQLite (rusqlite, bundled) |
| 凭证存储 | DPAPI (Windows) / 0600 权限文件 (macOS, Linux) |

> 版本由 `rust-toolchain.toml`（Rust 1.88.0）与 `config/sherpa-onnx.json`（sherpa-onnx 1.13.8，含 SHA-256）固定；构建全程使用 `--locked`。

---

## 快速开始

### 前置要求

- [Rust](https://www.rust-lang.org/tools/install) **1.88.0**（由 `rust-toolchain.toml` 固定，rustup 会自动安装）
- [Bun](https://bun.sh/) 1.2+（首选）或 Node.js 20+
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) 2.11+
- 平台构建依赖：见 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### 安装依赖

```bash
bun install
```

### 开发模式

```bash
bun run tauri:dev        # 完整开发：前端热重载 + Rust 编译/启动
bun run dev              # 仅前端开发服务器（浏览器，无 Tauri 壳）
```

### 构建生产版本

```bash
bun run tauri:build      # → src-tauri/target/release/bundle/{msi,nsis}/
```

> **关于 sherpa-onnx 原生库**：首次构建时构建脚本会**自动下载**对应平台的原生库归档。若需离线或可复现构建，可先预取并校验（避免依赖网络）：
>
> ```bash
> # Windows
> powershell -ExecutionPolicy Bypass -File scripts/prepare-sherpa-onnx.ps1
> # macOS / Linux / CI
> python3 scripts/fetch-sherpa-archive.py
> ```
>
> 脚本按 `config/sherpa-onnx.json` 固定版本并校验 SHA-256。`bun run tauri:build:windows` 已内置 Windows 预取步骤。

---

## 测试

项目采用两层测试策略。由于前端强依赖 Tauri API，无法在纯浏览器中渲染，因此**单元/组件测试在 jsdom 中通过 mock Tauri 层运行**，**端到端测试通过 tauri-driver 驱动真实桌面窗口**。

### 单元与组件测试（Vitest）

```bash
bun run test                # 运行一次（19 个测试）
bun run test:watch          # 监听模式
bun run test:ui             # Web UI 界面
```

覆盖内容（`src/test/`）：
- `useAppStore` — 重入守卫、设置持久化与迁移、maxChars 预算裁剪、toast 定时
- `SegmentItem` — 渲染、无障碍语义（role/aria-label）、clipboard 复制 + textarea 降级
- `FloatingWindow` — 控件渲染、录音状态切换、按内容高度自动调窗、最新内容优先显示

### Rust 测试

```bash
cargo test --locked         # 149 个测试
cargo test --locked --lib   # 同上（仅 lib target）
```

覆盖内容：
- **`asr/integration_tests.rs`** — 会话生命周期、Deepgram/OpenAI 事件解析契约、flush 与终结、聚合器（空/部分/多句/乱序/重复防护）
- **`asr/failure_tests.rs`** — 故障注入与恢复：错误释放资源、重复启动拦截、10× 快速启停、音频设备失败、网络断开、401/429、模型缺失、快捷键冲突、畸形响应
- **`asr/pipeline_tests.rs`** — 音频管线与假音源驱动的端到端流程
- 其余内联测试 — 注入控制器与目标校验、模型校验/安装、提供者解析与自动回退、发行形态检测、数据库操作

### 端到端测试（tauri-driver）

```bash
bun run tauri:build:test    # 构建单窗口测试变体（仅 floating 窗口）
bun run test:e2e            # 启动 tauri-driver + msedgedriver，连接真实应用
```

E2E 覆盖：应用启动、引擎标签、开始/停止按钮、设置按钮。

> **不覆盖**（需真实麦克风 / 系统消息循环 / WebView2）：全局快捷键、实际录音、ASR 转写、桌面拖拽。

### CI

`.github/workflows/ci.yml` 在每次推送与 PR 上运行四个作业：

| 作业 | 内容 |
|---|---|
| Frontend | 类型检查 + Vitest |
| Rust | `cargo fmt --check` + clippy (`-D warnings`) + 测试 + 离线构建校验 |
| macOS | `cargo check --all-targets`（编译 macOS 专属 `cfg` 分支） |
| Windows | clippy + 测试 + Tauri JS/Rust 版本对齐检查 + 离线构建校验 |

---

## 项目结构

```
VoiceBoom/
├── src/                          # React 19 前端 (TypeScript + Tailwind CSS)
│   ├── components/
│   │   ├── FloatingWindow/       # 悬浮窗核心（HUD 三态、最新优先、自动调高）
│   │   ├── Settings/             # 设置面板（6 个分区，含模型与云端提供者）
│   │   │   ├── index.tsx
│   │   │   └── controls.tsx      # 统一表单原语（Slider/Select/TextInput）
│   │   ├── HistoryPanel/         # 识别历史（搜索、复制、清空）
│   │   └── Waveform/             # 音频波形可视化（Canvas，12 段）
│   ├── hooks/
│   │   ├── useAsr.ts             # ASR 录音生命周期 + 结果订阅
│   │   └── useGlobalShortcut.ts  # 全局快捷键推麦
│   ├── stores/
│   │   └── useAppStore.ts        # Zustand 全局状态（设置/结果/模型/提供者/UI）
│   ├── constants/
│   │   └── engines.ts            # 引擎元数据单一来源
│   ├── utils/                    # clipboard 等工具
│   ├── styles/                   # 全局样式 + 语义化设计令牌
│   ├── test/                     # Vitest 测试套件 + Tauri mock
│   ├── App.tsx                   # 根路由（按窗口标签分发）+ 托盘/快捷键事件
│   └── main.tsx                  # React 19 入口 + ErrorBoundary
├── src-tauri/                    # Tauri 2 Rust 后端
│   ├── src/
│   │   ├── asr/                  # ASR 抽象 + 适配器
│   │   │   ├── adapters/         # local(SenseVoice) / openai_realtime / openai_whisper / deepgram
│   │   │   ├── engine_trait.rs   # StreamingAsrEngine trait
│   │   │   ├── session.rs        # AsrSession trait + AsrEvent 统一事件模型
│   │   │   ├── streaming.rs      # AsrManager（本地引擎常驻复用）
│   │   │   ├── aggregator.rs     # TranscriptAggregator（仅 UtteranceFinal 触发注入）
│   │   │   └── *_tests.rs        # 集成 / 故障注入 / 管线测试
│   │   ├── audio/                # CPAL 采集 + 有界实时管线 + 假音源（测试）
│   │   ├── commands/             # 31 个 Tauri 命令 + 会话状态机
│   │   ├── models/               # 模型管理：注册表、下载器、校验器、原子安装器
│   │   ├── provider/             # 云端提供者：配置、凭证、注册表与自动回退
│   │   ├── injection/            # 注入控制器 + 平台适配器 + 假注入目标（测试）
│   │   ├── inject.rs             # 跨平台文本注入调度
│   │   ├── secure_keystore.rs    # 凭证存储（DPAPI / 0600 文件）
│   │   ├── session.rs            # 录制会话状态机
│   │   ├── shortcut/             # 全局快捷键 + 平台默认值
│   │   ├── db/                   # SQLite（设置/历史/快捷键 + schema 迁移）
│   │   ├── resources/            # 模型路径解析 + 发行形态检测
│   │   ├── tray/                 # 系统托盘菜单
│   │   ├── lib.rs                # AppState + 命令注册 + 托盘 + 文件日志
│   │   └── main.rs               # 入口（windows_subsystem）
│   ├── vendor/                   # 内联 crate 源码（path 依赖）
│   │   ├── win-text-inject/      # Windows 延迟渲染剪贴板注入
│   │   └── enigo/                # 跨平台键入模拟
│   ├── capabilities/             # Tauri 权限配置
│   ├── tools/asr_debug.rs        # ASR 调试工具
│   ├── tauri.conf.json           # 窗口定义 + 构建配置
│   ├── tauri.offline.conf.json   # 离线打包配置（捆绑 asr-bundle）
│   └── tauri.test.conf.json      # 单窗口 E2E 测试配置
├── config/
│   └── sherpa-onnx.json          # 原生库版本、归档名与 SHA-256（单一来源）
├── models/
│   └── registry.json             # 模型注册表（编译期内嵌）
├── scripts/
│   ├── prepare-sherpa-onnx.ps1   # Windows：预取并校验原生库归档
│   ├── fetch-sherpa-archive.py   # macOS/Linux/CI：同上
│   ├── prepare-models.py         # CI：打包模型发行归档
│   ├── verify-models.py          # CI：校验模型归档完整性
│   ├── install-model-pack.py     # CI：为离线构建安装模型包
│   ├── merge-macos-universal.sh  # lipo 合并 arm64 + x64 为 Universal
│   └── e2e_smoke.mjs             # tauri-driver 端到端冒烟测试
├── docs/                         # ARCHITECTURE / ASR / DEVELOPMENT / PERFORMANCE / SECURITY / TESTING
├── .github/workflows/            # ci.yml / release.yml / model-release.yml
├── AGENTS.md                     # 架构与改动约束速查（面向贡献者）
├── rust-toolchain.toml           # 固定 Rust 1.88.0
└── package.json                  # 依赖 + 脚本
```

---

## 文本注入技术

语音转写完成后，文字自动注入到当前焦点输入框。技术实现：

| 平台 | 默认模式（Clipboard） | 备选模式（Typing） |
|---|---|---|
| **Windows** | `win-text-inject` 延迟渲染剪贴板注入 | `enigo` 逐字键入 |
| **macOS / Linux** | `enigo` 剪贴板+粘贴 | `enigo` 逐字键入 |

### Windows 注入为何特殊

朴素方案（保存剪贴板 → 覆盖 → Ctrl+V → sleep → 恢复）有 4 个结构性缺陷，`win-text-inject` 系统修复：

1. **剪贴板历史泄露** — 附加 4 种 opt-out 格式，规避 Windows 剪贴板历史/云剪贴板
2. **修饰键干扰** — 注入前释放所有按住修饰键，避免 Ctrl+V 变形
3. **UIPI 静默失败** — 完整性等级检测，降级时文本留在剪贴板并提示用户
4. **剪贴板恢复竞争** — 延迟渲染（`WM_RENDERFORMAT`），恢复严格排在目标读取之后，无延时常量

> 注入模式可在设置面板切换（`injectionMode`: `clipboard` / `typing`）。`InjectionController` 负责去重、会话绑定与目标校验；注入前会快照目标窗口，切换时不再误注入。

---

## 版本规划

- **v0.4.x（当前）** — 悬浮窗 HUD、本地离线 SenseVoice、全局快捷键、文本注入、上屏模式、模型管理与云端提供者自动回退、三平台 CI 校验
- **v0.5** — 自动标点、去口头禅、改口识别、个人词典、应用场景
- **v1.0** — AI 润色模式、语气/格式模式、翻译
- **v2.0** — 语音工作流平台、会议模式、插件 API

---

## 许可证

MIT License with Commercial Use Restriction — 详见 [LICENSE](./LICENSE)。

本软件在 MIT 许可证基础上附加**商业化使用限制**：个人学习、研究、非商业用途可自由使用与分发；**任何商业化使用（销售、授权、嵌入商业产品等）须事先获得作者（wellerlee820@163.com）的书面许可**。

---

*Built with ❤️ using Tauri, React, and Rust.*
