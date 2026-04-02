use crate::backend::SharedEngine;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct STTResponse {
    #[serde(rename = "type")]
    response_type: String,
    message: Option<String>,
    text: Option<String>,
    language: Option<String>,
    #[allow(dead_code)]
    language_probability: Option<f64>,
}

pub struct STTModel {
    engine: SharedEngine,
}

impl STTModel {
    /// Create a new STTModel using the shared engine
    /// The engine should already be started; this will initialize the STT service
    pub fn new(engine: SharedEngine) -> Result<Self, String> {
        // Initialize STT service on the shared engine
        {
            let mut eng = engine.lock().map_err(|e| format!("Engine lock error: {}", e))?;
            eng.init_service("stt")?;
        }

        Ok(Self { engine })
    }

    pub fn transcribe(&mut self, audio_wav: &[u8]) -> Result<String, String> {
        // Encode audio as base64
        let audio_b64 = BASE64.encode(audio_wav);

        // Send transcribe request
        let request = serde_json::json!({
            "cmd": "transcribe",
            "audio": audio_b64
        });

        let request_json = request.to_string();

        // Lock engine and perform request/response
        let mut engine = self
            .engine
            .lock()
            .map_err(|e| format!("Engine lock error: {}", e))?;

        engine.send_command(&request_json)?;

        // Read response
        loop {
            let line = engine.read_line()?;

            let response: STTResponse = serde_json::from_str(&line)
                .map_err(|e| format!("Failed to parse response: {} - line: {}", e, line))?;

            match response.response_type.as_str() {
                "transcription" => {
                    let text = response.text.unwrap_or_default();
                    if let Some(lang) = response.language {
                        println!(
                            "Transcribed ({}): {}",
                            lang,
                            if text.len() > 50 {
                                format!("{}...", &text[..50])
                            } else {
                                text.clone()
                            }
                        );
                    }
                    return Ok(text);
                }
                "error" => {
                    return Err(format!(
                        "Transcription error: {}",
                        response.message.unwrap_or_default()
                    ));
                }
                _ => {}
            }
        }
    }
}
