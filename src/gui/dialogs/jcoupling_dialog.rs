use egui::{TextEdit, Window};
use crate::core::JCouplingCandidate;

pub struct JCouplingDialogState {
    pub open: bool,
    pub candidates: Vec<JCouplingCandidate>,
    pub selected_idx: usize,
    pub edited_text: String,
    pub center_ppm: f64,
}

impl Default for JCouplingDialogState {
    fn default() -> Self {
        Self {
            open: false,
            candidates: Vec::new(),
            selected_idx: 0,
            edited_text: String::new(),
            center_ppm: 0.0,
        }
    }
}

pub fn show_jcoupling_dialog(
    ctx: &egui::Context,
    state: &mut JCouplingDialogState,
) -> Option<(String, f64)> {
    let mut confirmed = None;
    if !state.open {
        return None;
    }

    Window::new("J-Coupling Multiplet Analysis")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.heading("Select Multiplet Candidate");
            ui.separator();

            if state.candidates.is_empty() {
                ui.label("No candidates found.");
            } else {
                for (i, cand) in state.candidates.iter().enumerate() {
                    let err_str = if cand.error.is_finite() {
                        format!(" [Error: {:.3}]", cand.error)
                    } else {
                        "".to_string()
                    };
                    let label = format!("{}{}", cand.text, err_str);

                    if ui.radio(state.selected_idx == i, label).clicked() {
                        state.selected_idx = i;
                        state.edited_text = cand.text.clone();
                    }
                }
            }

            ui.separator();
            ui.label("Edit Output Text:");
            ui.add(TextEdit::singleline(&mut state.edited_text).desired_width(320.0));

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Add to Results").clicked() {
                    if !state.edited_text.trim().is_empty() {
                        confirmed = Some((state.edited_text.clone(), state.center_ppm));
                    }
                    state.open = false;
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }
            });
        });

    confirmed
}
