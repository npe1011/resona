use egui::{vec2, Button, Color32, DragValue, RichText, Window};

#[derive(Debug, Clone)]
pub struct MultiviewYScaleDialogState {
    pub open: bool,
    pub target_id: Option<String>,
    pub target_label: String,
    pub auto_y: bool,
    /// 100% の基準となるインセット内最大ピーク強度
    pub max_peak_intensity: f64,
    /// ピーク最大高さに対する上限比率 (%)
    pub y_max_scale: f64,
    /// ピーク最大高さに対する下限比率 (%)
    pub y_min_scale: f64,
}

impl Default for MultiviewYScaleDialogState {
    fn default() -> Self {
        Self {
            open: false,
            target_id: None,
            target_label: String::new(),
            auto_y: true,
            max_peak_intensity: 1.0,
            y_max_scale: 80.0,
            y_min_scale: 10.0,
        }
    }
}

pub struct MultiviewYScaleResult {
    pub target_id: String,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
}

pub fn show_multiview_yscale_dialog(
    ctx: &egui::Context,
    state: &mut MultiviewYScaleDialogState,
) -> Option<MultiviewYScaleResult> {
    if !state.open {
        return None;
    }

    let mut result = None;

    Window::new("Multiview Inset Y-Scale")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(300.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            if !state.target_label.is_empty() {
                ui.label(RichText::new(&state.target_label).strong().size(12.5));
            }

            ui.checkbox(&mut state.auto_y, "Auto Y-Scale (Fit to Inset Peak)");

            ui.add_enabled_ui(!state.auto_y, |ui| {
                ui.label(
                    RichText::new(format!("Peak Top 100% = {:.2e}", state.max_peak_intensity))
                        .size(10.5)
                        .color(Color32::from_gray(120)),
                );
                ui.horizontal(|ui| {
                    ui.label("Max:");
                    ui.add(
                        DragValue::new(&mut state.y_max_scale)
                            .speed(2.0)
                            .range(1.0..=10000.0)
                            .suffix("%"),
                    );
                    ui.label("Min:");
                    ui.add(
                        DragValue::new(&mut state.y_min_scale)
                            .speed(1.0)
                            .range(0.0..=10000.0)
                            .suffix("%"),
                    );
                });
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_ok = Button::new(RichText::new("Apply").strong().size(12.5).color(Color32::WHITE))
                        .min_size(vec2(70.0, 24.0))
                        .fill(Color32::from_rgb(13, 110, 253))
                        .rounding(3.0);

                    if ui.add(btn_ok).clicked() {
                        if let Some(ref id) = state.target_id {
                            let (y_min, y_max) = if state.auto_y {
                                (None, None)
                            } else {
                                let base_y = state.max_peak_intensity.max(1e-6);
                                let top_pct = state.y_max_scale.max(1.0);
                                let computed_y_max = base_y * (100.0 / top_pct);
                                let computed_y_min = -base_y * (state.y_min_scale / 100.0);
                                (Some(computed_y_min), Some(computed_y_max))
                            };

                            result = Some(MultiviewYScaleResult {
                                target_id: id.clone(),
                                y_min,
                                y_max,
                            });
                        }
                        state.open = false;
                    }
                });
            });
        });

    result
}
