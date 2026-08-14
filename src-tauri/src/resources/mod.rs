// Local ASR Resource Manager
// Manages bundled ONNX models for sherpa-onnx inference
// Models are bundled with the app or downloaded separately

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tauri;

/// Supported local ASR engines (sherpa-onnx based)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ResourceEngine {
    SenseVoice,
}

impl ResourceEngine {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceEngine::SenseVoice => "sensevoice",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ResourceEngine::SenseVoice => "SenseVoice",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sensevoice" | "funasr" => Some(ResourceEngine::SenseVoice),
            _ => None,
        }
    }

    /// Get the default ONNX model filename
    pub fn default_model_filename(&self) -> &'static str {
        match self {
            ResourceEngine::SenseVoice => "model.int8.onnx",
        }
    }

    /// Get the tokens file name
    pub fn tokens_filename(&self) -> &'static str {
        match self {
            ResourceEngine::SenseVoice => "tokens.txt",
        }
    }

    /// Get the Silero VAD model filename (shared across engines)
    pub fn vad_model_filename(&self) -> &'static str {
        "silero_vad.onnx"
    }
}

/// Version channel for updates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionChannel {
    Stable,
    Preview,
}

impl VersionChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            VersionChannel::Stable => "stable",
            VersionChannel::Preview => "preview",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            VersionChannel::Stable => "稳定版",
            VersionChannel::Preview => "前瞻版",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "stable" => Some(VersionChannel::Stable),
            "preview" => Some(VersionChannel::Preview),
            _ => None,
        }
    }
}

/// Information about a resource package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePackageInfo {
    pub engine: ResourceEngine,
    pub version: String,
    pub channel: VersionChannel,
    pub path: PathBuf,
    pub is_bundled: bool,
    pub size_bytes: u64,
    pub updated_at: String,
    pub model_file_exists: bool,
    pub tokens_file_exists: bool,
    pub vad_model_exists: bool,
    pub is_ready: bool,
}

/// Simple resource manager
pub struct ResourceManager {
    base_dir: PathBuf,
}

