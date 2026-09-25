use egui::{DragValue, Window};
use crate::core::{FtSettings, WindowFunction};

pub struct FtDialogState {
    pub open: bool,
    pub settings: FtSettings,
}

impl Default for FtDialogState {
    fn default() -> Self {
        Self {
            open: false,
            settings: FtSettings::default(),
        }
    }
}

pub fn show_ft_dialog(ctx: &egui::Context, state: &mut FtDialogState) -> Option<FtSettings> {
    let mut applied_settings = None;
    if !state.open {
        return None;
    }

    Window::new("Fourier Transform Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.heading("Processing Parameters");
            ui.separator();

            // 1. 窓関数
            ui.group(|ui| {
                ui.label(egui::RichText::new("Window Function").strong());
                let is_none = matches!(state.settings.window, WindowFunction::None);
                let is_em = matches!(state.settings.window, WindowFunction::Exponential { .. });
                let is_gm = matches!(state.settings.window, WindowFunction::Gaussian { .. });

                ui.horizontal(|ui| {
                    if ui.selectable_label(is_none, "None").clicked() {
                        state.settings.window = WindowFunction::None;
                    }
                    if ui.selectable_label(is_em, "Exponential (EM)").clicked() {
                        state.settings.window = WindowFunction::Exponential { lb: 0.3 };
                    }
                    if ui.selectable_label(is_gm, "Gaussian (GM)").clicked() {
                        state.settings.window = WindowFunction::Gaussian { g1: 0.0, g2: 0.0, g3: 0.0 };
                    }
                });

                match &mut state.settings.window {
                    WindowFunction::Exponential { lb } => {
                        ui.horizontal(|ui| {
                            ui.label("Line Broadening (Hz):");
                            ui.add(DragValue::new(lb).speed(0.05).range(0.01..=50.0));
                        });
                    }
                    WindowFunction::Gaussian { g1, g2, g3 } => {
                        ui.horizontal(|ui| {
                            ui.label("GM g1 (Hz):");
                            ui.add(DragValue::new(g1).speed(0.1));
                            ui.label("g2 (Hz):");
                            ui.add(DragValue::new(g2).speed(0.1));
                            ui.label("g3:");
                            ui.add(DragValue::new(g3).speed(0.01));
                        });
                    }
                    WindowFunction::None => {}
                }
            });

            // 2. ゼロフィリング
            ui.group(|ui| {
                ui.label(egui::RichText::new("Zero Filling").strong());
                ui.horizontal(|ui| {
                    let factors = [1, 2, 4, 8, 16];
                    for f in factors {
                        let label = if f == 1 { "None (1x)".to_string() } else { format!("{}x", f) };
                        if ui.selectable_label(state.settings.zf_factor == f, label).clicked() {
                            state.settings.zf_factor = f;
                        }
                    }
                });
            });

            // 3. その他オプション
            ui.group(|ui| {
                ui.checkbox(&mut state.settings.remove_digital_filter, "Remove Digital Filter (Fractional Shift)");
                ui.checkbox(&mut state.settings.auto_phase, "Run ACME Autophase after FT");
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Apply FT").clicked() {
                    applied_settings = Some(state.settings.clone());
                    state.open = false;
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }
            });
        });

    applied_settings
}
