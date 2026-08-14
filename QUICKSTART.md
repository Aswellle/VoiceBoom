# VoiceBoom 本地 ASR 快速启动指南

## ✅ 实施完成

你的 VoiceBoom 现已集成 **whisper.cpp** 和 **FunASR** 本地语音识别引擎，开箱即用，无需任何额外配置。

---

## 🚀 立即体验

### 开发模式（推荐先用这个测试）

```bash
cd D:\Chrome_Downloads\AI_Coding\VoiceBoom
bun run tauri:dev
```

**首次启动**：
1. 应用窗口会出现浮动语音输入框
2. 点击右下角齿轮图标 → 打开设置
3. 引擎选择：
   - **whisper_cpp** — 多语言支持，低延迟（0.3-1 秒）
   - **funasr** — 中文优化，稍高延迟（1-4 秒，含模型加载）
4. 按住全局快捷键（默认 `Ctrl+Space`）说话
5. 松开后自动识别并显示文本 ✅

**自动配置**：
- 首次选择引擎时，资源会自动从 `src-tauri/asr-bundle/` 加载
- whisper-server 会自动启动（端口 8080）
- FunASR 模式会在需要时按需调用

---

## 📦 打包安装版

### 构建安装包

```bash
bun run tauri:build
```

**产物**：
```
src-tauri/target/release/bundle/
├── msi/VoiceBoom_0.1.0_x64_en-US.msi  # Windows 安装包
└── nsis/VoiceBoom_0.1.0_x64-setup.exe # Windows 便携安装器
```

**安装包大小**：~430 MB（包含所有 ASR 资源）

### 安装后使用

1. 双击安装包安装
2. 启动 VoiceBoom
3. 右下角齿轮 → 选择引擎
4. 按快捷键说话 → **立即识别** ✅

**无需任何下载或配置！**

---

## 🎯 核心特性

### 双引擎支持

| 引擎 | 优势 | 延迟 | 适用场景 |
|------|------|------|---------|
| **whisper.cpp** | 多语言、英文优秀 | 0.3-1 秒 | 日常混合语言输入 |
| **FunASR** | 中文精度高 | 1-4 秒 | 中文专业场景 |

### 完全离线

- ✅ 无需网络连接
- ✅ 无需 API key
- ✅ 数据完全本地处理
- ✅ 无隐私泄漏风险

### 零依赖

- ❌ 不需要 Python
- ❌ 不需要 PyTorch
- ❌ 不需要 CUDA/GPU 驱动
- ✅ 纯 C++ 原生二进制，开箱即用

---

## 🔧 故障排查

### whisper_cpp 无法启动

**症状**：设置中显示"服务器程序未找到"

**解决**：
```bash
# 检查资源是否存在
ls src-tauri/asr-bundle/whisper_cpp/whisper-server.exe
ls src-tauri/asr-bundle/whisper_cpp/models/ggml-base.bin
```

如果缺失，重新运行：
```bash
cd src-tauri
curl -L -o asr-bundle/whisper_cpp/models/ggml-base.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
```

### funasr 识别慢

**原因**：FunASR 每次识别都重新加载 242MB 模型（架构限制）

**优化方案**：
1. 使用 whisper_cpp（常驻内存，快速响应）
2. 或等待未来持久化改进

### 端口冲突

**症状**：whisper-server 启动失败，提示端口占用

**解决**：
```bash
# 查看端口占用
netstat -ano | findstr :8080

# 修改默认端口（在代码中）
# src-tauri/src/commands/mod.rs 第 75 行
# 改为其他端口如 8081
```

---

## 📁 资源结构

```
src-tauri/asr-bundle/       # 397 MB
├── whisper_cpp/
│   ├── whisper-server.exe  # HTTP 服务器
│   ├── *.dll               # 14 个运行时库
│   └── models/
│       └── ggml-base.bin   # 141 MB Whisper Base 模型
└── funasr/
    ├── llama-funasr-sensevoice.exe  # CLI 工具
    └── models/
        ├── sensevoice-small-q8.gguf # 242 MB 中文模型
        └── fsmn-vad.gguf            # 2 MB VAD 模型
```

**模型说明**：
- **ggml-base.bin**：Whisper Base 多语言模型，支持 99 种语言
- **sensevoice-small-q8.gguf**：阿里 FunASR SenseVoice 中文优化模型
- **fsmn-vad.gguf**：语音活动检测（Voice Activity Detection）

---

## 📚 更多信息

- **详细技术文档**：`IMPLEMENTATION_SUMMARY.md`
- **代码审查修复**：`CODE_REVIEW_FIXES.md` ⭐ 新增
- **代码结构**：`CLAUDE.md` → Architecture 部分
- **验证脚本**：`./verify_asr_integration.sh`

---

## 🔄 最新更新（2026-08-11）

### 代码审查修复

已修复 4 个关键问题：
- ✅ FunASR 状态追踪失败（导致"服务器未运行"误报）
- ✅ 临时文件名冲突（并发录音互相覆盖）
- ✅ whisper-server 连接无重试（启动后立即连接失败）
- ✅ 资源优先级错误（阻止用户升级二进制）

详见 `CODE_REVIEW_FIXES.md`。

---

## 🎉 完成！

你的 VoiceBoom 现已支持：
- ✅ 完全离线的本地语音识别
- ✅ 双引擎切换（whisper.cpp + FunASR）
- ✅ 开箱即用，无需任何配置
- ✅ Windows 原生支持，性能优秀

**立即开始使用**：
```bash
bun run tauri:dev
```

*实施完成时间：2026-08-11*
