use egui::{ScrollArea, Ui};
use crate::core::{AcquisitionMetadata, JCouplingResultItem};

/// 右サイドパネル (メタデータ & J-Coupling 結果) の表示
pub fn show_side_panel(
    ui: &mut Ui,
    metadata: &AcquisitionMetadata,
    j_couplings: &mut Vec<JCouplingResultItem>,
    selected_j_idx: &mut Option<usize>,
) {
    ui.heading("Analysis Panel");
    ui.separator();

    // 1. メタデータアコーディオン
    egui::CollapsingHeader::new("Acquisition Metadata")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("meta_grid")
                .striped(true)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Title:");
                    ui.label(&metadata.title);
                    ui.end_row();

                    ui.label("Solvent:");
                    ui.label(&metadata.solvent);
                    ui.end_row();

                    ui.label("Nucleus:");
                    ui.label(&metadata.nucleus);
                    ui.end_row();

                    ui.label("Frequency:");
                    ui.label(format!("{:.3} MHz", metadata.obs_freq_mhz));
                    ui.end_row();

                    ui.label("Points:");
                    ui.label(metadata.points.to_string());
                    ui.end_row();

                    ui.label("Sweep Width:");
                    ui.label(format!("{:.1} Hz", metadata.spectral_width_hz));
                    ui.end_row();

                    ui.label("Scans:");
                    ui.label(metadata.scans.to_string());
                    ui.end_row();

                    if metadata.pulse_angle_deg > 0.0 {
                        ui.label("Pulse Angle:");
                        ui.label(format!("{:.1}°", metadata.pulse_angle_deg));
                        ui.end_row();
                    }
                });
        });

    ui.separator();

    // 2. J-Coupling 結果テーブル
    egui::CollapsingHeader::new(format!("J-Couplings ({})", j_couplings.len()))
        .default_open(true)
        .show(ui, |ui| {
            if j_couplings.is_empty() {
                ui.label(
                    egui::RichText::new("No multiplets analyzed yet.\nSelect J-Coupling mode and drag over a peak.")
                        .italics()
                        .color(egui::Color32::from_gray(160)),
                );
            } else {
                let mut remove_idx = None;

                ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for (i, jc) in j_couplings.iter().enumerate() {
                        let is_selected = *selected_j_idx == Some(i);
                        ui.horizontal(|ui| {
                            let label = format!("{}: {}", i + 1, jc.text);
                            let resp = ui.selectable_label(is_selected, label);
                            if resp.clicked() {
                                *selected_j_idx = Some(i);
                            }

                            if ui.small_button("×").clicked() {
                                remove_idx = Some(i);
                            }
                        });
                    }
                });

                if let Some(idx) = remove_idx {
                    j_couplings.remove(idx);
                    if *selected_j_idx == Some(idx) {
                        *selected_j_idx = None;
                    }
                }

                ui.separator();
                if ui.button("Copy All to Clipboard").clicked() {
                    let mut clip = String::new();
                    for jc in j_couplings.iter() {
                        clip.push_str(&jc.text);
                        clip.push('\n');
                    }
                    ui.output_mut(|o| o.copied_text = clip);
                }
            }
        });
}
