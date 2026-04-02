use crate::backend::SharedEngine;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use regex::Regex;
use rodio::{Decoder, OutputStream, Sink};
use serde::Deserialize;
use std::io::Cursor;

#[derive(Deserialize, Debug)]
struct TTSResponse {
    #[serde(rename = "type")]
    response_type: String,
    message: Option<String>,
    audio: Option<String>,
    #[allow(dead_code)]
    sample_rate: Option<u32>,
    #[allow(dead_code)]
    chunk_index: Option<u32>,
    #[allow(dead_code)]
    total_chunks: Option<u32>,
}

pub struct TTSModel {
    engine: SharedEngine,
}

impl TTSModel {
    /// Create a new TTSModel using the shared engine
    /// The engine should already be started; this will initialize the TTS service
    pub fn new(engine: SharedEngine) -> Result<Self, String> {
        // Initialize TTS service on the shared engine
        {
            let mut eng = engine.lock().map_err(|e| format!("Engine lock error: {}", e))?;
            eng.init_service("tts")?;
        }

        Ok(Self { engine })
    }

    /// Speak text with streaming audio (low latency - plays chunks as they arrive)
    pub fn speak(&mut self, text: &str) -> Result<(), String> {
        let clean_text = Self::clean_text_for_tts(text);
        if clean_text.is_empty() {
            return Ok(()); // Nothing to speak
        }
        self.speak_streaming(&clean_text)
    }

    /// Clean text for TTS by removing emojis, markdown formatting, and invalid characters
    fn clean_text_for_tts(text: &str) -> String {
        let mut result = text.to_string();

        // Remove markdown formatting (preserving the text content)
        // Bold **text** or __text__
        if let Ok(re) = Regex::new(r"\*\*(.+?)\*\*") {
            result = re.replace_all(&result, "$1").to_string();
        }
        if let Ok(re) = Regex::new(r"__(.+?)__") {
            result = re.replace_all(&result, "$1").to_string();
        }
        // Italic *text* or _text_
        if let Ok(re) = Regex::new(r"\*(.+?)\*") {
            result = re.replace_all(&result, "$1").to_string();
        }
        if let Ok(re) = Regex::new(r"_(.+?)_") {
            result = re.replace_all(&result, "$1").to_string();
        }
        // Code `text`
        if let Ok(re) = Regex::new(r"`(.+?)`") {
            result = re.replace_all(&result, "$1").to_string();
        }
        // Strikethrough ~~text~~
        if let Ok(re) = Regex::new(r"~~(.+?)~~") {
            result = re.replace_all(&result, "$1").to_string();
        }
        // Links [text](url) -> text
        if let Ok(re) = Regex::new(r"\[(.+?)\]\(.+?\)") {
            result = re.replace_all(&result, "$1").to_string();
        }
        // Headers # text -> text
        if let Ok(re) = Regex::new(r"(?m)^#+\s+") {
            result = re.replace_all(&result, "").to_string();
        }
        // Code blocks ```...``` -> remove entirely
        if let Ok(re) = Regex::new(r"(?s)```.*?```") {
            result = re.replace_all(&result, "").to_string();
        }

        // Remove emojis and other problematic Unicode characters
        // Filter out characters that can cause UTF-8 encoding issues
        result = result
            .chars()
            .filter(|c| {
                let code = *c as u32;
                // Keep basic ASCII and common extended Latin
                // Remove emojis (U+1F300-U+1F9FF), surrogate pairs, and other problematic ranges
                !(
                    // Emoticons and symbols
                    (0x1F600..=0x1F64F).contains(&code) ||  // Emoticons
                    (0x1F300..=0x1F5FF).contains(&code) ||  // Misc symbols & pictographs
                    (0x1F680..=0x1F6FF).contains(&code) ||  // Transport & map symbols
                    (0x1F1E0..=0x1F1FF).contains(&code) ||  // Flags
                    (0x1F900..=0x1F9FF).contains(&code) ||  // Supplemental symbols
                    (0x1FA00..=0x1FA6F).contains(&code) ||  // Chess symbols
                    (0x1FA70..=0x1FAFF).contains(&code) ||  // Symbols extended-A
                    (0x2600..=0x26FF).contains(&code) ||    // Misc symbols
                    (0x2700..=0x27BF).contains(&code) ||    // Dingbats
                    (0x2300..=0x23FF).contains(&code) ||    // Misc technical
                    (0xFE00..=0xFE0F).contains(&code) ||    // Variation selectors
                    (0xD800..=0xDFFF).contains(&code) ||    // Surrogate pairs (invalid in UTF-8)
                    (0xFFF0..=0xFFFF).contains(&code)       // Specials
                )
            })
            .collect();

        // Normalize whitespace
        if let Ok(re) = Regex::new(r"\s+") {
            result = re.replace_all(&result, " ").to_string();
        }

        result.trim().to_string()
    }

    /// Streaming TTS - plays audio chunks as they are generated
    /// Note: Audio playback starts as soon as the first chunk is received.
    /// However, there may be a perceived delay because Piper generates audio
    /// in sentence chunks, and each chunk takes time to synthesize.
    fn speak_streaming(&mut self, text: &str) -> Result<(), String> {
        // Send synthesize request with streaming enabled
        let request = serde_json::json!({
            "cmd": "synthesize",
            "text": text,
            "streaming": true
        });

        // Create audio output for streaming playback
        let (_output_stream, stream_handle) =
            OutputStream::try_default().map_err(|e| format!("Failed to get audio output: {}", e))?;

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        // Lock engine and perform request/response
        {
            let mut engine = self
                .engine
                .lock()
                .map_err(|e| format!("Engine lock error: {}", e))?;

            engine.send_command(&request.to_string())?;

            // Read and play chunks as they arrive
            loop {
                let line = engine.read_line()?;

                let response: TTSResponse = serde_json::from_str(&line)
                    .map_err(|e| format!("Failed to parse response: {} - line: {}", e, line))?;

                match response.response_type.as_str() {
                    "audio_chunk" => {
                        let audio_b64 = response.audio.ok_or("No audio data in chunk")?;
                        let wav_bytes = BASE64
                            .decode(&audio_b64)
                            .map_err(|e| format!("Failed to decode audio chunk: {}", e))?;

                        // Decode and queue the chunk for playback
                        let cursor = Cursor::new(wav_bytes);
                        let source = Decoder::new(cursor)
                            .map_err(|e| format!("Failed to decode WAV chunk: {}", e))?;
                        sink.append(source);
                    }
                    "audio_done" => {
                        // Exit the loop, we'll wait for audio outside the lock
                        break;
                    }
                    "error" => {
                        return Err(format!(
                            "Synthesis error: {}",
                            response.message.unwrap_or_default()
                        ));
                    }
                    _ => {}
                }
            }
        }

        // Wait for all queued audio to finish playing (outside the lock)
        sink.sleep_until_end();
        Ok(())
    }
}
