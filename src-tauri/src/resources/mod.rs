// Local ASR Resource Manager
// Manages bundled server binaries and downloaded model files
// Server binaries are bundled with the app; models are downloaded separately

pub mod server;

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tauri::{self, Manager};

/// Supported local ASR engines
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ResourceEngine {
    WhisperCpp,
    FunASR,
}

impl ResourceEngine {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceEngine::WhisperCpp => "whisper_cpp",
            ResourceEngine::FunASR => "funasr",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ResourceEngine::WhisperCpp => "Whisper.cpp",
            ResourceEngine::FunASR => "FunASR",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "whisper_cpp" => Some(ResourceEngine::WhisperCpp),
            "funasr" => Some(ResourceEngine::FunASR),
            _ => None,
        }
    }

    /// Get the server binary name for this engine
    pub fn server_binary_name(&self) -> &'static str {
        match self {
            ResourceEngine::WhisperCpp => "whisper-server.exe",
            ResourceEngine::FunASR => "llama-funasr-sensevoice.exe",
        }
    }

    /// Get the default WebSocket endpoint
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ResourceEngine::WhisperCpp => "ws://127.0.0.1:8080",
            ResourceEngine::FunASR => "ws://127.0.0.1:9880",
        }
    }

    /// Get the default model file name
    pub fn default_model_filename(&self) -> &'static str {
        match self {
            ResourceEngine::WhisperCpp => "ggml-base.bin",
            ResourceEngine::FunASR => "sensevoice-small-q8.gguf",
        }
    }

    /// Get the VAD model file name
    pub fn vad_model_filename(&self) -> Option<&'static str> {
        match self {
            ResourceEngine::WhisperCpp => None,
            ResourceEngine::FunASR => Some("fsmn-vad.gguf"),
        }
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
    pub server_binary_exists: bool,
    pub model_file_exists: bool,
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

    /// Check if server binary exists
    pub fn server_binary_path(&self, engine: ResourceEngine) -> Option<PathBuf> {
        let dir = self.engine_dir(engine);
        let binary = dir.join(engine.server_binary_name());
        if binary.exists() {
            Some(binary)
        } else {
            // Check Release/ subdirectory
            let release = dir.join("Release").join(engine.server_binary_name());
            if release.exists() {
                Some(release)
            } else {
                None
            }
        }
    }

    /// Get the model file path
    pub fn model_path(&self, engine: ResourceEngine) -> Option<PathBuf> {
        let models_dir = self.models_dir(engine);
        let model_file = models_dir.join(engine.default_model_filename());
        if model_file.exists() {
            Some(model_file)
        } else {
            // Check for any .bin or .gguf file
            if let Ok(entries) = std::fs::read_dir(&models_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        let ext = ext.to_string_lossy().to_lowercase();
                        if ext == "bin" || ext == "gguf" {
                            return Some(path);
                        }
                    }
                }
            }
            None
        }
    }

    /// Get the VAD model file path
    pub fn vad_model_path(&self, engine: ResourceEngine) -> Option<PathBuf> {
        let models_dir = self.models_dir(engine);
        if let Some(vad_name) = engine.vad_model_filename() {
            let vad_file = models_dir.join(vad_name);
            if vad_file.exists() {
                return Some(vad_file);
            }
        }
        None
    }

    /// Get package info for an engine
    pub fn get_package(&self, engine: ResourceEngine) -> Option<ResourcePackageInfo> {
        let dir = self.engine_dir(engine);
        if !dir.exists() {
            return None;
        }

        let server_binary_exists = self.server_binary_path(engine).is_some();
        let model_file_exists = self.model_path(engine).is_some();
        let vad_model_exists = self.vad_model_path(engine).is_some();

        // Ready = server binary exists AND model file exists
        let is_ready = server_binary_exists && model_file_exists;

        Some(ResourcePackageInfo {
            engine,
            version: self.read_version(&dir),
            channel: VersionChannel::Stable,
            path: dir.clone(),
            is_bundled: dir.join(".bundled").exists(),
            size_bytes: self.dir_size(&dir),
            updated_at: self.read_updated_at(&dir),
            server_binary_exists,
            model_file_exists,
            vad_model_exists,
            is_ready,
        })
    }

    /// Get all package info
    pub fn get_all_packages(&self) -> Vec<ResourcePackageInfo> {
        let mut results = Vec::new();
        for engine in [ResourceEngine::WhisperCpp, ResourceEngine::FunASR] {
            if let Some(info) = self.get_package(engine) {
                results.push(info);
            }
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

/// Embedded resource ZIPs (compiled into the binary at build time)
/// This ensures the server binaries are truly built-in and work
/// regardless of where the EXE is located.
pub const EMBEDDED_WHISPER_ZIP: &[u8] = include_bytes!("../../embedded_resources/whisper_cpp.zip");
pub const EMBEDDED_FUNASR_ZIP: &[u8] = include_bytes!("../../embedded_resources/funasr.zip");

/// Extract bundled resources from embedded ZIPs to the app data directory
/// This is called on every launch - it only extracts if not already present.
pub fn ensure_bundled_resources(app: &tauri::AppHandle) -> anyhow::Result<()> {
    let app_data_dir = app.path().app_data_dir()?;
    let resource_dir = default_resource_dir(&app_data_dir);
    std::fs::create_dir_all(&resource_dir)?;

    let whisper_dir = resource_dir.join("whisper_cpp");
    let funasr_dir = resource_dir.join("funasr");

    // Extract whisper.cpp if not already present
    if !is_server_extracted(&whisper_dir) {
        log::info!("Extracting bundled whisper.cpp resources ({} bytes)...", EMBEDDED_WHISPER_ZIP.len());
        extract_zip_to(EMBEDDED_WHISPER_ZIP, &whisper_dir)?;
        std::fs::write(whisper_dir.join(".bundled"), b"1")?;
        std::fs::write(whisper_dir.join(".version"), b"1.9.2")?;
        std::fs::create_dir_all(whisper_dir.join("models")).ok();
        log::info!("Extracted whisper.cpp to {:?}", whisper_dir);
    }

    // Extract FunASR if not already present
    if !is_server_extracted(&funasr_dir) {
        log::info!("Extracting bundled FunASR resources ({} bytes)...", EMBEDDED_FUNASR_ZIP.len());
        extract_zip_to(EMBEDDED_FUNASR_ZIP, &funasr_dir)?;
        std::fs::write(funasr_dir.join(".bundled"), b"1")?;
        std::fs::write(funasr_dir.join(".version"), b"1.4.1")?;
        std::fs::create_dir_all(funasr_dir.join("models")).ok();
        log::info!("Extracted FunASR to {:?}", funasr_dir);
    }

    Ok(())
}

/// Check if server resources have been extracted
fn is_server_extracted(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    // Check for the server binary marker file
    dir.join(".bundled").exists()
}

/// Extract a ZIP byte array to a destination directory
fn extract_zip_to(zip_data: &[u8], dest: &Path) -> anyhow::Result<()> {
    use std::io::Read;

    let cursor = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| anyhow::anyhow!("Failed to open embedded ZIP: {}", e))?;

    std::fs::create_dir_all(dest)?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| anyhow::anyhow!("Failed to read ZIP entry {}: {}", i, e))?;

        // Skip directories
        if file.is_dir() {
            continue;
        }

        let file_name = match file.enclosed_name() {
            Some(name) => name,
            None => continue, // Skip files with unsafe paths
        };

        let out_path = dest.join(file_name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        std::fs::write(&out_path, buffer)?;
    }

    Ok(())
}
