use std::fs;
use std::path::{Path, PathBuf};
use egui::{vec2, Align2, Button, Color32, DragValue, RichText, Window};
use serde::{Deserialize, Serialize};

use crate::gui::config::get_settings_dir;

/// 印刷用のRGBカラー表現
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn to_color32(&self) -> Color32 {
        Color32::from_rgb(self.r, self.g, self.b)
    }

    pub fn to_color32_alpha(&self, a: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(self.r, self.g, self.b, a)
    }

    pub fn from_color32(c: Color32) -> Self {
        Self {
            r: c.r(),
            g: c.g(),
            b: c.b(),
        }
    }
}

/// メインスペクトルの印刷スタイル設定
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintSpectrumStyle {
    pub line_width: f32,
    pub line_color: RgbColor,
    pub integral_width: f32,
    pub integral_color: RgbColor,
    pub peak_lead_width: f32,
    pub peak_lead_color: RgbColor,
}

impl Default for PrintSpectrumStyle {
    fn default() -> Self {
        Self {
            line_width: 0.85,
            line_color: RgbColor::new(0, 0, 0),
            integral_width: 1.3,
            integral_color: RgbColor::new(225, 29, 72),
            peak_lead_width: 0.55,
            peak_lead_color: RgbColor::new(112, 117, 122),
        }
    }
}

/// Multiviewインセットの印刷スタイル設定
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintMultiviewStyle {
    pub line_width: f32,
    pub line_color: RgbColor,
    pub integral_width: f32,
    pub integral_color: RgbColor,
    pub peak_lead_width: f32,
    pub peak_lead_color: RgbColor,
}

impl Default for PrintMultiviewStyle {
    fn default() -> Self {
        Self {
            line_width: 0.85,
            line_color: RgbColor::new(0, 0, 0),
            integral_width: 1.3,
            integral_color: RgbColor::new(217, 27, 66),
            peak_lead_width: 0.50,
            peak_lead_color: RgbColor::new(112, 117, 122),
        }
    }
}

/// 印刷スタイル全般の設定 (~/.resona/print_settings.json)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PrintStyleSettings {
    pub main_spectrum: PrintSpectrumStyle,
    pub multiview: PrintMultiviewStyle,
}

impl PrintStyleSettings {
    pub fn config_path() -> Option<PathBuf> {
        get_settings_dir().map(|d| d.join("print_settings.json"))
    }

    /// 設定ファイルから読み込む (存在しないかパースに失敗した場合はデフォルト)
    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(settings) = serde_json::from_str::<Self>(&content) {
                        return settings;
                    }
                }
            }
        }
        Self::default()
    }

    /// 設定ファイルに保存する
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_string_pretty(self).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?;
            fs::write(path, json)?;
        }
        Ok(())
    }

    /// 外部ファイルにエクスポート
    pub fn export_to<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
        fs::write(path, json)
    }

    /// 外部ファイルからインポート
    pub fn import_from<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Self>(&content).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct PrintStyleDialogState {
    pub is_open: bool,
    pub status_message: Option<(String, bool)>,
}

fn color_picker(ui: &mut egui::Ui, color: &mut RgbColor) {
    let mut rgb = [color.r, color.g, color.b];
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        color.r = rgb[0];
        color.g = rgb[1];
        color.b = rgb[2];
    }
}

