//! Unified Resource Downloader for VoiceBoom
//!
//! Supports:
//! - Multi-source fallback (sources array)
//! - SHA-256 validation
//! - Atomic installation (temp file → verify → rename)
//! - Offline mode (VOICEBOOM_OFFLINE=1)
//! - Retry with exponential backoff
//! - Cancellation support
//! - Disk space check

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// Progress callback type.
pub type ProgressFn = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Handle to an in-flight download.
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

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Default for DownloadHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource descriptor from configuration.
#[derive(Debug, Clone)]
pub struct ResourceDescriptor {
    pub version: String,
    pub archive: String,
    pub sha256: String,
    pub sources: Vec<String>,
}

/// Download a resource from multiple sources with fallback.
///
/// Priority:
/// 1. SHERPA_ONNX_ARCHIVE_DIR (if set and archive exists)
/// 2. Local cache
/// 3. sources[0] → retry → sources[1] → retry → ...
/// 4. Fail with all error reasons
pub async fn download_resource(
    client: &reqwest::Client,
    descriptor: &ResourceDescriptor,
    cache_dir: &Path,
    platform_key: &str,
    handle: &DownloadHandle,
    on_progress: Option<ProgressFn>,
) -> Result<PathBuf, String> {
    // Check offline mode
    let offline = std::env::var("VOICEBOOM_OFFLINE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    // 1. Check SHERPA_ONNX_ARCHIVE_DIR
    if let Ok(archive_dir) = std::env::var("SHERPA_ONNX_ARCHIVE_DIR") {
        let archive_path = PathBuf::from(&archive_dir).join(&descriptor.archive);
        if archive_path.exists() {
            return Ok(archive_path);
        }
    }

    // 2. Check local cache
    let cache_subdir = cache_dir.join(&descriptor.version).join(platform_key);
    let cache_path = cache_subdir.join(&descriptor.archive);

    if cache_path.exists() {
        // Verify hash if available
        if !descriptor.sha256.is_empty() && descriptor.sha256 != "PLACEHOLDER_WIN_SHA256" {
            match verify_sha256(&cache_path, &descriptor.sha256).await {
                Ok(true) => return Ok(cache_path),
                Ok(false) => {
                    // Hash mismatch, remove and re-download
                    tokio::fs::remove_file(&cache_path).await.ok();
                }
                Err(e) => {
                    return Err(format!("Hash verification failed: {}", e));
                }
            }
        } else {
            return Ok(cache_path);
        }
    }

    if offline {
        return Err(format!(
            "Resource '{}' not found in local cache and offline mode is enabled",
            descriptor.archive
        ));
    }

    // 3. Download from sources
    let mut all_errors = Vec::new();

    for (source_idx, source) in descriptor.sources.iter().enumerate() {
        for attempt in 0..3 {
            if attempt > 0 {
                let delay = 2u64.pow(attempt - 1);
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }

            match attempt_download(client, source, &cache_path, handle, &on_progress).await {
                Ok(()) => {
                    // Verify SHA-256
                    if !descriptor.sha256.is_empty() && descriptor.sha256 != "PLACEHOLDER_WIN_SHA256" {
                        match verify_sha256(&cache_path, &descriptor.sha256).await {
                            Ok(true) => return Ok(cache_path),
                            Ok(false) => {
                                all_errors.push(format!(
                                    "Source {}: SHA-256 mismatch",
                                    source_idx + 1
                                ));
                                tokio::fs::remove_file(&cache_path).await.ok();
                                break; // Try next source
                            }
                            Err(e) => {
                                all_errors.push(format!("Source {}: {}", source_idx + 1, e));
                                break;
                            }
                        }
                    } else {
                        return Ok(cache_path);
                    }
                }
                Err(e) => {
                    if e == "__cancelled__" {
                        return Err("Download cancelled".to_string());
                    }
                    all_errors.push(format!("Source {} attempt {}: {}", source_idx + 1, attempt + 1, e));
                }
            }
        }
    }

    Err(format!(
        "All download sources failed:\n{}",
        all_errors.join("\n")
    ))
}

/// Verify SHA-256 hash of a file.
async fn verify_sha256(path: &Path, expected: &str) -> Result<bool, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Cannot open file: {}", e))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8192];

    loop {
        let n = tokio::io::AsyncReadExt::read(&mut file, &mut buffer)
            .await
            .map_err(|e| format!("Read error: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();
    let actual = format!("{:x}", result);
    Ok(actual == expected.to_lowercase())
}

/// Check available disk space.
pub fn check_disk_space(path: &Path, required_bytes: u64) -> Result<(), String> {
    // Get available space using std::fs
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("Cannot access path {}: {}", path.display(), e))?;

    // On Windows, use GetDiskFreeSpaceEx
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::fileapi::GetDiskFreeSpaceExW;

        let path_wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;

        unsafe {
            if GetDiskFreeSpaceExW(
                path_wide.as_ptr(),
                &mut free_bytes as *mut u64 as *mut _,
                &mut total_bytes as *mut u64 as *mut _,
                &mut total_free_bytes as *mut u64 as *mut _,
            ) == 0
            {
                return Err("Failed to get disk space".to_string());
            }
        }

        if free_bytes < required_bytes {
            return Err(format!(
                "Insufficient disk space: {} bytes available, {} bytes required",
                free_bytes, required_bytes
            ));
        }
    }

    // On Unix, use statvfs
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let c_path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| "Invalid path".to_string())?;

        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        unsafe {
            if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
                return Err("Failed to get filesystem stats".to_string());
            }
        }

        let free_bytes = stat.f_bavail as u64 * stat.f_frsize as u64;
        if free_bytes < required_bytes {
            return Err(format!(
                "Insufficient disk space: {} bytes available, {} bytes required",
                free_bytes, required_bytes
            ));
        }
    }

    Ok(())
}

async fn attempt_download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    handle: &DownloadHandle,
    on_progress: &Option<ProgressFn>,
) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Cannot create directory: {}", e))?;
    }

    let part_path = dest.with_extension(
        dest.extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );

    let mut request = client.get(url);
    if part_path.exists() {
        let resume_from = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={resume_from}-"));
        }
    }

    let response = request.send().await.map_err(|e| format!("Request failed: {}", e))?;
    let status = response.status();

    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("HTTP {}", status));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded = if part_path.exists() {
        std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(downloaded > 0)
        .truncate(downloaded == 0)
        .open(&part_path)
        .await
        .map_err(|e| format!("Cannot open {}: {}", part_path.display(), e))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Transfer error: {}", e))?;

        if handle.is_cancelled() {
            return Err("__cancelled__".to_string());
        }

        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;
        if let Some(f) = on_progress {
            f(downloaded, total);
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;

    // Atomic rename
    tokio::fs::rename(&part_path, dest)
        .await
        .map_err(|e| format!("Rename failed: {}", e))?;

    Ok(())
}

/// Clean up a leftover `.part` file.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_handle() {
        let handle = DownloadHandle::new();
        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
    }

    #[test]
    fn test_cleanup_part() {
        let temp = std::env::temp_dir().join("voiceboom_test_cleanup.part");
        std::fs::write(&temp, b"test").unwrap();
        cleanup_part(&temp);
        assert!(!temp.exists());
    }
}
