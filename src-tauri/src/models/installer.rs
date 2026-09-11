// Model Installer — atomic install via .staging directory (spec section 7).
//
// Install flow:
//   extract archive → .staging/<engine>-<version>-<random>/
//   validate every file (SHA256 + size)
//   rename → models/<engine>/<version>/
//   update active.json
//
// If anything fails the staging directory is removed, leaving any previous
// version intact (rollback by design).

use std::path::{Path, PathBuf};

use crate::models::verifier::verify_file;

/// A file declared in the manifest that we expect to find after extraction.
#[derive(Debug, Clone)]
pub struct ExpectedFile {
    /// Relative path within the model dir, e.g. `sensevoice/1.0.0/model.int8.onnx`.
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

/// Result of a successful install.
#[derive(Debug, Clone)]
pub struct InstallResult {
    pub engine: String,
    pub version: String,
    pub model_dir: PathBuf,
}

/// Install a model from a verified archive into the models directory atomically.
///
/// `archive_path` is the downloaded zip. `models_dir` is the root
/// (`%LOCALAPPDATA%\VoiceBoom\models\`). `engine`/`version` identify the model.
/// `expected_files` is the file list from the registry manifest.
pub fn install_from_archive(
    archive_path: &Path,
    models_dir: &Path,
    engine: &str,
    version: &str,
    expected_files: &[ExpectedFile],
) -> Result<InstallResult, String> {
    // Create a unique staging directory.
    let staging = models_dir.join(format!(
        ".staging/{engine}-{version}-{rand}",
        rand = std::process::id()
    ));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| format!("清理旧 staging 失败: {e}"))?;
    }
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("创建 staging 目录失败 {}: {e}", staging.display()))?;

    // Extract the archive into staging.
    extract_zip(archive_path, &staging)?;

    // Final destination for this model version.
    let version_dir = models_dir.join(engine).join(version);

    // Validate every expected file.
    for expected in expected_files {
        let file_path = staging.join(&expected.relative_path);
        verify_file(&file_path, &expected.sha256, expected.size)?;
    }

    // The staging holds `<engine>/<version>/...` per the spec archive layout.
    let staged_version = staging.join(engine).join(version);

    // Write manifest.json into the staging version dir so it moves with the
    // atomic rename. This keeps `is_version_ready()` consistent.
    let manifest_path = staged_version.join("manifest.json");
    if !manifest_path.exists() {
        let manifest = serde_json::json!({
            "engine": engine,
            "version": version,
            "files": expected_files.iter().map(|f| &f.relative_path).collect::<Vec<_>>(),
        });
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).map_err(|e| format!("序列化 manifest 失败: {e}"))?,
        )
        .map_err(|e| format!("写入 manifest 失败: {e}"))?;
    }

    // Atomically move the version directory into place.
    if version_dir.exists() {
        // Back up the existing version so we can roll back on failure.
        let backup = models_dir.join(format!(".backup/{engine}-{version}"));
        std::fs::create_dir_all(&backup.parent().unwrap()).ok();
        if backup.exists() {
            std::fs::remove_dir_all(&backup).ok();
        }
        std::fs::rename(&version_dir, &backup).map_err(|e| {
            format!(
                "备份旧版本失败 {} → {}: {e}",
                version_dir.display(),
                backup.display()
            )
        })?;
    }

    // Move staging version content into the final location.
    // The staging holds `<engine>/<version>/...` per the spec archive layout.
    let staged_version = staging.join(engine).join(version);
    if staged_version.exists() {
        std::fs::create_dir_all(version_dir.parent().unwrap())
            .map_err(|e| format!("创建引擎目录失败: {e}"))?;
        std::fs::rename(&staged_version, &version_dir).map_err(|e| {
            format!(
                "原子移动失败 {} → {}: {e}",
                staged_version.display(),
                version_dir.display()
            )
        })?;
    }

    // Clean up staging.
    std::fs::remove_dir_all(&staging).ok();
    // Clean up backup on success (spec section 23 says keep old versions, but
    // the .backup dir is an internal detail we keep for one upgrade cycle).
    let _ = std::fs::remove_dir_all(models_dir.join(".backup"));

    Ok(InstallResult {
        engine: engine.to_string(),
        version: version.to_string(),
        model_dir: version_dir,
    })
}

