use egui::{Checkbox, DragValue, Window};

/// 表示設定ダイアログの適用結果
#[derive(Debug, Clone)]
pub struct DisplaySettingsResult {
    pub ppm_min: f64,
    pub ppm_max: f64,
    pub y_scale: f64,
    pub y_offset: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub auto_ticks: bool,
    pub tick_major: f64,
    pub tick_minor: usize,
    pub ppm_decimals: usize,
    pub integral_decimals: usize,
}

pub struct DisplayDialogState {
    pub open: bool,
    pub ppm_min: f64,
    pub ppm_max: f64,
    /// ピーク最大高さに対するスケール比率 (%) (例: 80.0 で上部にマージン)
    pub y_scale: f64,
    /// ベースライン位置のオフセット比率 (%) (例: 10.0 で下部に10%のマージン)
    pub y_offset: f64,
    /// 100% の基準となる最大ピーク強度
    pub max_peak_intensity: f64,
    pub auto_ticks: bool,
    pub tick_major: f64,
    pub tick_minor: usize,
    pub ppm_decimals: usize,
    pub integral_decimals: usize,
}

impl Default for DisplayDialogState {
    fn default() -> Self {
        Self {
            open: false,
            ppm_min: 0.0,
            ppm_max: 10.0,
            y_scale: 80.0,
            y_offset: 10.0,
            max_peak_intensity: 1.0,
            auto_ticks: false,
            tick_major: 1.0,
            tick_minor: 10,
            ppm_decimals: 3,
            integral_decimals: 3,
        }
    }
}

pub fn show_display_dialog(
    ctx: &egui::Context,
    state: &mut DisplayDialogState,
) -> Option<DisplaySettingsResult> {
    let mut applied = None;
    if !state.open {
        return None;
    }

    Window::new(egui::RichText::new("Display Settings").strong().size(13.5))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(320.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            // 1. X 軸表示範囲
            ui.label(egui::RichText::new("X-Axis (range)").strong());
            ui.horizontal(|ui| {
                ui.label("Max ppm");
                ui.add(DragValue::new(&mut state.ppm_max).speed(0.1));
                ui.label("Min ppm");
                ui.add(DragValue::new(&mut state.ppm_min).speed(0.1));
            });

            ui.separator();

            // 2. Y 軸スケール / オフセット (最大ピークに対する相対パーセンテージ)
            ui.label(egui::RichText::new("Y-Axis (scale & offset)").strong());
            ui.label(
                egui::RichText::new(format!("Peak Top 100% = {:.2e}", state.max_peak_intensity))
                    .size(10.5)
                    .color(egui::Color32::from_gray(120)),
            );
            ui.horizontal(|ui| {
                ui.label("Scale");
                ui.add(
                    DragValue::new(&mut state.y_scale)
                        .speed(2.0)
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

            ui.separator();

            // 3. X 軸目盛り (Ticks)
            ui.label(egui::RichText::new("X-Axis Ticks").strong());
            ui.horizontal(|ui| {
                ui.add(Checkbox::new(&mut state.auto_ticks, "Auto Ticks"));
                if !state.auto_ticks {
                    ui.label("Step (ppm)");
                    ui.add(
                        DragValue::new(&mut state.tick_major)
                            .speed(0.1)
                            .range(0.001..=100.0),
                    );
                    ui.label("Subdivisions");
                    ui.add(
                        DragValue::new(&mut state.tick_minor)
                            .range(1..=50),
                    );
                }
            });

            ui.separator();

            // 4. 小数点桁数
            ui.label(egui::RichText::new("Precision").strong());
            ui.horizontal(|ui| {
                ui.label("PPM Decimals");
                ui.add(DragValue::new(&mut state.ppm_decimals).range(1..=6));
                ui.label("Integral Decimals");
                ui.add(DragValue::new(&mut state.integral_decimals).range(0..=4));
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    let base_y = state.max_peak_intensity.max(1e-6);
                    let delta_y = base_y * (100.0 / state.y_scale.max(1.0));
                    let computed_y_min = -delta_y * (state.y_offset / 100.0);
                    let computed_y_max = delta_y * (1.0 - (state.y_offset / 100.0));

                    applied = Some(DisplaySettingsResult {
                        ppm_min: state.ppm_min,
                        ppm_max: state.ppm_max,
                        y_scale: state.y_scale,
                        y_offset: state.y_offset,
                        y_min: computed_y_min,
                        y_max: computed_y_max,
                        auto_ticks: state.auto_ticks,
                        tick_major: state.tick_major,
                        tick_minor: state.tick_minor,
                        ppm_decimals: state.ppm_decimals,
                        integral_decimals: state.integral_decimals,
                    });
                    state.open = false;
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }
            });
        });

    applied
}
