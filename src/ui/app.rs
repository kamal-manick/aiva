use eframe::egui::{self, ScrollArea};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::ai::AIModel;
use crate::backend::AIVAEngine;
use crate::chat::ChatManager;
use crate::stt::{AudioRecorder, STTModel};
use crate::tts::TTSModel;

pub enum AppMessage {
    Token(String),
    Done(String), // Include full response for TTS
    Error(String),
    ModelLoaded(Arc<Mutex<AIModel>>),
    STTLoaded(Arc<Mutex<STTModel>>),
    TTSLoaded(Arc<Mutex<TTSModel>>),
    Transcription(String),
    TTSDone,
}

pub struct ChatApp {
    chat_manager: ChatManager,
    input: String,
    is_loading: bool,
    streaming_response: String,

    msg_receiver: Receiver<AppMessage>,
    msg_sender: Sender<AppMessage>,

    ai_model: Option<Arc<Mutex<AIModel>>>,
    stt_model: Option<Arc<Mutex<STTModel>>>,
    tts_model: Option<Arc<Mutex<TTSModel>>>,
    audio_recorder: Option<AudioRecorder>,

    model_status: String,
    stt_status: String,
    tts_status: String,
    is_recording: bool,
    is_transcribing: bool,
    is_speaking: bool,
    pending_voice_message: Option<String>, // For auto-sending after transcription
}

impl ChatApp {
    pub fn new() -> Self {
        let manager = ChatManager::new(100);

        let (sender, receiver) = channel();

        let mut app = Self {
            chat_manager: manager,
            input: String::new(),
            is_loading: false,
            streaming_response: String::new(),
            msg_receiver: receiver,
            msg_sender: sender,
            ai_model: None,
            stt_model: None,
            tts_model: None,
            audio_recorder: None,
            model_status: "Starting engine...".to_string(),
            stt_status: "Waiting...".to_string(),
            tts_status: "Waiting...".to_string(),
            is_recording: false,
            is_transcribing: false,
            is_speaking: false,
            pending_voice_message: None,
        };

        app.load_models();
        app.init_audio();

        app
    }

    fn load_models(&mut self) {
        let sender = self.msg_sender.clone();

        // Load all models in a single background thread using the shared engine
        thread::spawn(move || {
            // 1. Create shared engine
            let engine = match AIVAEngine::new() {
                Ok(e) => Arc::new(Mutex::new(e)),
                Err(e) => {
                    let _ = sender.send(AppMessage::Error(format!("Engine failed: {}", e)));
                    return;
                }
            };

            // 2. Initialize LLM
            match AIModel::new(engine.clone()) {
                Ok(model) => {
                    let model = Arc::new(Mutex::new(model));
                    let _ = sender.send(AppMessage::ModelLoaded(model));
                }
                Err(e) => {
                    let _ = sender.send(AppMessage::Error(format!("LLM load failed: {}", e)));
                }
            }

            // 3. Initialize STT
            match STTModel::new(engine.clone()) {
                Ok(model) => {
                    let model = Arc::new(Mutex::new(model));
                    let _ = sender.send(AppMessage::STTLoaded(model));
                }
                Err(e) => {
                    let _ = sender.send(AppMessage::Error(format!("STT load failed: {}", e)));
                }
            }

            // 4. Initialize TTS
            match TTSModel::new(engine.clone()) {
                Ok(model) => {
                    let model = Arc::new(Mutex::new(model));
                    let _ = sender.send(AppMessage::TTSLoaded(model));
                }
                Err(e) => {
                    let _ = sender.send(AppMessage::Error(format!("TTS load failed: {}", e)));
                }
            }
        });
    }

    fn init_audio(&mut self) {
        match AudioRecorder::new() {
            Ok(recorder) => {
                self.audio_recorder = Some(recorder);
            }
            Err(e) => {
                eprintln!("Failed to initialize audio recorder: {}", e);
            }
        }
    }

    fn toggle_recording(&mut self) {
        if self.is_recording {
            self.stop_recording();
        } else {
            self.start_recording();
        }
    }

    fn start_recording(&mut self) {
        if let Some(ref mut recorder) = self.audio_recorder {
            match recorder.start_recording() {
                Ok(()) => {
                    self.is_recording = true;
                    self.stt_status = "Recording...".to_string();
                }
                Err(e) => {
                    self.stt_status = format!("Record error: {}", e);
                }
            }
        }
    }