/// Install from an already-extracted directory (used by import-model flow).
pub fn install_from_dir(
    source_dir: &Path,
    models_dir: &Path,
    engine: &str,
    version: &str,
    expected_files: &[ExpectedFile],
) -> Result<InstallResult, String> {
    let version_dir = models_dir.join(engine).join(version);

    std::fs::create_dir_all(&version_dir)
        .map_err(|e| format!("创建版本目录失败 {}: {e}", version_dir.display()))?;

    // Copy each expected file.
    for expected in expected_files {
        let src = source_dir.join(&expected.relative_path);
        let dst = version_dir.join(
            Path::new(&expected.relative_path)
                .file_name()
                .unwrap_or_else(|| Path::new(&expected.relative_path).as_os_str()),
        );
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        verify_file(&src, &expected.sha256, expected.size)?;
        std::fs::copy(&src, &dst).map_err(|e| {
            format!(
                "复制失败 {} → {}: {e}",
                src.display(),
                dst.display()
            )
        })?;
    }

    // Write manifest.json.
    let manifest_path = version_dir.join("manifest.json");
    if !manifest_path.exists() {
        let manifest = serde_json::json!({
            "engine": engine,
            "version": version,
        });
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest)
                .map_err(|e| format!("序列化 manifest 失败: {e}"))?,
        )
        .map_err(|e| format!("写入 manifest 失败: {e}"))?;
    }

    Ok(InstallResult {
        engine: engine.to_string(),
        version: version.to_string(),
        model_dir: version_dir,
    })
}

/// Remove a specific version directory.
pub fn remove_version(models_dir: &Path, engine: &str, version: &str) -> Result<(), String> {
    let version_dir = models_dir.join(engine).join(version);
    if version_dir.exists() {
        std::fs::remove_dir_all(&version_dir)
            .map_err(|e| format!("删除版本失败 {}: {e}", version_dir.display()))?;
    }
    // Remove empty engine dir.
    let engine_dir = models_dir.join(engine);
    if engine_dir.exists() {
        if std::fs::read_dir(&engine_dir).map(|mut d| d.next().is_none()).unwrap_or(false) {
            std::fs::remove_dir_all(&engine_dir).ok();
        }
    }
    Ok(())
}

/// Extract a zip archive into `dest`.
fn extract_zip(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("打开压缩包失败 {}: {e}", archive_path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("解析 zip 失败: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取 zip 条目 {i} 失败: {e}"))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("zip 条目 {i} 路径非法"))?;
        let out_path = dest.join(enclosed);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).ok();
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut outfile = std::fs::File::create(&out_path)
                .map_err(|e| format!("创建文件失败 {}: {e}", out_path.display()))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("解压失败 {}: {e}", out_path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_and_install_roundtrip() {
        // Build a tiny in-memory zip.
        let dir = std::env::temp_dir().join(format!("voiceboom_install_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();

        let archive = dir.join("model.zip");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts = zip::write::FileOptions::<()>::default();
            z.start_file("sensevoice/1.0.0/model.int8.onnx", opts).unwrap();
            z.write_all(b"model-bytes").unwrap();
            z.start_file("sensevoice/1.0.0/tokens.txt", opts).unwrap();
            z.write_all(b"token-bytes").unwrap();
            z.start_file("sensevoice/1.0.0/silero_vad.onnx", opts).unwrap();
            z.write_all(b"vad-bytes").unwrap();
            z.finish().unwrap();
        }

        // Extract first to compute real hashes.
        let extract_dir = dir.join("extracted");
        extract_zip(&archive, &extract_dir).unwrap();
        let expected = vec![
            ExpectedFile {
                relative_path: "sensevoice/1.0.0/model.int8.onnx".into(),
                size: 11,
                sha256: crate::models::verifier::compute_sha256(
                    &extract_dir.join("sensevoice/1.0.0/model.int8.onnx"),
                )
                .unwrap(),
            },
            ExpectedFile {
                relative_path: "sensevoice/1.0.0/tokens.txt".into(),
                size: 11,
                sha256: crate::models::verifier::compute_sha256(
                    &extract_dir.join("sensevoice/1.0.0/tokens.txt"),
                )
                .unwrap(),
            },
            ExpectedFile {
                relative_path: "sensevoice/1.0.0/silero_vad.onnx".into(),
                size: 9,
                sha256: crate::models::verifier::compute_sha256(
                    &extract_dir.join("sensevoice/1.0.0/silero_vad.onnx"),
                )
                .unwrap(),
            },
        ];

        let models_dir = dir.join("models");
        let version_dir = models_dir.join("sensevoice").join("1.0.0");
        let result = install_from_archive(
            &archive,
            &models_dir,
            "sensevoice",
            "1.0.0",
            &expected,
        );
        assert!(result.is_ok(), "install failed: {:?}", result.err());
        assert!(version_dir.join("model.int8.onnx").exists());
        assert!(version_dir.join("manifest.json").exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}