impl ResourceManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get the resource directory for an engine
    pub fn engine_dir(&self, engine: ResourceEngine) -> PathBuf {
        self.base_dir.join(engine.as_str())
    }

    /// Get the models directory for an engine
    pub fn models_dir(&self, engine: ResourceEngine) -> PathBuf {
        self.engine_dir(engine).join("models")
    }

    /// Directories that may hold model files, in resolution order.
    fn model_search_dirs(&self, engine: ResourceEngine) -> Vec<PathBuf> {
        let mut dirs = vec![self.models_dir(engine)];
        if let Some(bundle) = get_bundled_resource_dir() {
            dirs.push(bundle.join(engine.as_str()).join("models"));
            // Also check engine dir directly (models not in models/ subdir)
            dirs.push(bundle.join(engine.as_str()));
        }
        // Portable layout: models sitting next to the standalone EXE
        dirs.extend(portable_model_dirs(engine));
        dirs
    }

    /// Get the ONNX model file path
    pub fn model_path(&self, engine: ResourceEngine) -> Option<PathBuf> {
        find_in_dirs(&self.model_search_dirs(engine), engine.default_model_filename())
    }

    /// Get the tokens file path
    pub fn tokens_path(&self, engine: ResourceEngine) -> Option<PathBuf> {
        find_in_dirs(&self.model_search_dirs(engine), engine.tokens_filename())
    }

    /// Get the Silero VAD model file path (shared VAD for all engines)
    pub fn vad_model_path(&self, engine: ResourceEngine) -> Option<PathBuf> {
        find_in_dirs(&self.model_search_dirs(engine), engine.vad_model_filename())
    }

    /// Get package info for an engine.
    pub fn get_package(&self, engine: ResourceEngine) -> Option<ResourcePackageInfo> {
        let model_file = self.model_path(engine);
        let tokens_file = self.tokens_path(engine);
        let vad_model = self.vad_model_path(engine);

        let model_file_exists = model_file.is_some();
        let tokens_file_exists = tokens_file.is_some();
        let vad_model_exists = vad_model.is_some();

        let appdata_dir = self.engine_dir(engine);
        // Nothing resolved anywhere and no local dir — the engine is absent.
        if !model_file_exists && !tokens_file_exists && !appdata_dir.exists() {
            return None;
        }

        // Report the directory the resources are really coming from
        let effective_dir = model_file
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| appdata_dir.clone());

        // Ready = model file + tokens + VAD all exist
        let is_ready = model_file_exists && tokens_file_exists && vad_model_exists;

        // Size covers the model directory
        let size_bytes = self.dir_size(&effective_dir);

        Some(ResourcePackageInfo {
            engine,
            version: self.read_version(&appdata_dir),
            channel: VersionChannel::Stable,
            path: effective_dir.clone(),
            is_bundled: appdata_dir.join(".bundled").exists()
                || get_bundled_resource_dir()
                    .map(|b| effective_dir.starts_with(&b))
                    .unwrap_or(false),
            size_bytes,
            updated_at: self.read_updated_at(&appdata_dir),
            model_file_exists,
            tokens_file_exists,
            vad_model_exists,
            is_ready,
        })
    }

    /// Get all package info
    pub fn get_all_packages(&self) -> Vec<ResourcePackageInfo> {
        let mut results = Vec::new();
        if let Some(info) = self.get_package(ResourceEngine::SenseVoice) {
            results.push(info);
        }
        results
    }

    /// Ensure models directory exists
    pub fn ensure_models_dir(&self, engine: ResourceEngine) -> PathBuf {
        let dir = self.models_dir(engine);
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    // Helper methods

    fn read_version(&self, dir: &Path) -> String {
        let version_file = dir.join(".version");
        std::fs::read_to_string(version_file).unwrap_or_else(|_| "bundled".to_string())
    }

    fn read_updated_at(&self, dir: &Path) -> String {
        let file = dir.join(".updated_at");
        std::fs::read_to_string(file).unwrap_or_else(|_| "unknown".to_string())
    }

    fn dir_size(&self, dir: &Path) -> u64 {
        let mut size = 0u64;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        size += metadata.len();
                    } else if metadata.is_dir() {
                        size += self.dir_size(&entry.path());
                    }
                }
            }
        }
        size
    }
}

/// Get the default resource directory (in app data)
pub fn default_resource_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("resources")
}

/// Bundled ASR resources (ONNX models) shipped with the installer.
/// These are in asr-bundle/ and referenced via Tauri's resource API.
pub fn get_bundled_resource_dir() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let bundle = exe_dir.join("asr-bundle");
            if bundle.exists() {
                return Some(bundle);
            }
            // Installed layout: Tauri places bundle.resources under resources/
            let installed = exe_dir.join("resources").join("asr-bundle");
            if installed.exists() {
                return Some(installed);
            }
            // Dev / cargo run mode: the source bundle sits at
            // src-tauri/asr-bundle, two levels up from target/debug/ (or
            // target/release/).
            let dev_bundle = exe_dir.join("../../asr-bundle");
            if dev_bundle.exists() {
                return std::fs::canonicalize(dev_bundle).ok();
            }
        }
    }
    None
}

/// Extra model directories searched next to the executable (portable mode).
fn portable_model_dirs(engine: ResourceEngine) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            dirs.push(exe_dir.join("models").join(engine.as_str()));
            dirs.push(exe_dir.join("models"));
        }
    }
    dirs
}

/// Find a file by name across a list of candidate directories.
fn find_in_dirs(dirs: &[std::path::PathBuf], filename: &str) -> Option<std::path::PathBuf> {
    for dir in dirs {
        let candidate = dir.join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Extract bundled resources (no-op now, models are pre-placed in asr-bundle)
/// Kept for backward compatibility during transition
pub fn ensure_bundled_resources(_app: &tauri::AppHandle) -> anyhow::Result<()> {
    // sherpa-onnx models are statically bundled in asr-bundle/, no extraction needed
    Ok(())
}
