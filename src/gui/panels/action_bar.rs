use egui::{
    vec2, Button, Color32, DragValue, Frame, Margin, RichText, Slider, Stroke, Ui,
};

use crate::gui::mode::{AppMode, IntegrateSubMode, MultiviewSubMode, PeakSubMode, ZoomTool};

#[derive(Debug, Clone, PartialEq)]
pub enum ActionEvent {
    None,
    ResetZoom,
    UndoZoom,
    AutoPhase,
    ResetPhase,
    ApplyBaseline { log_lambda: f64, p: f64 },
    ClearBaseline,
    ApplyShiftReference { peak_ppm: f64, target_ppm: f64 },
    AutoPeak,
    PickPeaks { threshold: f64 },
    ClearPeaks,
    AutoIntegrate,
    ClearIntegrations,
    AutoMultiview,
    AlignMultiview,
    ResetMultiview,
    ClearJCoupling,
    CloseMode,
}

/// 専用アクションバーの表示コンポーネント
pub struct ActionBarState {
    pub peak_submode: PeakSubMode,
    pub integrate_submode: IntegrateSubMode,
    pub multiview_submode: MultiviewSubMode,
    pub jcoupling_add_active: bool,
    pub non_integer_protons: bool,

    // パラメータ
    pub baseline_log_lambda: f64,
    pub baseline_p: f64,
    pub baseline_applied: bool,

    // Reference モード
    pub ref_solvent_idx: usize,
    pub ref_target_ppm: f64,
    pub ref_set_active: bool,

    // Peak モード
    pub peak_threshold: f64,

    // Integrate モード
    pub integration_ref_val: f64,

    // Multiview モード
    pub multiview_ratio: f64,
}

impl ActionBarState {
    /// ズームツール起動時や Esc 押下時にすべてのサブモードを解除する
    pub fn clear_submodes(&mut self) {
        self.peak_submode = PeakSubMode::None;
        self.integrate_submode = IntegrateSubMode::None;
        self.multiview_submode = MultiviewSubMode::None;
        self.jcoupling_add_active = false;
        self.ref_set_active = false;
    }
}

impl Default for ActionBarState {
    fn default() -> Self {
        Self {
            peak_submode: PeakSubMode::None,
            integrate_submode: IntegrateSubMode::None,
            multiview_submode: MultiviewSubMode::None,
            jcoupling_add_active: false,
            non_integer_protons: false,
            baseline_log_lambda: 8.0,
            baseline_p: 0.005,
            baseline_applied: false,
            ref_solvent_idx: 0,
            ref_target_ppm: 7.26,
            ref_set_active: false,
            peak_threshold: 0.0,
            integration_ref_val: 1.0,
            multiview_ratio: 5.0,
        }
    }
}

/// ezNMRライトテーマ準拠のボタン用スタイル
fn light_button(
    ui: &mut Ui,
    text: &str,
    is_active: bool,
    min_width: f32,
) -> egui::Response {
    let (text_color, fill_color, stroke) = if is_active {
        // アクティブ: 淡いブルー背景 (#e7f1ff), 濃いブルー文字 (#084298), ブルー枠線 (#86b7fe)
        (
            Color32::from_rgb(8, 66, 152),
            Color32::from_rgb(231, 241, 255),
            Stroke::new(1.0_f32, Color32::from_rgb(134, 183, 254)),
        )
    } else {
        // 通常: 白背景 (#ffffff), 濃いグレー文字 (#495057), 薄いグレー枠線 (#ced4da)
        (
            Color32::from_rgb(73, 80, 87),
            Color32::WHITE,
            Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)),
        )
    };

    let rich = if is_active {
        RichText::new(text).strong().size(12.0).color(text_color)
    } else {
        RichText::new(text).size(12.0).color(text_color)
    };

    let btn = Button::new(rich)
        .min_size(vec2(min_width, 22.0))
        .fill(fill_color)
        .stroke(stroke)
        .rounding(3.0_f32);

    ui.add(btn)
}

/// モード終了用の Close ボタン (淡い赤背景・赤文字で識別しやすいデザイン)
fn close_button(ui: &mut Ui) -> egui::Response {
    let rich = RichText::new("Close").size(12.0).strong().color(Color32::from_rgb(185, 28, 28));
    let btn = Button::new(rich)
        .min_size(vec2(46.0, 22.0))
        .fill(Color32::from_rgb(254, 242, 242)) // #fef2f2
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(252, 165, 165))) // #fca5a5
        .rounding(3.0_f32);

    ui.add(btn)
}

