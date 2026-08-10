// Local ASR Server Manager
// Manages the lifecycle of local ASR server processes (Whisper.cpp and FunASR)
// Handles starting, stopping, and health-checking the local servers

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use super::{ResourceEngine, ResourceManager};

/// Server configuration for local ASR
#[derive(Clone)]
pub struct ServerConfig {
    pub engine: ResourceEngine,
    pub host: String,
    pub port: u16,
    pub model_path: PathBuf,
    pub vad_model_path: Option<PathBuf>,
    pub language: String,
    pub threads: u32,
}

/// Running server instance
pub struct ServerInstance {
    pub engine: ResourceEngine,
    pub config: ServerConfig,
    pub process: Child,
}

/// Manages local ASR server processes
pub struct ServerManager {
    instances: Mutex<Vec<ServerInstance>>,
}

impl ServerManager {
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(Vec::new()),
        }
    }

    /// Start a local ASR server
    pub fn start_server(
        &self,
        config: ServerConfig,
        resource_manager: &ResourceManager,
    ) -> anyhow::Result<String> {
        // Check if server is already running for this engine
        self.stop_server(config.engine)?;

        let mut cmd = self.build_command(&config, resource_manager)?;

        // Spawn the process
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start {} server: {}", config.engine.display_name(), e))?;

        let endpoint = format!("ws://{}:{}", config.host, config.port);

        let instance = ServerInstance {
            engine: config.engine,
            config: config.clone(),
            process: child,
        };

        self.instances.lock().unwrap().push(instance);

        log::info!("Started {} server at {}", config.engine.display_name(), endpoint);
        Ok(endpoint)
    }

    /// Stop a running server for an engine
    pub fn stop_server(&self, engine: ResourceEngine) -> anyhow::Result<()> {
        let mut instances = self.instances.lock().unwrap();
        if let Some(pos) = instances.iter().position(|i| i.engine == engine) {
            let mut instance = instances.remove(pos);
            match instance.process.kill() {
                Ok(_) => log::info!("Stopped {} server", engine.display_name()),
                Err(e) => log::warn!("Failed to kill {} server: {}", engine.display_name(), e),
            }
            // Wait for process to exit
            let _ = instance.process.wait();
        }
        Ok(())
    }

    /// Stop all running servers
    pub fn stop_all(&self) {
        let mut instances = self.instances.lock().unwrap();
        for instance in instances.iter_mut() {
            let _ = instance.process.kill();
            let _ = instance.process.wait();
        }
        instances.clear();
    }

    /// Check if a server is running for an engine
    pub fn is_running(&self, engine: ResourceEngine) -> bool {
        let mut instances = self.instances.lock().unwrap();
        if let Some(pos) = instances.iter().position(|i| i.engine == engine) {
            // Try to check if process is still alive
            match instances[pos].process.try_wait() {
                Ok(None) => true, // Still running
                Ok(Some(_)) => {
                    // Process exited, remove it
                    instances.remove(pos);
                    false
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Get the endpoint for a running server
    pub fn get_endpoint(&self, engine: ResourceEngine) -> Option<String> {
        let instances = self.instances.lock().unwrap();
        instances
            .iter()
            .find(|i| i.engine == engine)
            .map(|i| format!("ws://{}:{}", i.config.host, i.config.port))
    }

    /// Build the command for the server
    fn build_command(
        &self,
        config: &ServerConfig,
        resource_manager: &ResourceManager,
    ) -> anyhow::Result<Command> {
        match config.engine {
            ResourceEngine::WhisperCpp => {
                let package = resource_manager.get_package(ResourceEngine::WhisperCpp)
                    .ok_or_else(|| anyhow::anyhow!("Whisper.cpp resource not installed"))?;

                let server_binary = find_server_binary(&package.path, "whisper-server.exe")?;

                let mut cmd = Command::new(&server_binary);
                cmd.arg("--host").arg(&config.host);
                cmd.arg("--port").arg(config.port.to_string());
                cmd.arg("--model").arg(&config.model_path);
                cmd.arg("--language").arg(&config.language);
                cmd.arg("--threads").arg(config.threads.to_string());

                if let Some(vad_path) = &config.vad_model_path {
                    cmd.arg("--vad-model").arg(vad_path);
                }

                Ok(cmd)
            }
            ResourceEngine::FunASR => {
                let package = resource_manager.get_package(ResourceEngine::FunASR)
                    .ok_or_else(|| anyhow::anyhow!("FunASR resource not installed"))?;

                let server_binary = find_server_binary(&package.path, "llama-funasr-sensevoice.exe")?;

                let mut cmd = Command::new(&server_binary);
                cmd.arg("--host").arg(&config.host);
                cmd.arg("--port").arg(config.port.to_string());
                cmd.arg("-m").arg(&config.model_path);
                cmd.arg("--language").arg(&config.language);

                if let Some(vad_path) = &config.vad_model_path {
                    cmd.arg("--vad").arg(vad_path);
                }

                Ok(cmd)
            }
        }
    }
}

/// Find a server binary in the package directory
fn find_server_binary(package_path: &PathBuf, binary_name: &str) -> anyhow::Result<PathBuf> {
    // Check common locations
    let candidates = [
        package_path.join(binary_name),
        package_path.join("Release").join(binary_name),
        package_path.join("bin").join(binary_name),
        package_path.join("bin").join("Release").join(binary_name),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // Manual recursive search (limited depth)
    if let Ok(entries) = std::fs::read_dir(package_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let sub = path.join(binary_name);
                if sub.exists() {
                    return Ok(sub);
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Server binary '{}' not found in {}",
        binary_name,
        package_path.display()
    ))
}

impl Drop for ServerManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}
