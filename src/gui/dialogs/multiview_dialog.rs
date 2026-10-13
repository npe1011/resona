use egui::{vec2, Button, Color32, DragValue, RichText, Window};
use ndarray::Array1;

/// インセットの PPM 範囲とスペクトルから、Auto Y-Scale 時の (scale, offset, max_intensity) を計算する
pub fn compute_inset_auto_y(
    ppm: Option<&Array1<f64>>,
    spectrum: Option<&Array1<f64>>,
    src_x_min: f64,
    src_x_max: f64,
    global_max: f64,
) -> (f64, f64, f64) {
    let p_min = src_x_min.min(src_x_max);
    let p_max = src_x_min.max(src_x_max);
    let mut local_y_min = f64::INFINITY;
    let mut local_y_max = f64::NEG_INFINITY;

    if let (Some(ppm), Some(spec)) = (ppm, spectrum) {
        for i in 0..ppm.len().min(spec.len()) {
            let p = ppm[i];
            if p >= p_min && p <= p_max {
                let v = spec[i];
                if v < local_y_min {
                    local_y_min = v;
                }
                if v > local_y_max {
                    local_y_max = v;
                }
            }
        }
    }

    if local_y_min >= local_y_max || local_y_max.is_infinite() {
        local_y_min = 0.0;
        local_y_max = if global_max > 1e-6 { global_max } else { 1.0 };
    }

    let inset_max = local_y_max.max(1e-6);
    let y_min = local_y_min.min(0.0);
    let y_max = local_y_max;
    let h = (y_max - y_min).max(1e-6);
    let y_min_adj = y_min - 0.02 * h;
    let y_max_adj = y_max + 0.40 * h;

    let delta_y = (y_max_adj - y_min_adj).max(1e-6);
    let auto_scale = (inset_max / delta_y * 100.0).clamp(1.0, 10000.0);
    let auto_offset = (-y_min_adj / delta_y * 100.0).clamp(-1000.0, 10000.0);

    (auto_scale, auto_offset, inset_max)
}

#[derive(Debug, Clone)]
pub struct MultiviewYScaleDialogState {
    pub open: bool,
    pub target_id: Option<String>,
    pub auto_y: bool,
    pub src_x_min: f64,
    pub src_x_max: f64,
    /// 100% の基準となるインセット内最大ピーク強度
    pub max_peak_intensity: f64,
    /// ピーク最大高さに対するスケール比率 (%)
    pub y_scale: f64,
    /// ベースライン位置のオフセット比率 (%)
    pub y_offset: f64,
}

impl Default for MultiviewYScaleDialogState {
    fn default() -> Self {
        Self {
            open: false,
            target_id: None,
            auto_y: true,
            src_x_min: 0.0,
            src_x_max: 10.0,
            max_peak_intensity: 1.0,
            y_scale: 80.0,
            y_offset: 10.0,
        }
    }
}

pub struct MultiviewYScaleResult {
    pub target_id: String,
    pub src_x_min: f64,
    pub src_x_max: f64,
    pub y_scale: Option<f64>,
    pub y_offset: Option<f64>,
}

pub fn show_multiview_yscale_dialog(
    ctx: &egui::Context,
    state: &mut MultiviewYScaleDialogState,
    ppm: Option<&Array1<f64>>,
    spectrum: Option<&Array1<f64>>,
    global_max: f64,
) -> Option<MultiviewYScaleResult> {
    if !state.open {
        return None;
    }

    let mut result = None;

    Window::new("Multiview Inset Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(320.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            // X軸 (Inset) 範囲設定
            let mut ppm_changed = false;
            ui.horizontal(|ui| {
                ui.label("PPM Range");
                let r_max = ui.add(
                    DragValue::new(&mut state.src_x_max)
                        .speed(0.005)
                        .suffix(" ppm"),
                );
                ui.label("~");
                let r_min = ui.add(
                    DragValue::new(&mut state.src_x_min)
                        .speed(0.005)
                        .suffix(" ppm"),
                );
                if r_max.changed() || r_min.changed() {
                    ppm_changed = true;
                }
            });

            // Auto Y-Scale チェックボックス
            let chk = ui.checkbox(&mut state.auto_y, "Auto Y-Scale");
            if (chk.changed() && state.auto_y) || (ppm_changed && state.auto_y) {
                let (auto_s, auto_o, max_p) = compute_inset_auto_y(
                    ppm,
                    spectrum,
                    state.src_x_min,
                    state.src_x_max,
                    global_max,
                );
                state.y_scale = auto_s;
                state.y_offset = auto_o;
                state.max_peak_intensity = max_p;
            } else if ppm_changed && !state.auto_y {
                let (_, _, max_p) = compute_inset_auto_y(
                    ppm,
                    spectrum,
                    state.src_x_min,
                    state.src_x_max,
                    global_max,
                );
                state.max_peak_intensity = max_p;
            }

            ui.add_enabled_ui(!state.auto_y, |ui| {
                ui.label(
                    RichText::new(format!("Peak Top 100% = {:.2e}", state.max_peak_intensity))
                        .size(10.5)
                        .color(Color32::from_gray(120)),
                );
                ui.horizontal(|ui| {
                    ui.label("Scale");
                    ui.add(
                        DragValue::new(&mut state.y_scale)
                            .speed(1.0)
                            .range(1.0..=10000.0)
                            .suffix("%"),
                    );
                    ui.label("Offset");
                    ui.add(
                        DragValue::new(&mut state.y_offset)
                            .speed(1.0)
                            .range(-1000.0..=10000.0)
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
                            let (y_scale, y_offset) = if state.auto_y {
                                (None, None)
                            } else {
                                (Some(state.y_scale), Some(state.y_offset))
                            };

                            let p_low = state.src_x_min.min(state.src_x_max);
                            let p_high = state.src_x_min.max(state.src_x_max);

                            result = Some(MultiviewYScaleResult {
                                target_id: id.clone(),
                                src_x_min: p_low,
                                src_x_max: p_high,
                                y_scale,
                                y_offset,
                            });
                        }
                        state.open = false;
                    }
                });
            });
        });

    result
}
