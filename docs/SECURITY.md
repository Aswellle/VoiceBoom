# VoiceBoom AI — 安全文档

> 阶段：Phase 18 — 文档同步

---

## 1. API Key 安全

### 存储方式

| 平台 | 机制 | 实现 |
|---|---|---|
| Windows | DPAPI (CryptProtectData) | `secure_keystore.rs::WindowsKeyStore` |
| macOS | Keychain | `secure_keystore.rs::MacKeyStore` |
| Linux | 文件 0600 权限 | `secure_keystore.rs::LinuxKeyStore` |

### 安全保证

- ✅ API Key **不**以明文存储在 SQLite
- ✅ API Key **不**出现在日志
- ✅ API Key **不**出现在 URL
- ✅ API Key **不**出现在错误消息
- ✅ 应用重启后可正确读取
- ✅ 删除 key 后安全存储同步清理

### 迁移

旧版本（明文存储在 SQLite）的 key 会在首次读取时自动迁移到 OS 安全存储，并删除 SQLite 中的明文副本。

---

## 2. 文本注入安全

### Windows (win-text-inject)

- 延迟渲染剪贴板注入
- 解决：剪贴板历史隐私、修饰键 corruption、UIPI 静默失败、剪贴板恢复竞争
- 所有合成事件带 `INJECT_TAG` in `dwExtraInfo`

### macOS/Linux (enigo)

- 剪贴板 + 快捷键模拟
- 注入前保存用户剪贴板内容
- 注入后恢复用户剪贴板

### 注入结果

```rust
pub enum InjectionResult {
    Injected,           // 成功注入
    ClipboardFallback,  // 注入但无法确认
    PermissionDenied,   // 权限不足（高权限程序）
    TargetUnavailable,  // 无焦点输入区域
    Failed { reason: String }, // 其他失败
}
```

---

## 3. 架构锁

| 锁 | 规则 |
|---|---|
| **A** | Recording Session 是录音生命周期唯一权威状态源 |
| **B** | Audio callback 永远不等待网络/ASR |
| **C** | 音频队列必须 bounded |
| **D** | Provider event 必须先映射到统一 AsrEvent |
| **E** | UI 不直接理解 provider-specific event |
| **F** | 只有 utterance final 才允许自动注入 |
| **G** | flush 必须基于 completion/event，不基于固定 sleep |
| **H** | API Key 必须使用 OS secure storage |
| **I** | 所有 session 必须拥有唯一 session_id |
| **J** | 每个后台任务必须可取消、可结束、可观察 |

---

## 4. 已知限制

- 本地 SenseVoice 模型首次加载约 240MB
- 云端引擎需要 API Key 和网络连接
- Windows 上某些 elevated app 可能无法注入（UIPI 限制）
- macOS/Linux 注入依赖剪贴板，可能被安全软件拦截
