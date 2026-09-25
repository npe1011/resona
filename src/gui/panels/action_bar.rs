use egui::{DragValue, Slider, Ui};

use crate::gui::mode::{AppMode, IntegrateSubMode, MultiviewSubMode, PeakSubMode, ZoomSubMode};

#[derive(Debug, Clone, PartialEq)]
pub enum ActionEvent {
    None,
    ResetZoom,
    AutoPhase,
    ResetPhase,
    ApplyBaseline { log_lambda: f64, p: f64 },
    ClearBaseline,
    ApplyShiftReference { target_ppm: f64 },
    AutoPeak,
    PickAllPeaks,
    ClearPeaks,
    AutoIntegrate,
    ClearIntegrations,
    AlignMultiview,
    ResetMultiview,
    ClearJCoupling,
}

/// 専用アクションバーの表示コンポーネント
pub struct ActionBarState {
    pub zoom_submode: ZoomSubMode,
    pub peak_submode: PeakSubMode,
    pub integrate_submode: IntegrateSubMode,
    pub multiview_submode: MultiviewSubMode,
    pub jcoupling_add_active: bool,
    pub non_integer_protons: bool,

    // パラメータ
    pub baseline_log_lambda: f64,
    pub baseline_p: f64,
    pub ref_current_ppm: Option<f64>,
    pub ref_target_ppm: f64,
    pub multiview_ratio: f64,
}

impl Default for ActionBarState {
    fn default() -> Self {
        Self {
            zoom_submode: ZoomSubMode::Rect,
            peak_submode: PeakSubMode::Add,
            integrate_submode: IntegrateSubMode::Add,
            multiview_submode: MultiviewSubMode::AddRect,
            jcoupling_add_active: false,
            non_integer_protons: false,
            baseline_log_lambda: 8.0,
            baseline_p: 0.005,
            ref_current_ppm: None,
            ref_target_ppm: 0.0,
            multiview_ratio: 5.0,
        }
    }
}

