//! Backend process launcher utility
//! Supports the combined AIVAEngine for all ML services (LLM, STT, TTS)

use serde::Deserialize;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

/// Shared engine type for use across services
pub type SharedEngine = Arc<Mutex<AIVAEngine>>;

/// Response from the AIVA Engine
#[derive(Deserialize, Debug)]
struct EngineResponse {
    #[serde(rename = "type")]
    response_type: String,
    message: Option<String>,
}

/// Combined AIVA Engine that handles LLM, STT, and TTS
pub struct AIVAEngine {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl AIVAEngine {
    /// Create a new AIVAEngine instance
    pub fn new() -> Result<Self, String> {
        let backend_dir = find_backend_dir()?;
        let mut process = launch_engine(&backend_dir)?;

        let stdin = process
            .stdin
            .take()
            .ok_or("Failed to get stdin handle")?;
        let stdout = process
            .stdout
            .take()
            .ok_or("Failed to get stdout handle")?;

        let mut engine = Self {
            process,
            stdin,
            stdout: BufReader::new(stdout),
        };

        // Wait for engine to be ready
        engine.wait_for_ready()?;

        Ok(engine)
    }

    /// Wait for engine startup
    fn wait_for_ready(&mut self) -> Result<(), String> {
        loop {
            let line = self.read_line()?;

            let response: EngineResponse = serde_json::from_str(&line)
                .map_err(|e| format!("Failed to parse response: {} - line: {}", e, line))?;

            match response.response_type.as_str() {
                "status" => {
                    let msg = response.message.unwrap_or_default();
                    println!("Engine: {}", msg);
                    if msg.contains("AIVA Engine ready") {
                        return Ok(());
                    }
                }
                "error" => {
                    return Err(format!(
                        "Engine error: {}",
                        response.message.unwrap_or_default()
                    ));
                }
                _ => {}
            }
        }
    }

    /// Initialize a specific service (llm, stt, or tts)
    pub fn init_service(&mut self, service: &str) -> Result<(), String> {
        let request = serde_json::json!({
            "cmd": "init",
            "service": service
        });

        self.send_command(&request.to_string())?;

        // Wait for service to load
        loop {
            let line = self.read_line()?;
            let response: EngineResponse = serde_json::from_str(&line)
                .map_err(|e| format!("Failed to parse response: {} - line: {}", e, line))?;

            match response.response_type.as_str() {
                "status" => {
                    let msg = response.message.unwrap_or_default();
                    println!("{} service: {}", service.to_uppercase(), msg);
                    if msg.contains("loaded successfully") {
                        return Ok(());
                    }
                }
                "error" => {
                    return Err(format!(
                        "{} init error: {}",
                        service.to_uppercase(),
                        response.message.unwrap_or_default()
                    ));
                }
                _ => {}
            }
        }
    }

    /// Send a command to the engine
    pub fn send_command(&mut self, cmd: &str) -> Result<(), String> {
        writeln!(self.stdin, "{}", cmd)
            .map_err(|e| format!("Failed to write to engine: {}", e))?;
        self.stdin
            .flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;
        Ok(())
    }

    /// Read a single line from the engine
    /// Uses lossy UTF-8 conversion to handle potentially invalid characters
    pub fn read_line(&mut self) -> Result<String, String> {
        let mut bytes = Vec::new();

        // Read byte by byte until we hit a newline
        loop {
            let mut byte = [0u8; 1];
            match self.stdout.read(&mut byte) {
                Ok(0) => {
                    // EOF
                    if bytes.is_empty() {
                        return Err("Engine closed unexpectedly".to_string());
                    }
                    break;
                }
                Ok(_) => {
                    if byte[0] == b'\n' {
                        break;
                    }
                    bytes.push(byte[0]);
                }
                Err(e) => {
                    return Err(format!("Failed to read from engine: {}", e));
                }
            }
        }

        // Convert to string, replacing invalid UTF-8 with replacement character
        let line = String::from_utf8_lossy(&bytes).to_string();

        if line.is_empty() && bytes.is_empty() {
            return Err("Engine closed unexpectedly".to_string());
        }

        Ok(line)
    }
}

impl Drop for AIVAEngine {
    fn drop(&mut self) {
        // Send quit command
        let _ = writeln!(self.stdin, r#"{{"cmd": "quit"}}"#);
        let _ = self.stdin.flush();

        // Wait a moment for graceful shutdown
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = self.process.kill();
    }
}

/// Find the backend directory
pub fn find_backend_dir() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("Failed to get cwd: {}", e))?;

    // Also check relative to the executable location
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let mut candidates = vec![
        cwd.join("backend"),
        cwd.parent()
            .map(|p| p.join("backend"))
            .unwrap_or_default(),
    ];

    // Add exe-relative paths for packaged distribution
    if let Some(exe_dir) = exe_dir {
        candidates.push(exe_dir.join("backend"));
        candidates.push(
            exe_dir
                .parent()
                .map(|p| p.join("backend"))
                .unwrap_or_default(),
        );
    }

    for candidate in &candidates {
        // Check for combined engine or separate servers
        if candidate.join("AIVAEngine.exe").exists()
            || candidate.join("aiva_engine.py").exists()
            || candidate.join("llm_server.py").exists()
        {
            return Ok(candidate.clone());
        }
    }

    Err(format!(
        "Could not find backend directory. Searched: {:?}",
        candidates
    ))
}

/// Launch the AIVA Engine process
fn launch_engine(backend_dir: &PathBuf) -> Result<Child, String> {
    // Check for standalone exe first (packaged distribution)
    let exe_path = backend_dir.join("AIVAEngine.exe");
    if exe_path.exists() {
        println!("Starting AIVA Engine (standalone exe)...");
        return Command::new(&exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .current_dir(backend_dir)
            .spawn()
            .map_err(|e| format!("Failed to start AIVA Engine: {}", e));
    }

    // Fall back to Python script (development mode)
    let script_path = backend_dir.join("aiva_engine.py");
    let python_exe = backend_dir.join("Scripts").join("python.exe");

    if !python_exe.exists() {
        return Err(format!(
            "Python venv not found. Expected: {}",
            python_exe.display()
        ));
    }

    if !script_path.exists() {
        return Err(format!(
            "AIVA Engine script not found at: {}",
            script_path.display()
        ));
    }

    println!("Starting AIVA Engine (Python script)...");
    Command::new(&python_exe)
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .current_dir(backend_dir)
        .spawn()
        .map_err(|e| format!("Failed to start AIVA Engine: {}", e))
}
