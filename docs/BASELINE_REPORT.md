# VoiceBoom AI — 基线报告 (Baseline Report)

> 生成阶段：Phase 1 — 建立可靠基线与失败可复现机制
> 基线提交：`a2fd6a7` (main)
> 检查时间：2026-09-10

---

## 1. 构建结果

### 1.1 前端构建 (`bun run build` = `tsc -b && vite build`)

| 检查项 | 结果 | 详情 |
|---|---|---|
| TypeScript 类型检查 | ✅ 通过 | `tsc -b` 无错误 |
| Vite 生产构建 | ✅ 通过 | 410 modules, 8.28s |
| 输出 `dist/index.html` | ✅ | 0.46 kB |
| 输出 `dist/assets/index.css` | ✅ | 20.99 kB (gzip 4.83 kB) |
| 输出 `dist/assets/index.js` | ✅ | 375.26 kB (gzip 117.37 kB) |

### 1.2 Rust 构建 (`cargo check`)

| 检查项 | 结果 | 详情 |
|---|---|---|
| 编译 | ✅ 通过 | 2.07s (增量) |
| Errors | 0 | — |
| Warnings | 5 | 全部为 `non_snake_case` 命名警告 |

### 1.3 依赖安装 (`bun install`)

| 检查项 | 结果 | 详情 |
|---|---|---|
| 安装 | ✅ 无变更 | 255 installs / 309 packages, 484ms |
| Lockfile | ✅ | `bun.lock` 一致 |

---

## 2. 测试结果

### 2.1 Vitest 单元/组件测试 (`bun run test`)

| 指标 | 值 |
|---|---|
| Test Files | 2 passed (2) |
| Tests | 19 passed (19) |
| Duration | 39.21s |
| 覆盖范围 | store.test.ts (状态逻辑), components.test.tsx (UI 组件) |

**测试覆盖缺口**：核心链路（CPAL → queue → ASR → partial → final → injection）**零覆盖**。

### 2.2 Clippy 代码质量 (`cargo clippy`)

| 类别 | 数量 | 严重性 |
|---|---|---|
| `non_snake_case` (apiKey, vadSensitivity) | 3 | 低（FFI 兼容） |
| `new_without_default` (AppState) | 1 | 低 |
| `empty_line_after_doc_comment` | 1 | 低 |
| `redundant_field_names` | 1 | 低 |
| `unused_import` | 1 | 低 |
| `methods never used` (name, is_ready) | 1 | 中（死代码） |
| `function never used` (get_resource_endpoint) | 1 | 中（死代码） |
| `too many arguments` (8/7) | 1 | 低 |
| `redundant closure` | 3 | 低 |
| **总计** | **14** | — |

**无 correctness / security 级别警告。**

### 2.3 E2E 测试

| 检查项 | 结果 | 详情 |
|---|---|---|
| E2E 脚本 | ✅ 存在 | `scripts/e2e_smoke.mjs` |
| tauri-driver | ✅ 已安装 | `D:\cargo\bin\tauri-driver.exe` |
| msedgedriver | ✅ 已安装 | `D:\msedgedriver\msedgedriver.exe` |
| 测试配置 | ✅ 存在 | `src-tauri/tauri.test.conf.json` (单窗口) |
| 执行状态 | ⏸ 未运行 | 需先 `tauri build --config tauri.test.conf.json` |

---

## 3. 当前警告汇总

### Rust 编译警告 (5 个，全部 pre-existing)

| 文件 | 行 | 警告 | 建议 |
|---|---|---|---|
| `commands/mod.rs` | 68 | `apiKey` non_snake_case | 保留（前端 camelCase 对齐） |
| `commands/mod.rs` | 71 | `vadSensitivity` non_snake_case | 保留 |
| `commands/mod.rs` | 754 | `apiKey` non_snake_case | 保留 |

### Clippy 警告 (14 个)

详见 §2.2。**无高优先级警告。**

---

## 4. 已知缺陷与失败

### 4.1 架构缺陷（来自 Phase 0 分析）

| ID | 严重性 | 描述 |
|---|---|---|
| P0-A | 🔴 Critical | OpenAI 适配器协议层错误 — 把 HTTP REST URL 当 WebSocket endpoint |
| P0-B | 🔴 Critical | Deepgram 未区分 is_final / speech_final |
| P0-C | 🔴 Critical | AudioCapture 线程失败不回滚 is_recording（假录音） |
| P0-D | 🔴 Critical | 音频 unbounded channel — 内存/延迟无限增长 |
| P0-E | 🔴 Critical | API Key 明文存 SQLite（encrypted=1 非加密） |
| P1-A | 🟡 High | 本地 interim 每 200ms 全 buffer 重解码 |
| P1-B | 🟡 High | flush() 固定 sleep(500ms) |
| P1-C | 🟡 High | ASR task 无 JoinHandle/CancellationToken |
| P1-D | 🟡 High | 快捷键注册非事务化 |
| P1-E | 🟡 High | 5 个独立录音状态源 |
| P1-F | 🟡 High | 测试不覆盖核心链路 |

