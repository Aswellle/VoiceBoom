// Model Verifier — SHA256 + size validation (spec section 8).
//
// Every downloaded file and archive is verified before installation.
// Verification failures surface as `ModelState::Corrupt`.

#![allow(dead_code)]
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// Result of a successful verification.
#[derive(Debug, Clone)]
pub struct Verification {
    pub sha256: String,
    pub size: u64,
}

/// Verify a single file against expected SHA256 and optional expected size.
///
/// `expected_sha256` is the lowercase hex digest. `expected_size` of 0 means
/// "skip size check".
pub fn verify_file(
    path: &Path,
    expected_sha256: &str,
    expected_size: u64,
) -> Result<Verification, String> {
    let file = File::open(path).map_err(|e| format!("无法打开文件 {}: {}", path.display(), e))?;

    let metadata = file
        .metadata()
        .map_err(|e| format!("无法读取元数据 {}: {}", path.display(), e))?;

    let actual_size = metadata.len();

    if expected_size > 0 && actual_size != expected_size {
        return Err(format!(
            "大小不匹配 {}: 期望 {}, 实际 {}",
            path.display(),
            expected_size,
            actual_size
        ));
    }

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取失败 {}: {}", path.display(), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual_sha256 = hex::encode(hasher.finalize());

    if !expected_sha256.is_empty()
        && expected_sha256 != "REPLACE"
        && actual_sha256 != expected_sha256
    {
        return Err(format!(
            "SHA256 不匹配 {}: 期望 {}, 实际 {}",
            path.display(),
            expected_sha256,
            actual_sha256
        ));
    }

    Ok(Verification {
        sha256: actual_sha256,
        size: actual_size,
    })
}

/// Compute SHA256 of a file without any expected-value check.
/// Useful for generating registry values during `prepare-models.py`.
pub fn compute_sha256(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("无法打开 {}: {}", path.display(), e))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取失败 {}: {}", path.display(), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Compute SHA256 of a byte slice.
pub fn compute_sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Verify a zip archive's SHA256 without extracting it.
pub fn verify_archive(archive_path: &Path, expected_sha256: &str) -> Result<Verification, String> {
    verify_file(archive_path, expected_sha256, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_compute_sha256_bytes() {
        let digest = compute_sha256_bytes(b"hello");
        // Known SHA-56 of "hello"
        assert_eq!(
            digest,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_verify_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("voiceboom_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.bin");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"test content").unwrap();
        drop(f);

        let expected = compute_sha256(&path).unwrap();
        let result = verify_file(&path, &expected, 12);
        assert!(result.is_ok());
        let v = result.unwrap();
        assert_eq!(v.size, 12);
        assert_eq!(v.sha256, expected);

        // Wrong hash should fail.
        let bad = verify_file(&path, "0000000000000000000000000000000000000000000000000000000000000000", 12);
        assert!(bad.is_err());

        // Wrong size should fail.
        let bad_size = verify_file(&path, &expected, 999);
        assert!(bad_size.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
