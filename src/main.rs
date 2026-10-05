#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use resona::gui::ResonaApp;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Resona")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([800.0, 600.0]);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
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

/// アプリアイコン (.ico 内の PNG ストリーム) をデコードして egui::IconData を生成
fn load_app_icon() -> Option<egui::IconData> {
    const ICO_BYTES: &[u8] = include_bytes!("../assets/icons/icon_windows.ico");
    // ICO 内から最大の解像度（256x256）の PNG を探索
    let png_magic = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut last_png_offset = None;
    for i in 0..ICO_BYTES.len().saturating_sub(8) {
        if &ICO_BYTES[i..i + 8] == &png_magic {
            last_png_offset = Some(i);
        }
    }

    let png_data = match last_png_offset {
        Some(offset) => &ICO_BYTES[offset..],
        None => return None,
    };

    if let Ok(img) = image::load_from_memory(png_data) {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        Some(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        })
    } else {
        None
    }
}
