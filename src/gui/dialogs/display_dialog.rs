use egui::{Checkbox, DragValue, Window};

/// 表示設定ダイアログの適用結果
#[derive(Debug, Clone)]
pub struct DisplaySettingsResult {
    pub ppm_min: f64,
    pub ppm_max: f64,
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
    /// ピーク最大高さに対する下限比率 (%) (例: -10.0 で下部に10%のマージン)
    pub y_min_scale: f64,
    /// ピーク最大高さに対する上限比率 (%) (例: 110.0 で上部に10%のマージン)
    pub y_max_scale: f64,
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
            y_min_scale: 10.0,
            y_max_scale: 80.0,
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

    Window::new("Display Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // 1. X 軸表示範囲
            ui.label(egui::RichText::new("X-Axis (range)").strong());
            ui.horizontal(|ui| {
                ui.label("Max ppm");
                ui.add(DragValue::new(&mut state.ppm_max).speed(0.1));
                ui.label("Min ppm");
                ui.add(DragValue::new(&mut state.ppm_min).speed(0.1));
            });

            ui.separator();

            // 2. Y 軸スケール (最大ピークに対する相対パーセンテージ)
            ui.label(egui::RichText::new("Y-Axis (scale)").strong());
            ui.label(
                egui::RichText::new(format!("Peak Top 100% = {:.2e}", state.max_peak_intensity))
                    .size(10.5)
                    .color(egui::Color32::from_gray(120)),
            );
            ui.horizontal(|ui| {
                ui.label("Max");
                ui.add(
                    DragValue::new(&mut state.y_max_scale)
                        .speed(2.0)
                        .range(1.0..=10000.0)
                        .suffix("%"),
                );
                ui.label("Min");
                ui.add(
                    DragValue::new(&mut state.y_min_scale)
                        .speed(1.0)
                        .range(0.0..=10000.0)
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
                    let top_pct = state.y_max_scale.max(1.0);
                    let computed_y_max = base_y * (100.0 / top_pct);
                    let computed_y_min = -base_y * (state.y_min_scale / 100.0);

                    applied = Some(DisplaySettingsResult {
                        ppm_min: state.ppm_min,
                        ppm_max: state.ppm_max,
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
