// Local ASR Resource Manager
// Manages bundled and downloaded resource packages for Whisper.cpp and FunASR
// Supports version channels (stable/preview) and update mechanisms

pub mod server;

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

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
    /// Engine type
    pub engine: ResourceEngine,
    /// Version string (e.g., "1.0.0")
    pub version: String,
    /// Version channel
    pub channel: VersionChannel,
    /// Installation path
    pub path: PathBuf,
    /// Whether this is a bundled resource
    pub is_bundled: bool,
    /// Package size in bytes
    pub size_bytes: u64,
    /// Last updated timestamp
    pub updated_at: String,
    /// Whether the package is ready to use
    pub is_ready: bool,
}

/// Resource manager state
pub struct ResourceManager {
    /// Base directory for all resources
    base_dir: PathBuf,
    /// Installed packages
    packages: HashMap<ResourceEngine, ResourcePackageInfo>,
}

use std::collections::HashMap;

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(base_dir: PathBuf) -> Self {
        let mut manager = Self {
            base_dir,
            packages: HashMap::new(),
        };
        manager.scan_packages();
        manager
    }

    /// Get the resource directory for an engine
    fn engine_dir(&self, engine: ResourceEngine) -> PathBuf {
        self.base_dir.join(engine.as_str())
    }

    /// Scan for installed packages
    pub fn scan_packages(&mut self) {
        for engine in [ResourceEngine::WhisperCpp, ResourceEngine::FunASR] {
            let dir = self.engine_dir(engine);
            if dir.exists() {
                let is_ready = self.check_package_ready(engine, &dir);
                let size = self.dir_size(&dir);
                let info = ResourcePackageInfo {
                    engine,
                    version: self.read_version(&dir),
                    channel: VersionChannel::Stable, // Default
                    path: dir.clone(),
                    is_bundled: dir.join(".bundled").exists(),
                    size_bytes: size,
                    updated_at: self.read_updated_at(&dir),
                    is_ready,
                };
                self.packages.insert(engine, info);
            }
        }
    }

    /// Check if a package is ready to use
    fn check_package_ready(&self, engine: ResourceEngine, dir: &Path) -> bool {
        match engine {
            ResourceEngine::WhisperCpp => {
                // Check for whisper.cpp server binary and model file
                let server_binary = if cfg!(windows) {
                    dir.join("whisper-server.exe")
                } else {
                    dir.join("whisper-server")
                };
                let model_file = dir.join("models");
                server_binary.exists() && model_file.exists()
            }
            ResourceEngine::FunASR => {
                // Check for FunASR server script and model files
                let server_script = dir.join("server.py");
                let model_dir = dir.join("models");
                server_script.exists() && model_dir.exists()
            }
        }
    }

    /// Get the default server endpoint for an engine
    pub fn get_endpoint(&self, engine: ResourceEngine) -> String {
        match engine {
            ResourceEngine::WhisperCpp => "ws://localhost:8080/ws".to_string(),
            ResourceEngine::FunASR => "ws://localhost:9880/ws".to_string(),
        }
    }

    /// Get package info for an engine
    pub fn get_package(&self, engine: ResourceEngine) -> Option<&ResourcePackageInfo> {
        self.packages.get(&engine)
    }

    /// Get all package info
    pub fn get_all_packages(&self) -> Vec<&ResourcePackageInfo> {
        self.packages.values().collect()
    }

    /// Install a resource package from a source path
    pub fn install_package(
        &mut self,
        engine: ResourceEngine,
        source_path: &Path,
        version: &str,
        channel: VersionChannel,
    ) -> anyhow::Result<ResourcePackageInfo> {
        let target_dir = self.engine_dir(engine);

        // Create target directory if it doesn't exist
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)?;
        }
        std::fs::create_dir_all(&target_dir)?;

        // Copy files from source to target
        self.copy_dir_recursive(source_path, &target_dir)?;

        // Write version file
        let version_file = target_dir.join(".version");
        std::fs::write(version_file, version)?;

        // Write channel file
        let channel_file = target_dir.join(".channel");
        std::fs::write(channel_file, channel.as_str())?;

        // Write updated_at file
        let updated_file = target_dir.join(".updated_at");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        std::fs::write(updated_file, now.to_string())?;

        // Scan again to update state
        self.scan_packages();

        Ok(self.packages.get(&engine).unwrap().clone())
    }

    /// Remove a resource package
    pub fn remove_package(&mut self, engine: ResourceEngine) -> anyhow::Result<()> {
        let dir = self.engine_dir(engine);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        self.packages.remove(&engine);
        Ok(())
    }

    /// Get the command to start the local server
    pub fn get_start_command(&self, engine: ResourceEngine) -> Option<Vec<String>> {
        let package = self.packages.get(&engine)?;
        if !package.is_ready {
            return None;
        }

        match engine {
            ResourceEngine::WhisperCpp => {
                let server = package.path.join(if cfg!(windows) {
                    "whisper-server.exe"
                } else {
                    "whisper-server"
                });
                let model = package.path.join("models");
                Some(vec![
                    server.to_str()?.to_string(),
                    "--model".to_string(),
                    model.to_str()?.to_string(),
                    "--port".to_string(),
                    "8080".to_string(),
                ])
            }
            ResourceEngine::FunASR => {
                let server = package.path.join("server.py");
                Some(vec![
                    "python".to_string(),
                    server.to_str()?.to_string(),
                    "--port".to_string(),
                    "9880".to_string(),
                ])
            }
        }
    }

    // Helper methods

    fn read_version(&self, dir: &Path) -> String {
        let version_file = dir.join(".version");
        std::fs::read_to_string(version_file).unwrap_or_else(|_| "unknown".to_string())
    }

    fn read_updated_at(&self, dir: &Path) -> String {
        let file = dir.join(".updated_at");
        let timestamp: u64 = std::fs::read_to_string(file)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if timestamp == 0 {
            "unknown".to_string()
        } else {
            let datetime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
            format!("{:?}", datetime)
        }
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

    fn copy_dir_recursive(&self, src: &Path, dst: &Path) -> anyhow::Result<()> {
        if !dst.exists() {
            std::fs::create_dir_all(dst)?;
        }
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if file_type.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }
}

/// Get the default resource directory (in app data)
pub fn default_resource_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("resources")
}