/// 印刷スタイル設定ダイアログの表示
pub fn show_print_style_dialog(
    ctx: &egui::Context,
    state: &mut PrintStyleDialogState,
    settings: &mut PrintStyleSettings,
) {
    if !state.is_open {
        return;
    }

    let mut is_open = state.is_open;
    let mut should_close = false;

    Window::new(RichText::new("Print Settings").strong().size(13.5))
        .collapsible(false)
        .resizable(false)
        .open(&mut is_open)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(400.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            // 1. Main Spectrum グループ
            ui.group(|ui| {
                ui.label(RichText::new("Main Spectrum").strong().size(12.5));
                ui.add_space(2.0);

                egui::Grid::new("main_spectrum_style_grid")
                    .num_columns(3)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Spectrum line:");
                        ui.add(
                            DragValue::new(&mut settings.main_spectrum.line_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        color_picker(ui, &mut settings.main_spectrum.line_color);
                        ui.end_row();

                        ui.label("Integral curve:");
                        ui.add(
                            DragValue::new(&mut settings.main_spectrum.integral_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        color_picker(ui, &mut settings.main_spectrum.integral_color);
                        ui.end_row();

                        ui.label("Peak leader line:");
                        ui.add(
                            DragValue::new(&mut settings.main_spectrum.peak_lead_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        color_picker(ui, &mut settings.main_spectrum.peak_lead_color);
                        ui.end_row();
                    });
            });

            // 2. Multiview グループ
            ui.group(|ui| {
                ui.label(RichText::new("Multiview Inset").strong().size(12.5));
                ui.add_space(2.0);

                egui::Grid::new("multiview_style_grid")
                    .num_columns(3)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Spectrum line:");
                        ui.add(
                            DragValue::new(&mut settings.multiview.line_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        color_picker(ui, &mut settings.multiview.line_color);
                        ui.end_row();

                        ui.label("Integral curve:");
                        ui.add(
                            DragValue::new(&mut settings.multiview.integral_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        color_picker(ui, &mut settings.multiview.integral_color);
                        ui.end_row();

                        ui.label("Peak leader line:");
                        ui.add(
                            DragValue::new(&mut settings.multiview.peak_lead_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        color_picker(ui, &mut settings.multiview.peak_lead_color);
                        ui.end_row();
                    });
            });

            // ステータスメッセージ表示
            if let Some((ref msg, is_err)) = state.status_message {
                let color = if is_err {
                    Color32::from_rgb(220, 38, 38)
                } else {
                    Color32::from_rgb(25, 135, 84)
                };
                ui.label(RichText::new(msg).size(11.0).color(color));
            }

            ui.separator();

            // 3. アクションボタン: Default, Export, Import, Close
            ui.horizontal(|ui| {
                if ui.button("Default").clicked() {
                    *settings = PrintStyleSettings::default();
                    let _ = settings.save();
                    state.status_message = Some(("Reset to default style".to_string(), false));
                }

                if ui.button("Export...").clicked() {
                    if let Some(target) = rfd::FileDialog::new()
                        .set_title("Export Print Settings")
                        .add_filter("JSON", &["json"])
                        .set_file_name("resona_print_settings.json")
                        .save_file()
                    {
                        match settings.export_to(&target) {
                            Ok(()) => {
                                state.status_message = Some(("Exported print settings".to_string(), false));
                            }
                            Err(e) => {
                                state.status_message = Some((format!("Export failed: {}", e), true));
                            }
                        }
                    }
                }

                if ui.button("Import...").clicked() {
                    if let Some(src) = rfd::FileDialog::new()
                        .set_title("Import Print Settings")
                        .add_filter("JSON", &["json"])
                        .pick_file()
                    {
                        match PrintStyleSettings::import_from(&src) {
                            Ok(imported) => {
                                *settings = imported;
                                let _ = settings.save();
                                state.status_message = Some(("Imported print settings".to_string(), false));
                            }
                            Err(e) => {
                                state.status_message = Some((format!("Import failed: {}", e), true));
                            }
                        }
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_close = Button::new(RichText::new("Close").strong().size(12.0).color(Color32::WHITE))
                        .min_size(vec2(70.0, 24.0))
                        .fill(Color32::from_rgb(13, 110, 253))
                        .rounding(3.0_f32);
                    if ui.add(btn_close).clicked() {
                        should_close = true;
                    }
                });
            });
        });

    if should_close {
        is_open = false;
    }

    if !is_open {
        let _ = settings.save();
        state.is_open = false;
        state.status_message = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_color_conversions() {
        let col = RgbColor::new(12, 34, 56);
        assert_eq!(col.to_color32(), Color32::from_rgb(12, 34, 56));
        assert_eq!(RgbColor::from_color32(Color32::from_rgb(12, 34, 56)), col);
        assert_eq!(col.to_hex(), "#0c2238");

        let black = RgbColor::new(0, 0, 0);
        assert_eq!(black.to_hex(), "#000000");

        let white = RgbColor::new(255, 255, 255);
        assert_eq!(white.to_hex(), "#ffffff");
    }

    #[test]
    fn test_print_style_settings_default() {
        let def = PrintStyleSettings::default();
        assert!((def.main_spectrum.line_width - 0.85).abs() < 1e-5);
        assert_eq!(def.main_spectrum.line_color, RgbColor::new(0, 0, 0));
        assert!((def.main_spectrum.integral_width - 1.3).abs() < 1e-5);
        assert!((def.main_spectrum.peak_lead_width - 0.55).abs() < 1e-5);

        assert!((def.multiview.line_width - 0.85).abs() < 1e-5);
        assert_eq!(def.multiview.line_color, RgbColor::new(0, 0, 0));
        assert!((def.multiview.integral_width - 1.3).abs() < 1e-5);
        assert!((def.multiview.peak_lead_width - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_print_style_settings_serialization_roundtrip() {
        let mut settings = PrintStyleSettings::default();
        settings.main_spectrum.line_width = 2.5;
        settings.main_spectrum.line_color = RgbColor::new(10, 20, 30);
        settings.multiview.integral_width = 1.8;
        settings.multiview.integral_color = RgbColor::new(200, 100, 50);

        let json = serde_json::to_string_pretty(&settings).expect("serialization failed");
        let deserialized: PrintStyleSettings = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(settings, deserialized);
    }
}


