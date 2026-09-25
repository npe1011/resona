use egui::{DragValue, Window};

pub struct DisplayDialogState {
    pub open: bool,
    pub ppm_min: f64,
    pub ppm_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub ppm_decimals: usize,
    pub integral_decimals: usize,
}

impl Default for DisplayDialogState {
    fn default() -> Self {
        Self {
            open: false,
            ppm_min: 0.0,
            ppm_max: 10.0,
            y_min: 0.0,
            y_max: 1.0,
            ppm_decimals: 3,
            integral_decimals: 2,
        }
    }
}

pub fn show_display_dialog(
    ctx: &egui::Context,
    state: &mut DisplayDialogState,
) -> Option<(f64, f64, f64, f64, usize, usize)> {
    let mut applied = None;
    if !state.open {
        return None;
    }

    Window::new("Display Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.heading("Axis & Appearance");
            ui.separator();

            ui.group(|ui| {
                ui.label(egui::RichText::new("X-Axis (ppm)").strong());
                ui.horizontal(|ui| {
                    ui.label("Min ppm");
                    ui.add(DragValue::new(&mut state.ppm_min).speed(0.1));
                    ui.label("Max ppm");
                    ui.add(DragValue::new(&mut state.ppm_max).speed(0.1));
                });
            });

            ui.group(|ui| {
                ui.label(egui::RichText::new("Y-Axis (Intensity)").strong());
                ui.horizontal(|ui| {
                    ui.label("Min Intensity");
                    ui.add(DragValue::new(&mut state.y_min).speed(10.0));
                    ui.label("Max Intensity");
                    ui.add(DragValue::new(&mut state.y_max).speed(10.0));
                });
            });

            ui.group(|ui| {
                ui.label(egui::RichText::new("Precision").strong());
                ui.horizontal(|ui| {
                    ui.label("PPM Decimals");
                    ui.add(DragValue::new(&mut state.ppm_decimals).range(1..=6));
                    ui.label("Integral Decimals");
                    ui.add(DragValue::new(&mut state.integral_decimals).range(0..=4));
                });
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    applied = Some((
                        state.ppm_min,
                        state.ppm_max,
                        state.y_min,
                        state.y_max,
                        state.ppm_decimals,
                        state.integral_decimals,
                    ));
                    state.open = false;
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }
            });
        });

    applied
}
