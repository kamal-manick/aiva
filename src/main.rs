mod ai;
mod backend;
mod chat;
mod stt;
mod tts;
mod ui;

use ui::ChatApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "AI Voice Assistant",
        options,
        Box::new(|_cc| Ok(Box::new(ChatApp::new()))),
    )
}