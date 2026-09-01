# 语音转文本输入法 — 文本注入方案调研报告

> 目标：为 VoiceBoom 实现"将 ASR 识别结果注入当前焦点输入框"的核心能力，
> 优先复用 GitHub 上有权威背书的开源实现，避免重复造轮子。

---

## 结论（TL;DR）

**直接复用 [`win-text-inject`](https://github.com/emerson-d-lopes/win-text-inject)（Rust crate，MIT/Apache-2.0）。**

它是当前（2026-07 调研）Windows 平台上唯一系统解决文本注入四大难题的库，
被同类输入法项目 [`dictate`](https://github.com/3choff/dictate) 采用，
并已向 [`Handy`](https://github.com/cjpais/Handy/pull/1770) 上游贡献修复。

不要自己用 `enigo` + 固定延时 + 剪贴板恢复的"朴素方案"——那正是 `win-text-inject`
明确修复的反模式（详见下方"四大缺陷"）。

---

## 一、问题本质

语音输入法注入文本到"当前焦点输入框"，在 Windows 上**看起来简单，实际坑很深**。
朴素方案（也是目前绝大多数开源项目的做法）：

```
保存旧剪贴板 → 写入识别文本 → 合成 Ctrl+V → sleep → 恢复旧剪贴板
```

这个方案有 **4 个结构性缺陷**，每一个都会在生产环境暴露。

---

## 二、四大缺陷与 win-text-inject 的解法

### 缺陷 1：识别文本泄露到剪贴板历史与云剪贴板

写入 `CF_UNICODETEXT` 即加入 Windows 剪贴板历史 + 微软云剪贴板。
Chrome 隐身模式通过注册 4 种额外格式来规避，但几乎没别的软件这么做。

**win-text-inject 解法**：`set_text_private` 附加 4 种 opt-out 格式
（`ExcludeClipboardContentFromMonitorProcessing`、
`CanIncludeInClipboardHistory=0`、`CanUploadToCloudClipboard=0`、
`Clipboard Viewer Ignore`），被 Windows 剪贴板历史、云剪贴板、第三方管理器遵守。

### 缺陷 2：按住修饰键导致合成和弦错误

推麦录音时修饰键**必然**处于按住状态。MSDN 明确：
> `SendInput` 不重置键盘当前状态，已按下的键会干扰合成事件。

用户按住 Right-Alt 时，合成的 Ctrl+V 变成 AltGr+V（多数布局下是不同字符）。

**win-text-inject 解法**：`modifiers::sanitize` 在注入前释放所有按住修饰键，
且**不恢复**（重新按下用户已松开的修饰键会导致修饰键卡住）。

### 缺陷 3：注入提升权限窗口（UIPI）静默失败

`SendInput` 被 UIPI 拦截时，`GetLastError` 和返回值**都不会**指示失败原因——
文本直接消失。

**win-text-inject 解法**：`Target::accepts_injection` 在微秒级比较完整性等级，
让调用方诚实降级（把文本留在剪贴板并提示用户手动粘贴），而非静默丢失。

### 缺陷 4：剪贴板恢复与目标读取竞争

这是 Handy issue #502（2025-12 开放，52 条评论，至今未修）的根因。
目标**异步**读取剪贴板，固定延时的恢复可能在目标读取**之前**完成，
导致目标读到的是被恢复的旧内容。

**win-text-inject 解法**：**延迟渲染（Delayed Rendering）**——
不发布真实文本，而是发布一个承诺：`SetClipboardData(CF_UNICODETEXT, NULL)` +
隐藏所有者窗口。Windows 在消费者**真正请求数据**的瞬间发送 `WM_RENDERFORMAT`，
所有者才提供数据。这条消息即"目标已读取"信号，恢复严格排在读取之后。
**路径中没有任何延时常量**。

实测（120ms 恢复延时）：

| 算法 | 10ms | 60ms | 150ms | 400ms | 剪贴板恢复 |
|---|---|---|---|---|---|
| 无条件恢复（多数项目采用） | ✓ | ✓ | ✗ 错字 | ✗ 错字 | ✓ |
| 序列号门控恢复 | ✓ | ✓ | ✗ 错字 | ✗ 错字 | ✓ |
| 不恢复 | ✓ | ✓ | ✓ | ✓ | ✗ 用户剪贴板被破坏 |
| **延迟渲染** | ✓ | ✓ | ✓ | ✓ | ✓ |

真实应用验证：Chrome / VS Code / Notepad / Windows Terminal 全部通过。

---

## 三、推荐方案：win-text-inject

### 3.1 基本信息

| 项目 | 内容 |
|---|---|
| 仓库 | <https://github.com/emerson-d-lopes/win-text-inject> |
| 语言 | Rust |
| 许可证 | MIT OR Apache-2.0 |
| 依赖 | `windows` crate（Win32 API） |
| 状态 | 早期但核心功能已实现并通过真实语音测试 |
| 采用方 | [dictate](https://github.com/3choff/dictate)、[Handy](https://github.com/cjpais/Handy) |

### 3.2 API 概览

```toml
# Cargo.toml
[dependencies]
win-text-inject = "0.1"
```

```rust
use win_text_inject::{inject, Options, Target};

// 在热键按下时捕获目标（不是注入时！用户可能已切换窗口）
let target = Target::foreground()?;

// ... 录音 + ASR 转写 ...

let outcome = inject(&target, &transcript, Options::default())?;
if outcome.needs_manual_paste() {
    // 文本已在剪贴板，提示用户手动 Ctrl+V
}
```

### 3.3 关键设计要点

1. **热键按下时捕获目标**，而非注入时。按下到松开之间用户可能切换窗口。
2. **`Chord::for_exe` 按目标应用选择粘贴和弦**：终端用 Ctrl+Shift+V，其它用 Ctrl+V。
   每个条目都经过实测验证，非文档推测。
3. **`INJECT_TAG`**：所有合成事件在 `dwExtraInfo` 中携带标记。
   若你的应用安装了 `WH_KEYBOARD_LL` 钩子检测热键，需额外跳过 `dwExtraInfo == INJECT_TAG` 的事件，
   否则合成粘贴钩子会重新进入你的热键处理。
4. **不做的事**：
   - 不使用 UI Automation（微软明确：TextPattern 只读，不插入文本）。
   - 不实现 TSF 文本服务（需作为 in-proc COM DLL 注入每个目标进程，是另一个项目）。

### 3.4 已知局限

- `Strategy::UnicodeType` 未在真实窗口测试。
- `SendInput` 在粘贴过程中被阻塞时无回退链。
- 未在 Windows on ARM、RDP/Citrix/VDI、游戏、Windows 10 上测试。
- 每应用粘贴和弦表目前较小（仅覆盖常见应用）。

---

## 四、备选方案对比

### 4.1 enigo-rs/enigo（★1778）—— 跨平台输入模拟

| 项目 | 内容 |
|---|---|
| 仓库 | <https://github.com/enigo-rs/enigo> |
| 用途 | 跨平台鼠标/键盘/文本模拟 |
| Windows 实现 | `SendInput` |

**适用场景**：`direct_typing` 模式（直接模拟逐字键入）。

**局限**：不解决上述四大缺陷。若用于剪贴板方案，需自己处理延时、UIPI、修饰键、剪贴板隐私。
`dictate` 项目即采用 `enigo` + 朴素剪贴板方案（固定 50ms 延时），
属于 `win-text-inject` 明确修复的反模式。

**结论**：可作为 `direct_typing` 模式的底层（逐字键入），
但**剪贴板注入方案应优先用 win-text-inject**。

### 4.2 dictate（Tauri + Rust，★17）—— 参考实现

| 项目 | 内容 |
|---|---|
| 仓库 | <https://github.com/3choff/dictate> |
| 用途 | 完整的 Tauri 语音输入法 |
| 注入模块 | `text_injection.rs`、`clipboard_paste.rs`、`direct_typing.rs` |

**架构参考价值高**：
- `text_injection.rs`：Tauri 命令封装，按 `insertion_mode` 分发。
- `clipboard_paste.rs`：朴素剪贴板方案（`enigo` + 固定延时 + 剪贴板恢复）。
- `direct_typing.rs`：`enigo::text()` 逐字键入。

**注意**：其剪贴板方案是反模式，建议替换为 `win-text-inject`。
但其**整体架构**（命令分发、状态管理、双模式切换）值得参考。

### 4.3 不推荐的方案

| 方案 | 原因 |
|---|---|
| **UI Automation** | 微软明确 TextPattern 只读，不提供插入；`ValuePattern::SetValue` 替换整个控件值且忽略光标；启用 UIA 会让 Chromium 进程全局启用无障碍树，有持续性能损耗 |
| **TSF（Text Services Framework）** | 微软官方推荐的输入法 API，但需作为 in-proc COM DLL 注入每个目标进程，需注册、按架构构建、签名——是另一个独立项目，不适合当前阶段 |
| **SendMessage/WM_CHAR** | 跨进程不可靠，多数现代应用（Chromium、Electron）忽略 |

---

## 五、对 VoiceBoom 的实施建议

### 5.1 推荐架构

```
[全局热键] → [录音 + ASR] → [文本注入]
                                  ├── 主路径：win-text-inject（剪贴板 + 延迟渲染）
                                  └── 备选：enigo direct_typing（逐字键入，用于不支持剪贴板的场景）
```

### 5.2 实施步骤

1. **添加依赖**：`win-text-inject = "0.1"` 到 `src-tauri/Cargo.toml`。
2. **新建 `src-tauri/src/inject.rs`**：封装 Tauri 命令 `inject_text(text, mode)`。
3. **热键按下时调用 `Target::foreground()`** 捕获目标窗口。
4. **ASR 完成后调用 `inject()`**，根据 `outcome.needs_manual_paste()` 决定是否提示用户。
5. **处理 `INJECT_TAG`**：在现有 `WH_KEYBOARD_LL` 热键钩子中跳过注入事件。
6. **保留 `direct_typing` 备选**：用 `enigo` 实现，供用户在不支持剪贴板的应用中切换。

### 5.3 与现有架构的集成点

- `src-tauri/src/commands/mod.rs`：新增 `inject_text` 命令。
- `src-tauri/src/lib.rs`：注册命令。
- `src/hooks/useAsr.ts`：在 `stopRecording` 的 ASR 最终结果回调中调用注入。
- `src-tauri/Cargo.toml`：添加 `win-text-inject` + 保留 `enigo`（备选模式）。

### 5.4 风险与缓解

| 风险 | 缓解 |
|---|---|
| win-text-inject 处于早期 | 核心功能已通过真实语音测试；MIT/Apache-2.0 可 fork 维护 |
| UIPI 导致注入失败 | 库已返回 `needs_manual_paste()`，UI 提示用户手动粘贴 |
| 每应用粘贴和弦表不全 | 可提交 PR 扩展，或用户设置中允许自定义和弦 |
| Windows on ARM / RDP 未测试 | 后续版本覆盖；当前 MVP 聚焦主流 Windows 11 x64 |

---

## 六、参考项目汇总

| 项目 | Stars | 用途 | 复用方式 |
|---|---|---|---|
| [win-text-inject](https://github.com/emerson-d-lopes/win-text-inject) | — | Windows 文本注入（权威方案） | **直接依赖** |
| [enigo-rs/enigo](https://github.com/enigo-rs/enigo) | 1778 | 跨平台输入模拟 | direct_typing 备选 |
| [dictate](https://github.com/3choff/dictate) | 17 | Tauri 语音输入法（参考架构） | 架构参考 |
| [Handy](https://github.com/cjpais/Handy) | — | 同类输入法（已采用 win-text-inject 修复） | 验证背书 |
| [getdictus/dictus-desktop](https://github.com/getdictus/dictus-desktop) | 18 | 跨平台离线语音输入法 | 参考 |
| [Echo](https://github.com/GithubPhobos/Echo) | 21 | 离线推麦语音助手 | 参考 |

---

*调研时间：2026-09-01*
*调研范围：GitHub 开源项目 + Win32 API 文档 + MSDN*
