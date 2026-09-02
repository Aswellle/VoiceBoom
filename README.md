# 🎙️ VoiceBoom AI

[![Tests](https://img.shields.io/badge/tests-19%20passing-brightgreen)](./src/test)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-9C27F0?logo=tauri)](https://v2.tauri.app)
[![Rust](https://img.shields.io/badge/Rust-180%2B-EA5800?logo=rust)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react)](https://react.dev)
[![License](https://img.shields.io/badge/license-MIT%20%2B%20non--commercial-important)](./LICENSE)

**实时流式智能语音输入法** — Real-time Streaming Voice Input Method

> 像微信语音转文本、iOS 键盘听写一样：按住热键说话，松手后文字**直接出现在光标所在的输入框**，无需手动复制粘贴。

一个面向 Windows / macOS 的低延迟实时语音转文字输入工具。按住全局快捷键唤醒麦克风，连续自然讲话，语音流实时转为文字，在毛玻璃悬浮窗中展示最新识别结果，并**自动注入到当前焦点输入框**。

---

## 核心特性

- 🎯 **直接注入输入框** — 转写文字自动出现在光标位置（微信/iOS 听写体验），非仅悬浮窗展示
- ⚡ **实时流式识别** — 延迟 ≤ 500ms，边说边出字
- 🎨 **毛玻璃悬浮窗** — Glassmorphism 设计，Framer Motion 动画，自动调高适配内容
- 🔌 **可插拔 ASR 引擎** — 本地离线 SenseVoice（内置，开箱即用）/ OpenAI Whisper / Deepgram
- ⌨️ **全局快捷键** — 按住说话，松开停止
- 🔒 **注入安全** — Windows 延迟渲染技术：不泄露剪贴板历史、不破坏用户剪贴板、UIPI 自动降级
- 🌐 **多语言支持** — 中/英/日/韩 自动检测与切换
- 🧪 **双层测试** — Vitest 单元/组件测试 + tauri-driver 真实桌面 E2E
- 🪶 **轻量级** — Tauri 2.0，Rust 后端无 Node.js 依赖

---

## 技术栈

| 模块 | 选型 |
|------|------|
| 桌面框架 | Tauri 2.0 (Rust) |
| 前端 UI | React 19 + TypeScript + Tailwind CSS |
| 动画 | Framer Motion |
| 状态管理 | Zustand |
| 音频采集 | CPAL (Rust) |
| 本地 ASR | sherpa-onnx (SenseVoice + Silero VAD) |
| 云端 ASR | OpenAI Whisper / Deepgram (WebSocket) |
| 文本注入 | win-text-inject (Windows) / enigo (跨平台) |
| 数据库 | SQLite (rusqlite) |

> **文本注入 crate 已源码内联到 `src-tauri/vendor/`，无外部 crates.io 依赖。**

---

## 快速开始

### 前置要求

- [Rust](https://www.rust-lang.org/tools/install) 1.80+
- [Bun](https://bun.sh/) 1.2+（首选）或 Node.js 20+
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) 2.0+

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

---

## 测试

项目采用两层测试策略。由于前端强依赖 Tauri API，无法在纯浏览器中渲染，因此**单元/组件测试在 jsdom 中通过 mock Tauri 层运行**，**端到端测试通过 tauri-driver 驱动真实桌面窗口**。

### 单元与组件测试（Vitest）

```bash
bun run test                # 运行一次（19 个测试）
bun run test:watch          # 监听模式
bun run test:ui             # Web UI 界面
bun run coverage            # 覆盖率报告
```

覆盖内容：
- `useAppStore` — M8 重入守卫、设置持久化、maxChars 预算裁剪、toast 定时
- `SegmentItem` — 渲染、无障碍语义（role/aria-label/键盘）、clipboard 复制 + textarea 降级
- `FloatingWindow` — 控件渲染、录音状态切换、按内容高度自动调窗、滚动"回到最新" FAB

### 端到端测试（tauri-driver）

```bash
bun run tauri:build:test    # 构建单窗口测试变体（仅 floating 窗口）
bun run test:e2e            # 启动 tauri-driver + msedgedriver，连接真实应用
```

E2E 覆盖：应用启动、引擎标签、开始/停止按钮、设置按钮。

> **不覆盖**（需真实麦克风 / 系统消息循环 / WebView2）：全局快捷键、实际录音、ASR 转写、桌面拖拽。

---

## 项目结构

```
VoiceBoom/
├── src/                        # React 19 前端 (TypeScript + Tailwind CSS)
│   ├── components/
│   │   ├── FloatingWindow/     # 悬浮窗核心组件（自动调高、滚动、复制）
│   │   ├── Settings/           # 设置面板（7 个标签页）
│   │   └── Waveform/           # 音频波形可视化
│   ├── hooks/
│   │   ├── useAsr.ts           # ASR 录音生命周期 + 注入调用
│   │   └── useGlobalShortcut.ts # 全局快捷键推麦
│   ├── stores/
│   │   └── useAppStore.ts      # Zustand 全局状态（设置/识别结果/UI/注入）
│   ├── test/                   # Vitest 测试套件
│   │   ├── setup.ts            # Tauri API mock + jsdom 补丁
│   │   ├── store.test.ts       # store 逻辑测试
│   │   └── components.test.tsx # 组件渲染与交互测试
│   ├── utils/                  # 工具函数
│   ├── styles/                 # 全局样式 + glassmorphism 设计令牌
│   ├── App.tsx                 # 根路由（按窗口标签分发）
│   └── main.tsx                # React 19 入口 + ErrorBoundary
├── src-tauri/                  # Tauri 2.0 Rust 后端
│   ├── src/
│   │   ├── asr/                # ASR 引擎抽象 + 适配器
│   │   │   ├── adapters/       # local(SenseVoice) / openai_whisper / deepgram
│   │   │   ├── engine_trait.rs # StreamingAsrEngine trait
│   │   │   └── streaming.rs    # AsrManager（引擎复用）
│   │   ├── audio/              # CPAL 音频采集 + 重采样
│   │   ├── commands/           # Tauri 命令处理器（含 inject_text）
│   │   ├── inject.rs           # 跨平台文本注入调度
│   │   ├── shortcut/           # 全局快捷键（平台默认）
│   │   ├── db/                 # SQLite（设置/历史/快捷键）
│   │   ├── resources/          # ONNX 模型路径解析
│   │   ├── tray/               # 系统托盘
│   │   ├── lib.rs              # AppState + 命令注册 + 托盘
│   │   └── main.rs             # 入口（windows_subsystem）
│   ├── vendor/                 # 内联 crate 源码（无外部依赖）
│   │   ├── win-text-inject/    # Windows 延迟渲染剪贴板注入
│   │   └── enigo/              # 跨平台键入模拟
│   ├── capabilities/           # Tauri 权限配置
│   ├── gen/schemas/            # 生成的 ACL schema
│   ├── icons/                  # 应用图标
│   ├── tools/                  # asr_debug 调试工具
│   ├── tauri.conf.json         # 窗口定义 + 构建配置
│   └── tauri.test.conf.json    # 单窗口 E2E 测试配置
├── scripts/
│   └── e2e_smoke.mjs           # tauri-driver 端到端冒烟测试
├── docs/
│   └── DEVELOPMENT.md          # 开发指南
├── public/                     # 静态资源
├── index.html                  # Vite 入口 HTML
├── verify_asr_integration.sh   # ASR 集成验证脚本
├── package.json                # 依赖 + 脚本
├── vite.config.ts              # Vite + Vitest 配置
├── tailwind.config.js          # Tailwind 配置
└── tsconfig.json               # TypeScript 配置
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

> 注入模式可在设置面板切换（`injectionMode`: `clipboard` / `typing`）。

---

## 版本规划

- **V1.0 (当前)** — 悬浮窗 + 本地离线 ASR + 全局快捷键 + 文本注入输入框 + 双层测试
- **V1.5** — 多语言增强、历史记录、系统输入注入优化
- **V2.0** — AI 润色模式、专业术语库、多设备同步

---

## 许可证

MIT License with Commercial Use Restriction — 详见 [LICENSE](./LICENSE)。

本软件在 MIT 许可证基础上附加**商业化使用限制**：个人学习、研究、非商业用途可自由使用与分发；**任何商业化使用（销售、授权、嵌入商业产品等）须事先获得作者（wellerlee820@163.com）的书面许可**。

---

*Built with ❤️ using Tauri, React, and Rust.*
