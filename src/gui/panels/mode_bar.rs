use egui::Ui;

use crate::gui::mode::AppMode;

/// メインモード切替ツールバーの描画
pub fn show_mode_bar(ui: &mut Ui, current_mode: &mut AppMode) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("MODE:").strong());

        let modes = [
            (AppMode::View, "View"),
            (AppMode::Zoom, "Zoom"),
            (AppMode::Phase, "Phase"),
            (AppMode::Baseline, "Baseline"),
            (AppMode::Reference, "Reference"),
            (AppMode::Peak, "Peak"),
            (AppMode::Integrate, "Integrate"),
            (AppMode::Multiview, "Multiview"),
            (AppMode::JCoupling, "J-Coupling"),
        ];

        for (mode, label) in modes {
            let is_selected = *current_mode == mode;
            let resp = ui.selectable_label(is_selected, label);
            if resp.clicked() && !is_selected {
                *current_mode = mode;
                changed = true;
            }
        }
    });
    changed
}
