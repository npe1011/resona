use egui::{Align2, Color32, DragValue, RichText, Window};

use crate::multispec::state::MultiSpecState;

/// 全スペクトル一括 Y-Scale 設定ダイアログの状態管理
pub struct MultiSpecYScaleDialogState {
    pub is_open: bool,
    pub y_scale_max: f64,
    pub y_scale_min: f64,
}

impl Default for MultiSpecYScaleDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            y_scale_max: 100.0,
            y_scale_min: 0.0,
        }
    }
}

impl MultiSpecYScaleDialogState {
    pub fn open_from(&mut self, state: &MultiSpecState) {
        self.is_open = true;
        if let Some(first) = state.items.first() {
            self.y_scale_max = first.y_scale_max;
            self.y_scale_min = first.y_scale_min;
        } else {
            self.y_scale_max = 100.0;
            self.y_scale_min = 0.0;
        }
    }
}

/// 全スペクトル一括 Y-Scale 設定ダイアログの描画 (モーダル表示)
pub fn show_multispec_y_scale_dialog(
    ctx: &egui::Context,
    dialog_state: &mut MultiSpecYScaleDialogState,
    state: &mut MultiSpecState,
) {
    if !dialog_state.is_open {
        return;
    }

    let mut is_open = dialog_state.is_open;
    let mut applied = false;
    let mut should_close = false;

    Window::new(RichText::new("Uniform Y-Scale").strong().size(13.5))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(280.0)
        .open(&mut is_open)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            ui.label(
                RichText::new("Apply uniform Y-scale to all spectra")
                    .size(11.5)
                    .color(Color32::from_gray(100)),
            );
            ui.add_space(2.0);

            ui.horizontal(|ui| {
                ui.label("Y-Scale Max");
                ui.add(
                    DragValue::new(&mut dialog_state.y_scale_max)
                        .speed(1.0)
                        .range(1.0..=10000.0)
                        .suffix("%"),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Y-Scale Min");
                ui.add(
                    DragValue::new(&mut dialog_state.y_scale_min)
                        .speed(1.0)
                        .range(0.0..=10000.0)
                        .suffix("%"),
                );
            });

            ui.separator();

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
        for item in &mut state.items {
            item.y_scale_max = dialog_state.y_scale_max;
            item.y_scale_min = dialog_state.y_scale_min;
        }
        state.push_history();
    }

    dialog_state.is_open = is_open;
}