### 4.2 运行时已知问题

| 场景 | 表现 | 触发条件 |
|---|---|---|
| 无麦克风 | UI 显示录音中但无音频 | 设备不存在时按下快捷键 |
| 快速连按 | 10s 超时错误提示 | 前次 bridge 未结束再次 start |
| 长句转写 | 延迟逐渐增长 | 连续说话 > 8s (VAD max_speech) |
| 多次注入 | 一段话被注入多次 | 本地引擎每个 VAD segment 都触发 |

---

## 5. 运行时复现步骤

### 5.1 正常录音流程

```
1. 启动: bun run tauri:dev
2. 确保麦克风已连接
3. 按 Ctrl+Space (默认快捷键)
4. 说话
5. 释放 Ctrl+Space
6. 观察: 毛玻璃窗口显示转写结果
7. 检查日志: %TEMP%\voiceboom_debug.log
```

### 5.2 日志检查点

正常录音应在日志中看到：

```
[session=1726000000000-0] recording.start
[session=1726000000000-0] recording.config engine=funasr is_local=true
[session=1726000000000-0] asr.initialize engine=funasr
[session=1726000000000-0] asr.ready
[session=1726000000000-0] bridge.start frames=0
[session=1726000000000-0] recording.flush frames=123 had_partial=true
```

### 5.3 故障复现

| 故障 | 复现步骤 | 预期日志 |
|---|---|---|
| 假录音 | 拔出麦克风 → 按快捷键说话 → 释放 | `recording.start` 后无 `bridge.start` 或 0 frames |
| 无 ASR 结果 | 使用 cloud engine 但不配置 API key | `asr.initialize` 后出现 error |
| 注入失败 | 目标窗口是 elevated app | inject 日志显示 ClipboardOnly |

---

## 6. Phase 1 新增：统一日志字段

### 6.1 变更内容

在 `src-tauri/src/commands/mod.rs` 中新增 session ID 追踪：

- **`generate_session_id()`** — 生成 `{unix_ms}-{counter}` 格式唯一 ID
- **`recording:started` 事件** — 现在携带 `{session_id}` 供前端关联
- **所有录音链路日志** — 统一前缀 `[session=xxx]`

### 6.2 日志字段规范

| 字段 | 格式 | 示例 |
|---|---|---|
| `session` | `{unix_ms}-{counter}` | `1726000000000-42` |
| `event` | 点分命名 | `recording.start`, `asr.ready`, `recording.flush` |
| `engine` | 引擎标识 | `funasr`, `openai_whisper`, `deepgram` |
| `timestamp` | 隐含 (log 时间戳) | — |

### 6.3 变更文件

| 文件 | 变更类型 |
|---|---|
| `src-tauri/src/commands/mod.rs` | 新增 session_id 生成 + 日志字段 |

### 6.4 验证

- ✅ `cargo check` — 通过，无新增警告
- ✅ `bun run test` — 19/19 通过（无测试受影响）
- ✅ `bun run build` — 通过

---

## 7. 基线度量快照

| 指标 | 值 |
|---|---|
| Rust 代码行数 | ~2500 (src-tauri/src) |
| TS/TSX 代码行数 | ~3500 (src) |
| 单元测试数 | 19 |
| 核心链路测试数 | 0 |
| Rust warnings | 5 |
| Clippy warnings | 14 |
| 已知 P0 缺陷 | 5 |
| 已知 P1 缺陷 | 6 |
| 构建时间 (前端) | ~15s |
| 构建时间 (Rust 增量) | ~2s |
| 构建时间 (Rust 全量) | ~2m51s |

---

## 8. Gate 1 — 准入检查

| 条件 | 状态 |
|---|---|
| `bun install` 成功 | ✅ |
| 前端 build 成功 | ✅ |
| Vitest 全部通过 | ✅ (19/19) |
| `cargo check` 通过 | ✅ (0 errors) |
| Clippy 已检查 | ✅ (14 warnings, 0 critical) |
| E2E 基础设施确认 | ✅ (driver + config 就绪) |
| 统一日志字段已添加 | ✅ (session_id 贯穿录音链路) |
| BASELINE_REPORT.md 已输出 | ✅ (本文档) |

**✅ Gate 1 通过 — 可以进入 Phase 2。**
