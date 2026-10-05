use egui::{Align2, Checkbox, DragValue, RichText, Window};

use crate::multispec::settings::MultiSpecSettings;
use crate::multispec::state::MultiSpecState;

/// MultiSpec 表示設定ダイアログの状態管理
pub struct MultiSpecDisplayDialogState {
    pub is_open: bool,
    pub ppm_min: f64,
    pub ppm_max: f64,
    pub auto_ticks: bool,
    pub tick_major: f64,
    pub tick_minor: usize,
    pub integral_decimals: usize,
}

impl Default for MultiSpecDisplayDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            ppm_min: -1.0,
            ppm_max: 10.0,
            auto_ticks: false,
            tick_major: 1.0,
            tick_minor: 10,
            integral_decimals: 3,
        }
    }
}

impl MultiSpecDisplayDialogState {
    pub fn open_from(&mut self, state: &MultiSpecState, settings: &MultiSpecSettings) {
        self.is_open = true;
        self.ppm_min = state.common_ppm_min;
        self.ppm_max = state.common_ppm_max;
        self.auto_ticks = settings.auto_ticks;
        self.tick_major = settings.tick_major;
        self.tick_minor = settings.tick_minor;
        self.integral_decimals = settings.integral_decimals;
    }
}

/// MultiSpec 表示設定ダイアログの描画 (モーダル表示)
pub fn show_multispec_display_dialog(
    ctx: &egui::Context,
    dialog_state: &mut MultiSpecDisplayDialogState,
    state: &mut MultiSpecState,
    settings: &mut MultiSpecSettings,
) {
    if !dialog_state.is_open {
        return;
    }

    let mut is_open = dialog_state.is_open;
    let mut applied = false;
    let mut should_close = false;

    Window::new(RichText::new("Display Settings").strong().size(13.5))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(320.0)
        .open(&mut is_open)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            // 1. X 軸表示範囲 (ppm)
            ui.label(RichText::new("X-Axis (range)").strong());
            ui.horizontal(|ui| {
                ui.label("Max ppm");
                ui.add(DragValue::new(&mut dialog_state.ppm_max).speed(0.1));
                ui.label("Min ppm");
                ui.add(DragValue::new(&mut dialog_state.ppm_min).speed(0.1));
            });

            ui.separator();

            // 2. X 軸目盛り (Ticks)
            ui.label(RichText::new("X-Axis Ticks").strong());
            ui.horizontal(|ui| {
                ui.add(Checkbox::new(&mut dialog_state.auto_ticks, "Auto Ticks"));
                if !dialog_state.auto_ticks {
                    ui.label("Step (ppm)");
                    ui.add(
                        DragValue::new(&mut dialog_state.tick_major)
                            .speed(0.1)
                            .range(0.001..=100.0),
                    );
                    ui.label("Subdivisions");
                    ui.add(
                        DragValue::new(&mut dialog_state.tick_minor)
                            .range(1..=50),
                    );
                }
            });

            ui.separator();

            // 3. 精度 (Integral Decimals のみ)
            ui.label(RichText::new("Precision").strong());
            ui.horizontal(|ui| {
                ui.label("Integral Decimals");
                ui.add(DragValue::new(&mut dialog_state.integral_decimals).range(0..=4));
            });

            ui.separator();

            // 4. アクションボタン (Apply, Cancel)
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    applied = true;
                    should_close = true;
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });
        });

    if should_close {
        is_open = false;
    }

    if applied {
        state.common_ppm_min = dialog_state.ppm_min;
        state.common_ppm_max = dialog_state.ppm_max;
        settings.auto_ticks = dialog_state.auto_ticks;
        settings.tick_major = dialog_state.tick_major;
        settings.tick_minor = dialog_state.tick_minor;
        settings.integral_decimals = dialog_state.integral_decimals;
        state.push_history();
    }

    dialog_state.is_open = is_open;
}
