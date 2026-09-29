#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use resona::gui::ResonaApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Resona")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    let args: Vec<String> = std::env::args().collect();
    let initial_file = if args.len() > 1 && !args[1].starts_with('-') {
        Some(std::path::PathBuf::from(&args[1]))
    } else {
        None
    };

    eframe::run_native(
        "Resona",
        native_options,
        Box::new(move |cc| {
            let mut app = ResonaApp::new(cc);
            if let Some(ref path) = initial_file {
                if path.exists() {
                    app.open_file(path);
                }
            }
            Ok(Box::new(app))
        }),
    )
}
