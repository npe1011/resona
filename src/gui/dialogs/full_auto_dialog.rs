use egui::{vec2, Button, Color32, ComboBox, DragValue, RichText, Stroke, Window};
use serde::{Deserialize, Serialize};

use crate::core::analysis::AutoSensitivity;
use crate::core::baseline::BaselineMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FullAutoBaselineChoice {
    #[default]
    None,
    AirPLS,
    Polynomial,
}

#[derive(Debug, Clone)]
pub struct FullAutoDialogState {
    pub open: bool,
    pub sensitivity: AutoSensitivity,
    pub baseline_choice: FullAutoBaselineChoice,
    pub airpls_log_lambda: f64,
    pub poly_order: usize,
    pub enable_integration: bool,
}

impl Default for FullAutoDialogState {
    fn default() -> Self {
        Self {
            open: false,
            sensitivity: AutoSensitivity::Middle,
            baseline_choice: FullAutoBaselineChoice::None,
            airpls_log_lambda: 8.0,
            poly_order: 3,
            enable_integration: false,
        }
    }
}

/// Full Auto ダイアログの実行結果
#[derive(Debug, Clone)]
pub struct FullAutoResult {
    pub sensitivity: AutoSensitivity,
    pub baseline_method: BaselineMethod,
    pub enable_integration: bool,
}

/// Full Auto Process ダイアログを表示する (最低限のUI・ラベルとRun/Cancelボタンのみ)
pub fn show_full_auto_dialog(
    ctx: &egui::Context,
    state: &mut FullAutoDialogState,
) -> Option<FullAutoResult> {
    if !state.open {
        return None;
    }

    let mut result = None;

    Window::new("Full Auto Process")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(320.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;

            // 1. Sensitivity 設定 (全体設定と連動)
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sensitivity:").strong().size(12.5));
                ComboBox::from_id_salt("full_auto_sensitivity_combo")
                    .width(100.0)
                    .selected_text(RichText::new(state.sensitivity.label()).size(12.0))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.sensitivity, AutoSensitivity::Low, "Low");
                        ui.selectable_value(&mut state.sensitivity, AutoSensitivity::Middle, "Middle");
                        ui.selectable_value(&mut state.sensitivity, AutoSensitivity::High, "High");
                    });
            });

            // 2. Baseline Correction 設定
            ui.horizontal(|ui| {
                ui.label(RichText::new("Baseline Correction:").strong().size(12.5));
                ui.radio_value(&mut state.baseline_choice, FullAutoBaselineChoice::None, "None");
                ui.radio_value(&mut state.baseline_choice, FullAutoBaselineChoice::AirPLS, "airPLS");
                ui.radio_value(&mut state.baseline_choice, FullAutoBaselineChoice::Polynomial, "Polynomial");
            });

            match state.baseline_choice {
                FullAutoBaselineChoice::None => {}
                FullAutoBaselineChoice::AirPLS => {
                    ui.horizontal(|ui| {
                        ui.label("Stiffness (log10 λ):");
                        ui.add(
                            DragValue::new(&mut state.airpls_log_lambda)
                                .speed(0.1)
                                .range(3.0..=12.0),
                        );
                    });
                }
                FullAutoBaselineChoice::Polynomial => {
                    ui.horizontal(|ui| {
                        ui.label("Polynomial Order:");
                        ui.add(
                            DragValue::new(&mut state.poly_order)
                                .speed(1)
                                .range(1..=6),
                        );
                    });
                }
            }

            ui.separator();

            // 3. Integration 設定
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.enable_integration, RichText::new("Integration").strong().size(12.5));
            });

            ui.separator();

            // 4. ボタン行 (Cancel & Run)
            ui.horizontal(|ui| {
                // Cancel ボタン
                let btn_cancel = Button::new(RichText::new("Cancel").size(12.5).color(Color32::from_rgb(73, 80, 87)))
                    .min_size(vec2(70.0, 26.0))
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)))
                    .rounding(3.0);

                if ui.add(btn_cancel).clicked() {
                    state.open = false;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Run ボタン (パープル強調)
                    let btn_run = Button::new(RichText::new("Run").strong().size(12.5).color(Color32::WHITE))
                        .min_size(vec2(70.0, 26.0))
                        .fill(Color32::from_rgb(126, 34, 206)) // purple-700
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(107, 33, 168)))
                        .rounding(3.0);

                    if ui.add(btn_run).clicked() {
                        let baseline_method = match state.baseline_choice {
                            FullAutoBaselineChoice::None => BaselineMethod::None,
                            FullAutoBaselineChoice::AirPLS => BaselineMethod::AirPLS {
                                log_lambda: state.airpls_log_lambda,
                                max_iter: 15,
                            },
                            FullAutoBaselineChoice::Polynomial => BaselineMethod::Polynomial {
                                order: state.poly_order,
                                max_iter: 10,
                            },
                        };

                        result = Some(FullAutoResult {
                            sensitivity: state.sensitivity,
                            baseline_method,
                            enable_integration: state.enable_integration,
                        });
                        state.open = false;
                    }
                });
            });
        });

    result
}
