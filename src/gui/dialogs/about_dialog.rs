use egui::{Align2, Color32, Context, RichText, Window};

/// About ダイアログの状態
#[derive(Debug, Clone, Default)]
pub struct AboutDialogState {
    pub open: bool,
}

/// About ダイアログの描画
pub fn show_about_dialog(ctx: &Context, state: &mut AboutDialogState) {
    if !state.open {
        return;
    }

    let version = env!("CARGO_PKG_VERSION");
    let build_date = option_env!("RESONA_BUILD_DATE").unwrap_or("2026-10-07");

    let mut is_open = state.open;
    let mut close_clicked = false;

    Window::new("About Resona")
        .open(&mut is_open)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(380.0)
        .show(ctx, |ui| {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }

            ui.spacing_mut().item_spacing.y = 8.0;

            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Resona")
                        .size(24.0)
                        .strong()
                        .color(Color32::from_rgb(33, 37, 41)),
                );
                ui.label(
                    RichText::new(format!("Version {}", version))
                        .size(14.0)
                        .color(Color32::from_rgb(108, 117, 125)),
                );
                ui.label(
                    RichText::new(format!("Build: {}", build_date))
                        .size(12.0)
                        .color(Color32::from_rgb(108, 117, 125)),
                );
                ui.add_space(2.0);
                ui.hyperlink("https://github.com/npe1011/resona");
            });

            ui.separator();

            ui.label("A lightweight simple 1D NMR spectroscopy analysis application written in Rust.");

            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(RichText::new("License Information").strong());
                ui.label("Released under the MIT License.");
                ui.label(
                    RichText::new("Copyright (c) 2026 Tatsuhiko Yoshino")
                        .small()
                        .color(Color32::from_rgb(108, 117, 125)),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(
                        "All dependencies use permissive open-source licenses (MIT, Apache-2.0, BSD, etc.). See THIRD_PARTY_LICENSES.md for full details.",
                    )
                    .small(),
                );
            });

            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                if ui.button(RichText::new("  OK  ").strong()).clicked() {
                    close_clicked = true;
                }
            });
        });

    if close_clicked || !is_open {
        state.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_about_dialog_state_default() {
        let state = AboutDialogState::default();
        assert!(!state.open);
    }
}
