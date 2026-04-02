use crate::backend::SharedEngine;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct GenerateRequest {
    cmd: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct LLMResponse {
    #[serde(rename = "type")]
    response_type: String,
    message: Option<String>,
    token: Option<String>,
    full_response: Option<String>,
}

pub struct AIModel {
    engine: SharedEngine,
    conversation: Vec<Message>,
}

impl AIModel {
    /// Create a new AIModel using the shared engine
    /// The engine should already be started; this will initialize the LLM service
    pub fn new(engine: SharedEngine) -> Result<Self, String> {
        // Initialize LLM service on the shared engine
        {
            let mut eng = engine.lock().map_err(|e| format!("Engine lock error: {}", e))?;
            eng.init_service("llm")?;
        }

        Ok(Self {
            engine,
            conversation: Vec::new(),
        })
    }

    pub fn generate<F>(
        &mut self,
        prompt: &str,
        _max_tokens: usize,
        mut callback: F,
    ) -> Result<String, String>
    where
        F: FnMut(String),
    {
        // Add user message to conversation history
        self.conversation.push(Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        // Send generate request
        let request = GenerateRequest {
            cmd: "generate".to_string(),
            messages: self.conversation.clone(),
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;

        // Lock engine and perform request/response
        let full_response = {
            let mut engine = self
                .engine
                .lock()
                .map_err(|e| format!("Engine lock error: {}", e))?;

            engine.send_command(&request_json)?;

            // Read streaming response
            let full_response;

            loop {
                let line = engine.read_line()?;

                let response: LLMResponse = serde_json::from_str(&line)
                    .map_err(|e| format!("Failed to parse response: {} - line: {}", e, line))?;

                match response.response_type.as_str() {
                    "generation_start" => {
                        // Generation started
                    }
                    "token" => {
                        if let Some(token) = response.token {
                            callback(token);
                        }
                    }
                    "generation_end" => {
                        full_response = response.full_response.unwrap_or_default();
                        break;
                    }
                    "status" => {
                        // Ignore status messages during generation
                    }
                    "error" => {
                        return Err(format!(
                            "Generation error: {}",
                            response.message.unwrap_or_default()
                        ));
                    }
                    _ => {}
                }
            }

            full_response
        };

        // Add assistant response to conversation history
        self.conversation.push(Message {
            role: "assistant".to_string(),
            content: full_response.clone(),
        });

        Ok(full_response)
    }

    pub fn clear_history(&mut self) {
        self.conversation.clear();
    }
}
