# VoiceBoom AI — 开发指南

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
```

## 项目架构

```
VoiceBoom/
├── src/                      # React 19 前端
│   ├── components/
│   │   ├── FloatingWindow/   # 毛玻璃悬浮窗（核心 UI）
│   │   ├── Waveform/         # 音频波形可视化
│   │   ├── Settings/         # 设置面板（6 标签页）
│   │   ├── Shared/           # 通用 UI 组件
│   │   └── Animation/        # Framer Motion 动画配置
│   ├── stores/useAppStore.ts # Zustand 全局状态
│   ├── hooks/
│   │   ├── useAsr.ts         # ASR 引擎控制
│   │   └── useGlobalShortcut.ts # 全局快捷键
│   ├── utils/                # 工具函数
│   └── styles/               # Tailwind 全局样式
├── src-tauri/                # Tauri 2.0 + Rust 后端
│   ├── audio/
│   │   ├── capture.rs        # CPAL 音频采集（独立线程）
│   │   └── vad.rs            # 语音活动检测
│   ├── asr/
│   │   ├── engine_trait.rs   # ASR 引擎抽象接口
│   │   ├── streaming.rs      # 流管理器
│   │   └── adapters/
│   │       ├── openai_whisper.rs  # OpenAI Whisper 适配器
│   │       └── deepgram.rs        # Deepgram 适配器
│   ├── shortcut/             # 全局快捷键管理
│   ├── commands/             # Tauri 命令（前端→后端）
│   └── db/                   # SQLite 数据库
└── docs/                     # 文档
```

## MVP (V1.0) 功能清单

| 功能 | 状态 | 说明 |
|------|------|------|
| Tauri 桌面壳 | ✅ | 双窗口（悬浮窗 + 设置窗） |
| 音频采集 | ✅ | CPAL 16kHz mono，独立线程 |
| 云端流式 ASR | ✅ | OpenAI Whisper + Deepgram 适配器 |
| 悬浮文字窗口 | ✅ | 毛玻璃 UI，Framer Motion 动画 |
| 最大展示长度限制 | ✅ | Zustand store 自动裁剪 |
| 全局快捷键 | ✅ | tauri-plugin-global-shortcut |
| 设置中心 | ✅ | 6 标签页配置 UI |
| 数据库 | ✅ | SQLite settings/history 表 |
| VAD | ✅ | 能量检测语音活动识别 |
| 多语言 | 🔧 | 前端 UI 已备，需后端适配 |
| 系统输入注入 | ⏸ | V1.5 功能 |
| 本地离线引擎 | ⏸ | V1.5 功能 |

## 下一步

1. 连接真实 ASR API（需 API Key）
2. 实现音频流到 ASR 的实时管道
3. 添加系统托盘图标和菜单
4. 实现系统输入注入（SendInput / CGEventPost）
5. 添加本地离线引擎支持（Whisper.cpp）
