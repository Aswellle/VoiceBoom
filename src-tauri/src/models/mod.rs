// Model Manager — local ASR model lifecycle
//
// Implements the Phase 2 spec: registry parsing, download progress tracking,
// atomic install via .staging, and active.json version management.
//
// Directory layout (per spec section 5):
//   %LOCALAPPDATA%\VoiceBoom\models\            (Windows)
//   ~/Library/Application Support/VoiceBoom/models/  (macOS)
//     ├── sensevoice/
//     │   └── 1.0.0/
//     │       ├── manifest.json
//     │       ├── model.int8.onnx
//     │       ├── tokens.txt
//     │       └── silero_vad.onnx
//     └── active.json  {"sensevoice": "1.0.0"}

pub mod downloader;
pub mod installer;
pub mod verifier;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::models::downloader::DownloadHandle;

/// Runtime model state machine (spec section 9).
/// The frontend renders UI based on this — never guess from filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    NotInstalled,
    Downloading,
    Verifying,
    Installing,
    Ready,
    Corrupt,
    UpdateAvailable,
    Unavailable,
}

/// Top-level registry, parsed from `models/registry.json` (spec section 6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistry {
    pub schema_version: u32,
    pub channel: String,
    pub models: Vec<ModelInfo>,
}

/// A single downloadable model entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub engine: String,
    pub version: String,
    pub languages: Vec<String>,
    pub platforms: Vec<String>,
    pub archive: ArchiveInfo,
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// Per-model status returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub id: String,
    pub engine: String,
    pub version: String,
    pub state: ModelState,
    pub installed_versions: Vec<String>,
    pub active_version: Option<String>,
    pub size_bytes: u64,
    pub languages: Vec<String>,
    /// Download progress (0.0–1.0) when state == Downloading.
    pub progress: Option<f64>,
    pub error: Option<String>,
}

/// Maps engine -> active version. Persisted to `active.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveMap {
    #[serde(flatten)]
    pub engines: HashMap<String, String>,
}

/// Coordinates download, verification, and installation of models.
/// Lives in AppState behind an `Arc<RwLock<...>>`.
#[derive(Clone)]
pub struct ModelManager {
    /// Root models directory (e.g. `%LOCALAPPDATA%\VoiceBoom\models\`).
    models_dir: PathBuf,
    /// Parsed registry.
    registry: ModelRegistry,
    /// Currently in-flight downloads, keyed by model id.
    downloads: Arc<RwLock<HashMap<String, DownloadHandle>>>,
}

impl ModelManager {
    /// Create a manager. `models_dir` is created if missing.
    pub fn new(models_dir: PathBuf, registry: ModelRegistry) -> Self {
        std::fs::create_dir_all(&models_dir).ok();
        Self {
            models_dir,
            registry,
            downloads: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a manager with an embedded registry (bundled at compile time).
    pub fn new_embedded(models_dir: PathBuf) -> Self {
        let registry = Self::embedded_registry();
        std::fs::create_dir_all(&models_dir).ok();
        Self {
            models_dir,
            registry,
            downloads: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the models directory (called during Tauri setup once app_handle is available).
    pub fn init_models_dir(&mut self, models_dir: PathBuf) {
        self.models_dir = models_dir;
        std::fs::create_dir_all(&self.models_dir).ok();
    }

    /// Registry bundled into the binary at compile time.
    fn embedded_registry() -> ModelRegistry {
        let json = include_str!("../../../models/registry.json");
        serde_json::from_str(json).unwrap_or_else(|e| {
            log::error!("Failed to parse embedded models/registry.json: {e}");
            ModelRegistry {
                schema_version: 1,
                channel: "stable".into(),
                models: vec![],
            }
        })
    }

    /// Returns the directory a specific model version lives in:
    /// `<models_dir>/<engine>/<version>/`
    pub fn version_dir(&self, engine: &str, version: &str) -> PathBuf {
        self.models_dir.join(engine).join(version)
    }

    /// Load `active.json`, or empty if missing/corrupt.
    pub fn load_active(&self) -> ActiveMap {
        let path = self.models_dir.join("active.json");
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(map) = serde_json::from_slice::<ActiveMap>(&bytes) {
                return map;
            }
        }
        ActiveMap::default()
    }

    /// Persist `active.json`.
    pub fn save_active(&self, active: &ActiveMap) -> anyhow::Result<()> {
        let path = self.models_dir.join("active.json");
        let bytes = serde_json::to_vec_pretty(active)?;
        std::fs::write(&path, bytes)?;
        Ok(())
    }

    /// List every model in the registry with its current runtime status.
    pub fn list_models(&self) -> Vec<ModelStatus> {
        let active = self.load_active();
        self.registry
            .models
            .iter()
            .map(|info| self.status_for(info, &active))
            .collect()
    }

    /// Get status for one model by id.
    pub fn get_status(&self, model_id: &str) -> Option<ModelStatus> {
        let active = self.load_active();
        let info = self.registry.models.iter().find(|m| m.id == model_id)?;
        Some(self.status_for(info, &active))
    }

    fn status_for(&self, info: &ModelInfo, active: &ActiveMap) -> ModelStatus {
        let installed = self.installed_versions(&info.engine);
        let active_version = active.engines.get(&info.engine).cloned();

        let state = if installed.contains(&info.version) {
            match &active_version {
                Some(v) if v == &info.version => ModelState::Ready,
                _ => ModelState::NotInstalled,
            }
        } else {
            ModelState::NotInstalled
        };

        let size_bytes = self
            .version_dir(&info.engine, &info.version)
            .metadata()
            .map(|m| {
                if m.is_file() {
                    m.len()
                } else {
                    dir_size(&self.version_dir(&info.engine, &info.version))
                }
            })
            .unwrap_or(0);

        ModelStatus {
            id: info.id.clone(),
            engine: info.engine.clone(),
            version: info.version.clone(),
            state,
            installed_versions: installed,
            active_version,
            size_bytes,
            languages: info.languages.clone(),
            progress: None,
            error: None,
        }
    }

    /// Which versions of an engine are fully present on disk.
    fn installed_versions(&self, engine: &str) -> Vec<String> {
        let engine_dir = self.models_dir.join(engine);
        let mut versions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&engine_dir) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.path().join("manifest.json").exists() {
                    versions.push(name);
                }
            }
        }
        versions.sort();
        versions
    }

    /// Check whether a specific version is fully installed and complete.
    pub fn is_version_ready(&self, engine: &str, version: &str) -> bool {
        self.version_dir(engine, version)
            .join("manifest.json")
            .exists()
    }

    pub fn registry(&self) -> &ModelRegistry {
        &self.registry
    }

    pub fn downloads(&self) -> Arc<RwLock<HashMap<String, DownloadHandle>>> {
        self.downloads.clone()
    }
}

/// Compute total size of a directory recursively.
fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}