/// ezNMR 2段目ツールバーの描画 (左: 常駐ZOOMフレーム, 右: コンテキストフレーム)
pub fn show_action_bar(
    ui: &mut Ui,
    active_mode: Option<AppMode>,
    active_zoom: &mut Option<ZoomTool>,
    state: &mut ActionBarState,
    p0: &mut f64,
    p1: &mut f64,
    integration_scale: &mut f64,
    nucleus: &str,
    noise_level: f64,
) -> ActionEvent {
    let mut event = ActionEvent::None;

    let frame_style = Frame::none()
        .fill(Color32::from_rgb(248, 249, 250)) // #f8f9fa
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))) // #dee2e6
        .rounding(4.0_f32)
        .inner_margin(Margin::symmetric(6.0, 3.0));

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        // -------------------------------------------------------------
        // 左側: ZOOM フレーム (常駐)
        // -------------------------------------------------------------
        frame_style.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;

                // ヘッダーラベル: "ZOOM" (小さな大文字グレー太字, コロンなし)
                ui.label(
                    RichText::new("ZOOM")
                        .size(10.0)
                        .strong()
                        .color(Color32::from_rgb(108, 117, 125)),
                );

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;

                    let is_rect = *active_zoom == Some(ZoomTool::Rect);
                    if light_button(ui, "Rect", is_rect, 46.0).clicked() {
                        *active_zoom = Some(ZoomTool::Rect);
                        state.clear_submodes();
                    }

                    let is_x = *active_zoom == Some(ZoomTool::X);
                    if light_button(ui, "X", is_x, 34.0).clicked() {
                        *active_zoom = Some(ZoomTool::X);
                        state.clear_submodes();
                    }

                    let is_y = *active_zoom == Some(ZoomTool::Y);
                    if light_button(ui, "Y", is_y, 34.0).clicked() {
                        *active_zoom = Some(ZoomTool::Y);
                        state.clear_submodes();
                    }

                    if light_button(ui, "Reset", false, 48.0).clicked() {
                        event = ActionEvent::ResetZoom;
                    }
                });
            });
        });

        // -------------------------------------------------------------
        // 右側: モード専用コンテキストフレーム
        // -------------------------------------------------------------
        if let Some(mode) = active_mode {
            frame_style.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;

                    // モード別ヘッダーラベル (小さな大文字グレー太字, コロンなし)
                    let header_text = match mode {
                        AppMode::Phase => "PHASE CORRECTION",
                        AppMode::Baseline => "BASELINE CORRECTION",
                        AppMode::Reference => "REFERENCE",
                        AppMode::Peak => "PEAK PICKING",
                        AppMode::Integrate => "INTEGRATION",
                        AppMode::Multiview => "MULTIVIEW",
                        AppMode::JCoupling => "J COUPLING",
                    };

                    ui.label(
                        RichText::new(header_text)
                            .size(10.0)
                            .strong()
                            .color(Color32::from_rgb(108, 117, 125)),
                    );

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;

                        match mode {
                            AppMode::Phase => {
                                if light_button(ui, "Auto", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoPhase;
                                }

                                ui.label(RichText::new("P0").size(12.0));
                                ui.add(DragValue::new(p0).speed(0.1).suffix("°"));
                                ui.add(Slider::new(p0, -180.0..=180.0).show_value(false));

                                ui.label(RichText::new("P1").size(12.0));
                                ui.add(DragValue::new(p1).speed(0.5).suffix("°"));
                                ui.add(Slider::new(p1, -360.0..=360.0).show_value(false));

                                if light_button(ui, "Reset", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ResetPhase;
                                }
                            }
                            AppMode::Baseline => {
                                ui.label(RichText::new("Stiffness (log10 λ)").size(12.0));
                                ui.add(DragValue::new(&mut state.baseline_log_lambda).speed(0.1).range(3.0..=12.0));

                                ui.label(RichText::new("Asymmetry (p)").size(12.0));
                                ui.add(DragValue::new(&mut state.baseline_p).speed(0.001).range(0.0001..=0.5));

                                let btn_apply_text = "Apply Correction";
                                if light_button(ui, btn_apply_text, false, 110.0).clicked() {
                                    *active_zoom = None;
                                    state.baseline_applied = true;
                                    event = ActionEvent::ApplyBaseline {
                                        log_lambda: state.baseline_log_lambda,
                                        p: state.baseline_p,
                                    };
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    state.baseline_applied = false;
                                    event = ActionEvent::ClearBaseline;
                                }

                                let (status_txt, status_color) = if state.baseline_applied {
                                    ("Status Applied", Color32::from_rgb(25, 135, 84)) // Bootstrap green
                                } else {
                                    ("Status Not Applied", Color32::from_rgb(108, 117, 125))
                                };
                                ui.label(RichText::new(status_txt).size(11.0).strong().color(status_color));
                            }
                            AppMode::Reference => {
                                let is_13c = nucleus.contains("13C") || nucleus.contains("C13");
                                let solvents: &[(&str, f64)] = if is_13c {
                                    &[
                                        ("CDCl3 (77.16 ppm)", 77.16),
                                        ("DMSO-d6 (39.52 ppm)", 39.52),
                                        ("CD3OD (49.00 ppm)", 49.00),
                                        ("Acetone-d6 (29.84 ppm)", 29.84),
                                        ("CD3CN (1.32 ppm)", 1.32),
                                        ("TMS (0.00 ppm)", 0.00),
                                    ]
                                } else {
                                    &[
                                        ("CDCl3 (7.26 ppm)", 7.26),
                                        ("D2O (4.79 ppm)", 4.79),
                                        ("DMSO-d6 (2.50 ppm)", 2.50),
                                        ("CD3OD (3.31 ppm)", 3.31),
                                        ("Acetone-d6 (2.05 ppm)", 2.05),
                                        ("CD3CN (1.94 ppm)", 1.94),
                                        ("TMS (0.00 ppm)", 0.00),
                                    ]
                                };

                                ui.label(RichText::new("Solvent").size(12.0));
                                let current_solvent_name = solvents
                                    .get(state.ref_solvent_idx)
                                    .map(|s| s.0)
                                    .unwrap_or("Custom");
                                egui::ComboBox::from_id_salt("ref_solvent_combo")
                                    .selected_text(current_solvent_name)
                                    .width(140.0)
                                    .show_ui(ui, |ui| {
                                        for (i, (name, val)) in solvents.iter().enumerate() {
                                            if ui.selectable_label(state.ref_solvent_idx == i, *name).clicked() {
                                                state.ref_solvent_idx = i;
                                                state.ref_target_ppm = *val;
                                            }
                                        }
                                    });

                                ui.label(RichText::new("Target (ppm)").size(12.0));
                                ui.add(DragValue::new(&mut state.ref_target_ppm).speed(0.01));

                                if light_button(ui, "Set", state.ref_set_active, 46.0).clicked() {
                                    state.ref_set_active = true;
                                    *active_zoom = None;
                                }

                                if state.ref_set_active {
                                    ui.label(RichText::new("Drag on peak to reference").size(11.0).color(Color32::from_rgb(13, 110, 253)));
                                } else {
                                    ui.label(RichText::new("Click Set to select peak").size(11.0).italics().color(Color32::from_gray(140)));
                                }
                            }
                            AppMode::Peak => {
                                if light_button(ui, "Auto", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoPeak;
                                }
                                if light_button(ui, "Pick", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::PickPeaks { threshold: state.peak_threshold };
                                }

                                let is_thresh = state.peak_submode == PeakSubMode::Threshold;
                                if light_button(ui, "Threshold", is_thresh, 72.0).clicked() {
                                    state.peak_submode = PeakSubMode::Threshold;
                                    *active_zoom = None;
                                }
                                ui.add(DragValue::new(&mut state.peak_threshold).speed(0.5).range(0.0..=1e9));

                                ui.label(RichText::new(format!("Noise {:.2}", noise_level)).size(11.0).color(Color32::from_rgb(108, 117, 125)));

                                let is_add = state.peak_submode == PeakSubMode::Add;
                                if light_button(ui, "Add", is_add, 46.0).clicked() {
                                    state.peak_submode = PeakSubMode::Add;
                                    *active_zoom = None;
                                }

                                let is_del = state.peak_submode == PeakSubMode::Delete;
                                if light_button(ui, "Delete", is_del, 52.0).clicked() {
                                    state.peak_submode = PeakSubMode::Delete;
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ClearPeaks;
                                }
                            }
                            AppMode::Integrate => {
                                if light_button(ui, "Auto", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoIntegrate;
                                }

                                let is_add = state.integrate_submode == IntegrateSubMode::Add;
                                if light_button(ui, "Add", is_add, 44.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Add;
                                    *active_zoom = None;
                                }

                                let is_edit = state.integrate_submode == IntegrateSubMode::Edit;
                                if light_button(ui, "Edit", is_edit, 44.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Edit;
                                    *active_zoom = None;
                                }

                                let is_split = state.integrate_submode == IntegrateSubMode::Split;
                                if light_button(ui, "Split", is_split, 44.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Split;
                                    *active_zoom = None;
                                }

                                let is_del = state.integrate_submode == IntegrateSubMode::Delete;
                                if light_button(ui, "Delete", is_del, 50.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Delete;
                                    *active_zoom = None;
                                }

                                ui.label(RichText::new("Scale").size(12.0));
                                ui.add(DragValue::new(integration_scale).speed(0.1).range(1e-12..=1e12));

                                ui.label(RichText::new("Ref Val").size(12.0));
                                ui.add(DragValue::new(&mut state.integration_ref_val).speed(0.1).range(0.01..=1000.0));

                                let is_ref = state.integrate_submode == IntegrateSubMode::Reference;
                                if light_button(ui, "Set", is_ref, 40.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Reference;
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ClearIntegrations;
                                }

                                match state.integrate_submode {
                                    IntegrateSubMode::Add => {
                                        ui.label(RichText::new("Drag over peak to integrate").size(11.0).color(Color32::from_rgb(13, 110, 253)));
                                    }
                                    IntegrateSubMode::Edit => {
                                        ui.label(RichText::new("Drag handles or curve").size(11.0).color(Color32::from_rgb(13, 110, 253)));
                                    }
                                    IntegrateSubMode::Split => {
                                        ui.label(RichText::new("Click or drag to split").size(11.0).color(Color32::from_rgb(13, 110, 253)));
                                    }
                                    IntegrateSubMode::Delete => {
                                        ui.label(RichText::new("Click or drag to delete").size(11.0).color(Color32::from_rgb(220, 38, 38)));
                                    }
                                    IntegrateSubMode::Reference => {
                                        ui.label(RichText::new("Click or drag to set reference").size(11.0).color(Color32::from_rgb(13, 110, 253)));
                                    }
                                    _ => {}
                                }
                            }
                            AppMode::Multiview => {
                                ui.label(RichText::new("Ratio").size(12.0));
                                ui.add(DragValue::new(&mut state.multiview_ratio).speed(0.5).range(0.5..=100.0));

                                if light_button(ui, "Auto", false, 44.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoMultiview;
                                }

                                let is_rect = state.multiview_submode == MultiviewSubMode::AddRect;
                                if light_button(ui, "Add (Rect)", is_rect, 68.0).clicked() {
                                    state.multiview_submode = MultiviewSubMode::AddRect;
                                    *active_zoom = None;
                                }

                                let is_x = state.multiview_submode == MultiviewSubMode::AddX;
                                if light_button(ui, "Add (X)", is_x, 58.0).clicked() {
                                    state.multiview_submode = MultiviewSubMode::AddX;
                                    *active_zoom = None;
                                }

                                let is_edit = state.multiview_submode == MultiviewSubMode::Edit;
                                if light_button(ui, "Edit", is_edit, 44.0).clicked() {
                                    state.multiview_submode = MultiviewSubMode::Edit;
                                    *active_zoom = None;
                                }

                                let is_del = state.multiview_submode == MultiviewSubMode::Delete;
                                if light_button(ui, "Delete", is_del, 50.0).clicked() {
                                    state.multiview_submode = MultiviewSubMode::Delete;
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Align", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AlignMultiview;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ResetMultiview;
                                }

                                match state.multiview_submode {
                                    MultiviewSubMode::AddRect => {
                                        ui.label(RichText::new("Drag rectangle on peak to create inset").size(11.0).color(Color32::from_rgb(147, 51, 234)));
                                    }
                                    MultiviewSubMode::AddX => {
                                        ui.label(RichText::new("Drag PPM range to create inset").size(11.0).color(Color32::from_rgb(147, 51, 234)));
                                    }
                                    MultiviewSubMode::Edit => {
                                        ui.label(RichText::new("Drag inside to move, edges to resize, Delete to remove").size(11.0).color(Color32::from_rgb(13, 110, 253)));
                                    }
                                    MultiviewSubMode::Delete => {
                                        ui.label(RichText::new("Click inset to delete").size(11.0).color(Color32::from_rgb(220, 38, 38)));
                                    }
                                    _ => {}
                                }
                            }
                            AppMode::JCoupling => {
                                ui.checkbox(&mut state.non_integer_protons, "Allow Non-Integer");

                                let is_add = state.jcoupling_add_active;
                                if light_button(ui, "Add", is_add, 46.0).clicked() {
                                    state.jcoupling_add_active = true;
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ClearJCoupling;
                                }
                            }
                        }

                        // 各モードの一番右端に Close ボタンを配置 (押すとモードを閉じる)
                        ui.add_space(6.0);
                        if close_button(ui).clicked() {
                            event = ActionEvent::CloseMode;
                        }
                    });
                });
            });
        }
    });

    event
}
