# 🎙️ VoiceBoom AI

**实时流式智能语音输入法** — Real-time Streaming Voice Input Method

> 像 Apple macOS 原生交互一样简洁优雅，同时具备 AI 时代实时语音输入能力。

## 产品概述

VoiceBoom AI 是一款面向 Windows / macOS 平台的低延迟实时语音转文字输入工具。用户通过快捷键唤醒麦克风后，可以连续自然讲话，系统实时将语音流转换为文字，并以悬浮窗口形式展示最新识别结果。

## 核心特性

- ⚡ **实时流式识别** — 延迟 ≤ 500ms，边说边出字
- 🎨 **macOS 毛玻璃 UI** — Glassmorphism 设计，Framer Motion 动画
- 🔌 **可插拔 ASR 引擎** — OpenAI Whisper / Deepgram 云端引擎
- ⌨️ **全局快捷键** — 按住说话，松开停止
- 🌐 **多语言支持** — 中/英/日/韩 自动检测与切换
- 🪶 **轻量级** — Tauri 2.0，安装包 < 15MB，内存 < 100MB

## 技术栈

| 模块 | 选型 |
|------|------|
| 桌面框架 | Tauri 2.0 (Rust) |
| 前端 UI | React 19 + TypeScript + Tailwind CSS |
| 动画 | Framer Motion |
| 状态管理 | Zustand |
| 音频采集 | CPAL (Rust) |
| 实时通信 | WebSocket (Tungstenite) |
| 数据库 | SQLite (rusqlite) |

## 快速开始

### 前置要求

- [Rust](https://www.rust-lang.org/tools/install) 1.80+
- [Node.js](https://nodejs.org/) 20+ 或 [Bun](https://bun.sh/) 1.2+
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) 2.0+

### 安装依赖

```bash
bun install
```

### 开发模式

```bash
bun run tauri:dev
```

### 构建生产版本

```bash
bun run tauri:build
```

## 项目结构

```
VoiceBoom/
├── src/                      # React 前端源码
│   ├── components/
│   │   ├── FloatingWindow/   # 悬浮窗核心组件
│   │   ├── Waveform/         # 动态波形
│   │   ├── Settings/         # 设置面板
│   │   ├── Shared/           # 通用 UI 组件
│   │   └── Animation/        # 动画配置
│   ├── stores/               # Zustand 状态管理
│   ├── hooks/                # 自定义 Hooks
│   ├── utils/                # 工具函数
│   ├── styles/               # 全局样式
│   ├── App.tsx
│   └── main.tsx
├── src-tauri/                # Rust 后端源码
│   ├── audio/                # 音频采集与 VAD
│   ├── asr/                  # ASR 引擎抽象与适配器
│   ├── shortcut/             # 全局快捷键
│   ├── commands/             # Tauri 命令
│   ├── db/                   # SQLite 数据库
│   └── src/main.rs
├── docs/                     # 设计文档
├── scripts/                  # 构建脚本
└── package.json
```

## 版本规划

- **V1.0 (MVP)** — 核心体验闭环：悬浮窗、云端 ASR、快捷键、设置
- **V1.5** — 本地离线引擎、多语言、历史记录、系统输入注入
- **V2.0** — AI 润色模式、专业术语库、多设备同步

## 许可证

MIT License

---

*Built with ❤️ using Tauri, React, and Rust.*
