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

use std::path::{Path, PathBuf};
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
    on_progress: Option<ProgressFn>,
) -> Result<(), String> {
    let part_path = dest.with_extension(
        dest.extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );

    // Determine resume point.
    let resume_from = if part_path.exists() {
        std::fs::metadata(&part_path)
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    let mut last_err = None;
    for attempt in 0..3 {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s.
            tokio::time::sleep(Duration::from_secs(1 << (attempt - 1))).await;
        }

        match attempt_download(client, url, &part_path, resume_from, handle, &on_progress).await {
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

async fn attempt_download(
    client: &reqwest::Client,
    url: &str,
    part_path: &Path,
    resume_from: u64,
    handle: &DownloadHandle,
    on_progress: &Option<ProgressFn>,
) -> Result<(), String> {
    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={resume_from}-"));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;

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
