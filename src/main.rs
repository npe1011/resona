use eframe::egui;
use resona::gui::ResonaApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Resona - NMR Processing & Analysis")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Resona",
        native_options,
        Box::new(|cc| Ok(Box::new(ResonaApp::new(cc)))),
    )
}
