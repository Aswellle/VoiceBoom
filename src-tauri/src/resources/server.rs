// Local ASR Server Process Manager
// Manages the lifecycle of local ASR server processes

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use super::ResourceEngine;

/// Running server instance
pub struct ServerInstance {
    pub engine: ResourceEngine,
    pub port: u16,
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
        engine: ResourceEngine,
        server_binary: PathBuf,
        model_path: PathBuf,
        vad_model_path: Option<PathBuf>,
        language: &str,
        port: u16,
        threads: u32,
    ) -> anyhow::Result<String> {
        // Stop existing server for this engine
        self.stop_server(engine)?;

        let mut cmd = Command::new(&server_binary);

        match engine {
            ResourceEngine::WhisperCpp => {
                cmd.arg("--host").arg("127.0.0.1");
                cmd.arg("--port").arg(port.to_string());
                cmd.arg("--model").arg(&model_path);
                cmd.arg("--language").arg(language);
                cmd.arg("--threads").arg(threads.to_string());
                if let Some(vad) = &vad_model_path {
                    cmd.arg("--vad-model").arg(vad);
                }
            }
            ResourceEngine::FunASR => {
                cmd.arg("--host").arg("127.0.0.1");
                cmd.arg("--port").arg(port.to_string());
                cmd.arg("-m").arg(&model_path);
                cmd.arg("--language").arg(language);
                if let Some(vad) = &vad_model_path {
                    cmd.arg("--vad").arg(vad);
                }
            }
        }

        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start {} server: {}", engine.display_name(), e))?;

        let endpoint = format!("ws://127.0.0.1:{}", port);

        let instance = ServerInstance {
            engine,
            port,
            process: child,
        };

        self.instances.lock().unwrap().push(instance);

        log::info!("Started {} server at {}", engine.display_name(), endpoint);
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
            match instances[pos].process.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => {
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
            .map(|i| format!("ws://127.0.0.1:{}", i.port))
    }
}

impl Drop for ServerManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}