    fn stop_recording(&mut self) {
        if let Some(ref mut recorder) = self.audio_recorder {
            match recorder.stop_recording() {
                Ok(wav_data) => {
                    self.is_recording = false;
                    self.stt_status = "Transcribing...".to_string();
                    self.is_transcribing = true;

                    if let Some(ref stt) = self.stt_model {
                        let stt = stt.clone();
                        let sender = self.msg_sender.clone();

                        thread::spawn(move || {
                            let mut stt = stt.lock().unwrap();
                            match stt.transcribe(&wav_data) {
                                Ok(text) => {
                                    let _ = sender.send(AppMessage::Transcription(text));
                                }
                                Err(e) => {
                                    let _ = sender.send(AppMessage::Error(format!(
                                        "Transcription failed: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    self.is_recording = false;
                    self.stt_status = format!("Record error: {}", e);
                }
            }
        }
    }

    fn send_message(&mut self) {
        self.send_message_internal(false);
    }

    fn send_message_internal(&mut self, from_voice: bool) {
        let trimmed = self.input.trim();
        if trimmed.is_empty() || self.is_loading || self.ai_model.is_none() {
            return;
        }

        self.chat_manager
            .add_message("user".to_string(), trimmed.to_string());

        self.is_loading = true;
        self.streaming_response.clear();

        let user_message = trimmed.to_string();
        let sender = self.msg_sender.clone();
        let model = self.ai_model.clone().unwrap();

        // Remember if this was a voice message (for TTS response)
        if from_voice {
            self.pending_voice_message = Some(user_message.clone());
        }

        thread::spawn(move || {
            let mut model = model.lock().unwrap();

            let result = model.generate(&user_message, 256, |token| {
                let _ = sender.send(AppMessage::Token(token));
            });

            match result {
                Ok(response) => {
                    let _ = sender.send(AppMessage::Done(response));
                }
                Err(e) => {
                    let _ = sender.send(AppMessage::Error(format!("Generation failed: {}", e)));
                }
            }
        });

        self.input.clear();
    }

    fn speak_response(&mut self, text: &str) {
        if let Some(ref tts) = self.tts_model {
            self.is_speaking = true;
            self.tts_status = "Speaking...".to_string();

            let tts = tts.clone();
            let sender = self.msg_sender.clone();
            let text = text.to_string();

            thread::spawn(move || {
                let mut tts = tts.lock().unwrap();
                match tts.speak(&text) {
                    Ok(()) => {
                        let _ = sender.send(AppMessage::TTSDone);
                    }
                    Err(e) => {
                        eprintln!("TTS error: {}", e);
                        let _ = sender.send(AppMessage::TTSDone);
                    }
                }
            });
        }
    }

    fn check_messages(&mut self) {
        while let Ok(msg) = self.msg_receiver.try_recv() {
            match msg {
                AppMessage::Token(token) => {
                    self.streaming_response.push_str(&token);
                }
                AppMessage::Done(full_response) => {
                    if self.is_loading {
                        let response_text = if full_response.is_empty() {
                            self.streaming_response.trim().to_string()
                        } else {
                            full_response
                        };

                        self.chat_manager.add_message(
                            "assistant".to_string(),
                            response_text.clone(),
                        );

                        // If this was from voice input, speak the response
                        if self.pending_voice_message.take().is_some() {
                            self.speak_response(&response_text);
                        }

                        self.streaming_response.clear();
                        self.is_loading = false;
                    }
                }
                AppMessage::ModelLoaded(model) => {
                    self.ai_model = Some(model);
                    self.model_status = "LLM ready!".to_string();
                    self.chat_manager.add_message(
                        "assistant".to_string(),
                        "Hello! I'm your AI assistant. Type or use the microphone to talk to me."
                            .to_string(),
                    );
                }
                AppMessage::STTLoaded(model) => {
                    self.stt_model = Some(model);
                    self.stt_status = "STT ready!".to_string();
                }
                AppMessage::TTSLoaded(model) => {
                    self.tts_model = Some(model);
                    self.tts_status = "TTS ready!".to_string();
                }
                AppMessage::Transcription(text) => {
                    self.is_transcribing = false;
                    if !text.is_empty() {
                        // Auto-send the transcribed text
                        self.input = text;
                        self.stt_status = "STT ready!".to_string();
                        self.send_message_internal(true); // true = from voice
                    } else {
                        self.stt_status = "No speech detected".to_string();
                    }
                }
                AppMessage::TTSDone => {
                    self.is_speaking = false;
                    self.tts_status = "TTS ready!".to_string();
                }
                AppMessage::Error(err) => {
                    eprintln!("Error: {}", err);
                    println!("Error: {}", err);
                    self.is_loading = false;
                    self.is_transcribing = false;
                    self.is_speaking = false;
                }
            }
        }
    }

}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_messages();

        if self.is_loading
            || self.is_recording
            || self.is_transcribing
            || self.is_speaking
            || self.ai_model.is_none()
            || self.stt_model.is_none()
        {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let panel_width = ui.available_width();

            // Header
            ui.horizontal(|ui| {
                ui.heading("AI Voice Assistant");
                ui.separator();
                ui.label(&self.model_status);
                ui.separator();
                ui.label(&self.stt_status);
                ui.separator();
                ui.label(&self.tts_status);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear").clicked() && !self.is_loading {
                        self.chat_manager.clear();
                        if let Some(ref model) = self.ai_model {
                            if let Ok(mut m) = model.lock() {
                                m.clear_history();
                            }
                        }
                        self.chat_manager.add_message(
                            "assistant".to_string(),
                            "Chat cleared. How can I help you?".to_string(),
                        );
                    }
                });
            });

            ui.separator();

            // Chat area
            let available_height = ui.available_height() - 50.0;

            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(available_height)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_width(panel_width);

                    for msg in self.chat_manager.get_messages() {
                        if msg.role == "user" {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Min),
                                |ui| {
                                    ui.label("\u{1F464}");
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&msg.content)
                                                .color(egui::Color32::from_rgb(100, 150, 255)),
                                        )
                                        .wrap(),
                                    );
                                },
                            );
                        } else {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Min),
                                |ui| {
                                    ui.label("\u{2728}");
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&msg.content)
                                                .color(egui::Color32::from_rgb(100, 255, 150)),
                                        )
                                        .wrap(),
                                    );
                                },
                            );
                        }
                        ui.add_space(8.0);
                    }

                    // Show streaming response
                    if self.is_loading && !self.streaming_response.is_empty() {
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.label("\u{2728}");
                                ui.spinner();
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&self.streaming_response)
                                            .color(egui::Color32::from_rgb(100, 255, 150)),
                                    )
                                    .wrap(),
                                );
                            },
                        );
                    } else if self.is_loading {
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.label("\u{2728}");
                                ui.spinner();
                                ui.label("Thinking...");
                            },
                        );
                    }
                });

            ui.separator();

            // Input area
            ui.horizontal(|ui| {
                // Microphone button
                let mic_enabled = self.stt_model.is_some()
                    && !self.is_loading
                    && !self.is_transcribing
                    && !self.is_speaking;

                ui.add_enabled_ui(mic_enabled, |ui| {
                    let mic_text = if self.is_recording {
                        "Stop"
                    } else if self.is_transcribing {
                        "..."
                    } else {
                        "Mic"
                    };

                    let mic_button = egui::Button::new(mic_text);
                    let mic_button = if self.is_recording {
                        mic_button.fill(egui::Color32::from_rgb(255, 100, 100))
                    } else {
                        mic_button
                    };

                    if ui.add_sized([50.0, 30.0], mic_button).clicked() {
                        self.toggle_recording();
                    }
                });

                // Text input
                let text_width = ui.available_width() - 70.0;
                let response = ui.add_sized(
                    [text_width, 30.0],
                    egui::TextEdit::singleline(&mut self.input).hint_text("Type or speak..."),
                );

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.send_message();
                    response.request_focus();
                }

                // Send button
                ui.add_enabled_ui(!self.is_loading && self.ai_model.is_some(), |ui| {
                    if ui
                        .add_sized([60.0, 30.0], egui::Button::new("Send"))
                        .clicked()
                    {
                        self.send_message();
                    }
                });
            });
        });
    }
}
