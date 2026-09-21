// Model Downloader — HTTPS with retry, progress, resumable downloads (spec section 8).
//
// Capabilities:
// - HTTPS with redirect following (reqwest default)
// - 3 retries with exponential backoff (1s, 2s, 4s)
// - Progress reporting via callback
// - Cancellation via AtomicBool flag
// - Resumable downloads using `.part` files + Range header
// - Concurrent-download handle so the manager can cancel in-flight jobs
//
// The HTTP client is passed in (not built per-download) so connections pool.

#![allow(dead_code)]
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;

/// Progress callback type. `downloaded` and `total` are byte counts.
/// `total` is 0 when the server does not send Content-Length.
pub type ProgressFn = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Handle to an in-flight download, used to cancel it.
#[derive(Clone)]
pub struct DownloadHandle {
    cancelled: Arc<AtomicBool>,
}

impl DownloadHandle {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Check whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Default for DownloadHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Download `url` into `dest`, writing through a `.part` file.
///
/// - Resumes an existing `.part` if its size matches a valid Range start.
/// - Calls `on_progress(downloaded, total)` as chunks arrive.
/// - Stops early if the handle is cancelled.
/// - On success, renames `.part` → final `dest`.
///
/// `client` is a shared HTTP client (connection reuse across downloads).
pub async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    handle: &DownloadHandle,
    on_progress: Option<&ProgressFn>,
) -> Result<(), String> {
    let part_path = dest.with_extension(
        dest.extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );

    // Determine resume point.
    let resume_from = if part_path.exists() {
        std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let mut last_err = None;
    for attempt in 0..3 {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s.
            tokio::time::sleep(Duration::from_secs(1 << (attempt - 1))).await;
        }

        match attempt_download(client, url, &part_path, resume_from, handle, on_progress).await {
            Ok(()) => {
                // Success — rename .part → final.
                std::fs::rename(&part_path, dest).map_err(|e| {
                    format!(
                        "重命名失败 {} → {}: {e}",
                        part_path.display(),
                        dest.display()
                    )
                })?;
                return Ok(());
            }
            Err(e) => {
                // If cancelled, do not retry.
                if e == "__cancelled__" {
                    return Err("下载已取消".to_string());
                }
                last_err = Some(e);
            }
        }
    }

    Err(format!(
        "下载失败（重试 3 次）: {}",
        last_err.unwrap_or_else(|| "未知错误".to_string())
    ))
}

/// Whether offline mode is active (`VOICEBOOM_OFFLINE=1`).
///
/// In offline mode the downloader refuses to touch the network: a build or
/// download either works from what is already on disk, or fails loudly.
pub fn is_offline() -> bool {
    std::env::var("VOICEBOOM_OFFLINE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Verify there is room for `required_bytes` on the filesystem holding `path`.
pub fn check_disk_space(path: &Path, required_bytes: u64) -> Result<(), String> {
    let free = free_bytes(path)?;
    if free < required_bytes {
        return Err(format!(
            "磁盘空间不足：可用 {free} 字节，需要 {required_bytes} 字节"
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn free_bytes(path: &Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::fileapi::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);

    let mut available: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available as *mut u64 as *mut _,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!("无法读取磁盘空间: {}", path.display()));
    }
    Ok(available)
}

#[cfg(unix)]
fn free_bytes(path: &Path) -> Result<u64, String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| "路径包含非法字符".to_string())?;

    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(format!("无法读取磁盘空间: {}", path.display()));
    }
    // f_bavail and f_frsize are 64-bit on the Linux and macOS targets we
    // build for, so this stays a plain u64 multiplication.
    Ok(stat.f_bavail * stat.f_frsize)
}

/// Download from an ordered list of sources, falling back on failure.
///
/// Returns the index of the source that succeeded. In offline mode, or when
/// every source fails, returns an error naming what was attempted.
pub async fn download_from_sources(
    client: &reqwest::Client,
    sources: &[String],
    dest: &Path,
    handle: &DownloadHandle,
    on_progress: Option<&ProgressFn>,
) -> Result<usize, String> {
    if sources.is_empty() {
        return Err("模型没有配置下载源".to_string());
    }

    if is_offline() {
        return Err("离线模式已启用，无法下载模型".to_string());
    }

    let mut failures: Vec<String> = Vec::new();

    for (idx, source) in sources.iter().enumerate() {
        if handle.is_cancelled() {
            return Err("下载已取消".to_string());
        }

        match download(client, source, dest, handle, on_progress).await {
            Ok(()) => return Ok(idx),
            Err(e) => {
                // Cancellation is terminal — never fall through to another source.
                if e == "下载已取消" {
                    return Err(e);
                }
                log::warn!("下载源 {source} 失败: {e}");
                failures.push(format!("源 {} ({source}): {e}", idx + 1));
            }
        }
    }

    cleanup_part(dest);
    Err(format!("所有下载源均失败:\n{}", failures.join("\n")))
}

async fn attempt_download(
    client: &reqwest::Client,
    url: &str,
    part_path: &Path,
    resume_from: u64,
    handle: &DownloadHandle,
    on_progress: Option<&ProgressFn>,
) -> Result<(), String> {
    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={resume_from}-"));
    }

    let response = request.send().await.map_err(|e| format!("请求失败: {e}"))?;

    let status = response.status();
    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("HTTP {status}"));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded = resume_from;

    // Open append if resuming, otherwise truncate.
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(resume_from > 0)
        .truncate(resume_from == 0)
        .open(part_path)
        .await
        .map_err(|e| format!("无法打开 {}: {e}", part_path.display()))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("传输错误: {e}"))?;

        // Cancellation check.
        if handle.is_cancelled() {
            return Err("__cancelled__".to_string());
        }

        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| format!("写入失败: {e}"))?;

        downloaded += chunk.len() as u64;
        if let Some(f) = on_progress {
            f(downloaded, total);
        }
    }

    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .map_err(|e| format!("刷新失败: {e}"))?;

    Ok(())
}

/// Remove a leftover `.part` file for a given destination.
pub fn cleanup_part(dest: &Path) {
    let part_path = dest.with_extension(
        dest.extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );
    if part_path.exists() {
        std::fs::remove_file(&part_path).ok();
    }
}
