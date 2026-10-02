use egui::{vec2, Align2, Button, Color32, RichText, Window};

#[derive(Debug, Clone, Default)]
pub struct PeakListDialogState {
    pub open: bool,
    pub text: String,
    pub peak_count: usize,
    pub copied: bool,
}

pub fn show_peak_list_dialog(
    ctx: &egui::Context,
    state: &mut PeakListDialogState,
) {
    if !state.open {
        return;
    }

    Window::new("Peak List")
        .collapsible(false)
        .resizable(true)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .default_size([440.0, 240.0])
        .min_width(320.0)
        .min_height(180.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("PPM values ({} peaks, low-field to high-field):", state.peak_count))
                        .strong()
                        .size(12.5),
                );
            });

            // 編集不能な複数行テキストエリア (選択・コピー可能)
            ui.add(
                egui::TextEdit::multiline(&mut state.text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(6)
                    .interactive(true),
            );

            ui.separator();

            ui.horizontal(|ui| {
                let btn_copy = Button::new(RichText::new("Copy to Clipboard").size(12.0))
                    .min_size(vec2(120.0, 24.0));
                if ui.add(btn_copy).clicked() {
                    ui.output_mut(|o| o.copied_text = state.text.clone());
                    state.copied = true;
                }

                if state.copied {
                    ui.label(RichText::new("Copied!").color(Color32::from_rgb(16, 185, 129)).size(12.0));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        state.open = false;
                        state.copied = false;
                    }
                });
            });
        });
}