pub fn show_action_bar(
    ui: &mut Ui,
    mode: AppMode,
    state: &mut ActionBarState,
    p0: &mut f64,
    p1: &mut f64,
    integration_scale: &mut f64,
) -> ActionEvent {
    let mut event = ActionEvent::None;

    ui.horizontal(|ui| {
        match mode {
            AppMode::View => {
                ui.label(
                    egui::RichText::new("Drag: Pan | Scroll: Zoom | Middle-Click / Home: Reset View")
                        .italics()
                        .color(egui::Color32::from_gray(180)),
                );
            }
            AppMode::Zoom => {
                ui.label("Zoom Submode:");
                if ui.selectable_label(state.zoom_submode == ZoomSubMode::Rect, "Rect").clicked() {
                    state.zoom_submode = ZoomSubMode::Rect;
                }
                if ui.selectable_label(state.zoom_submode == ZoomSubMode::X, "X Only").clicked() {
                    state.zoom_submode = ZoomSubMode::X;
                }
                if ui.selectable_label(state.zoom_submode == ZoomSubMode::Y, "Y Only").clicked() {
                    state.zoom_submode = ZoomSubMode::Y;
                }

                ui.separator();
                if ui.button("Reset Zoom (Home)").clicked() {
                    event = ActionEvent::ResetZoom;
                }
            }
            AppMode::Phase => {
                if ui.button("Auto (ACME)").clicked() {
                    event = ActionEvent::AutoPhase;
                }
                ui.separator();

                ui.label("P0 (deg):");
                ui.add(Slider::new(p0, -360.0..=360.0).step_by(0.1));
                ui.add(DragValue::new(p0).speed(0.1).suffix("°"));

                ui.separator();
                ui.label("P1 (deg):");
                ui.add(Slider::new(p1, -1000.0..=1000.0).step_by(0.5));
                ui.add(DragValue::new(p1).speed(0.5).suffix("°"));

                ui.separator();
                if ui.button("Reset").clicked() {
                    event = ActionEvent::ResetPhase;
                }
            }
            AppMode::Baseline => {
                ui.label("log10(λ):");
                ui.add(DragValue::new(&mut state.baseline_log_lambda).speed(0.1).range(3.0..=12.0));

                ui.label("p (Asymmetry):");
                ui.add(DragValue::new(&mut state.baseline_p).speed(0.0005).range(0.0001..=0.1));

                if ui.button("Apply ALS").clicked() {
                    event = ActionEvent::ApplyBaseline {
                        log_lambda: state.baseline_log_lambda,
                        p: state.baseline_p,
                    };
                }
                if ui.button("Clear Baseline").clicked() {
                    event = ActionEvent::ClearBaseline;
                }
            }
            AppMode::Reference => {
                if let Some(cur) = state.ref_current_ppm {
                    ui.label(format!("Selected: {:.3} ppm", cur));
                } else {
                    ui.label("Drag marker to peak");
                }

                ui.separator();
                ui.label("Target ppm:");
                ui.add(DragValue::new(&mut state.ref_target_ppm).speed(0.01));

                if ui.button("Apply Reference Shift").clicked() {
                    event = ActionEvent::ApplyShiftReference {
                        target_ppm: state.ref_target_ppm,
                    };
                }
            }
            AppMode::Peak => {
                if ui.selectable_label(state.peak_submode == PeakSubMode::Add, "Add Peak").clicked() {
                    state.peak_submode = PeakSubMode::Add;
                }
                if ui.selectable_label(state.peak_submode == PeakSubMode::Delete, "Delete Peak").clicked() {
                    state.peak_submode = PeakSubMode::Delete;
                }

                ui.separator();
                if ui.button("Auto Detect (10x MAD)").clicked() {
                    event = ActionEvent::AutoPeak;
                }
                if ui.button("Pick All at Thresh").clicked() {
                    event = ActionEvent::PickAllPeaks;
                }
                if ui.button("Clear All").clicked() {
                    event = ActionEvent::ClearPeaks;
                }
            }
            AppMode::Integrate => {
                let submodes = [
                    (IntegrateSubMode::Add, "Add"),
                    (IntegrateSubMode::Edit, "Edit"),
                    (IntegrateSubMode::Split, "Split"),
                    (IntegrateSubMode::Delete, "Delete"),
                    (IntegrateSubMode::Reference, "Reference"),
                ];
                for (sm, label) in submodes {
                    if ui.selectable_label(state.integrate_submode == sm, label).clicked() {
                        state.integrate_submode = sm;
                    }
                }

                ui.separator();
                ui.label("Scale:");
                ui.add(Slider::new(integration_scale, 0.1..=10.0).step_by(0.1));

                ui.separator();
                if ui.button("Auto Integrals").clicked() {
                    event = ActionEvent::AutoIntegrate;
                }
                if ui.button("Clear All").clicked() {
                    event = ActionEvent::ClearIntegrations;
                }
            }
            AppMode::Multiview => {
                let submodes = [
                    (MultiviewSubMode::AddRect, "Add Rect"),
                    (MultiviewSubMode::AddX, "Add X"),
                    (MultiviewSubMode::Edit, "Edit"),
                    (MultiviewSubMode::Delete, "Delete"),
                ];
                for (sm, label) in submodes {
                    if ui.selectable_label(state.multiview_submode == sm, label).clicked() {
                        state.multiview_submode = sm;
                    }
                }

                ui.separator();
                ui.label("Inset Ratio:");
                ui.add(DragValue::new(&mut state.multiview_ratio).speed(0.5).range(1.0..=50.0));

                ui.separator();
                if ui.button("Align Top").clicked() {
                    event = ActionEvent::AlignMultiview;
                }
                if ui.button("Reset Insets").clicked() {
                    event = ActionEvent::ResetMultiview;
                }
            }
            AppMode::JCoupling => {
                let toggle_text = if state.jcoupling_add_active {
                    "Analysis Active (Drag to analyze)"
                } else {
                    "Add Multiplet"
                };
                if ui.selectable_label(state.jcoupling_add_active, toggle_text).clicked() {
                    state.jcoupling_add_active = !state.jcoupling_add_active;
                }

                ui.separator();
                ui.checkbox(&mut state.non_integer_protons, "Non-integer H");

                ui.separator();
                if ui.button("Clear Results").clicked() {
                    event = ActionEvent::ClearJCoupling;
                }
            }
        }
    });

    event
}
