use egui::{Color32, RichText, ScrollArea, Ui};
use crate::core::{AcquisitionMetadata, FtSettings, JCouplingResultItem, WindowFunction};

/// 右サイドパネル (ezNMR完全準拠のライトテーマ: FT Settings, Metadata, J-Couplings)
pub fn show_side_panel(
    ui: &mut Ui,
    metadata: &AcquisitionMetadata,
    ft_settings: &FtSettings,
    j_couplings: &mut Vec<JCouplingResultItem>,
    selected_j_idx: &mut Option<usize>,
) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;

        // -------------------------------------------------------------
        // 1. Experimental Parameters テーブル (Python版 ezNMR 完全準拠)
        // -------------------------------------------------------------
        ui.label(RichText::new("Experimental Parameters").strong().size(12.0).color(Color32::from_rgb(70, 75, 80)));
        render_zebra_table(ui, "meta_table", &metadata.to_display_rows());

        ui.separator();

        // -------------------------------------------------------------
        // 2. FT Settings テーブル (Zebra stripe)
        // -------------------------------------------------------------
        ui.label(RichText::new("FT Settings").strong().size(12.0).color(Color32::from_rgb(70, 75, 80)));
        render_zebra_table(ui, "ft_table", &[
            ("Window Type", match ft_settings.window {
                WindowFunction::None => "None".to_string(),
                WindowFunction::Exponential { lb } => format!("Exponential ({} Hz)", lb),
                WindowFunction::Gaussian { g1, g2, g3 } => format!("Gaussian (g1={}, g2={}, g3={})", g1, g2, g3),
            }),
            ("Zero Fill", format!("{}x", ft_settings.zf_factor)),
            ("Digital Resolution", {
                let eff_points = (metadata.points * ft_settings.zf_factor).max(1);
                format!("{:.4} Hz/pt", metadata.spectral_width_hz / (eff_points as f64))
            }),
            ("Group Delay", if ft_settings.remove_digital_filter { "Removed".to_string() } else { "Kept".to_string() }),
            ("Filter Delay", metadata.digital_filter_delay.map(|d| format!("{:.4} pts", d)).unwrap_or_else(|| "-".to_string())),
            ("Auto Phase", if ft_settings.auto_phase { "Yes".to_string() } else { "No".to_string() }),
        ]);

        ui.separator();

        // -------------------------------------------------------------
        // 3. J-Coupling 結果テーブル
        // -------------------------------------------------------------
        ui.label(RichText::new(format!("J-Couplings ({})", j_couplings.len())).strong().size(12.0).color(Color32::from_rgb(70, 75, 80)));

        if j_couplings.is_empty() {
            ui.label(
                RichText::new("No multiplets analyzed yet\nSelect J Coupling and drag over peak")
                    .size(11.0)
                    .italics()
                    .color(Color32::from_gray(140)),
            );
        } else {
            let mut remove_idx = None;

            for (i, jc) in j_couplings.iter().enumerate() {
                let is_selected = *selected_j_idx == Some(i);
                let bg_color = if is_selected {
                    Color32::from_rgb(231, 241, 255)
                } else if i % 2 == 0 {
                    Color32::from_rgb(255, 255, 255)
                } else {
                    Color32::from_rgb(248, 249, 250)
                };

                egui::Frame::none()
                    .fill(bg_color)
                    .inner_margin(egui::Margin::symmetric(4.0, 3.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label = format!("#{}  {}", i + 1, jc.text);
                            let text_color = if is_selected {
                                Color32::from_rgb(8, 66, 152)
                            } else {
                                Color32::from_rgb(40, 40, 40)
                            };

                            let resp = ui.selectable_label(is_selected, RichText::new(label).size(11.0).color(text_color));
                            if resp.clicked() {
                                *selected_j_idx = Some(i);
                            }

                            if ui.small_button("×").clicked() {
                                remove_idx = Some(i);
                            }
                        });
                    });
            }

            if let Some(idx) = remove_idx {
                j_couplings.remove(idx);
                if *selected_j_idx == Some(idx) {
                    *selected_j_idx = None;
                }
            }

            ui.add_space(4.0);
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

/// 2列のゼブラストライプテーブルを描画するヘルパー
fn render_zebra_table(ui: &mut Ui, id_salt: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id_salt)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (key, val) in rows {
                ui.label(RichText::new(*key).size(11.0).color(Color32::from_rgb(108, 117, 125)));
                ui.label(RichText::new(val).size(11.0).strong().color(Color32::from_rgb(33, 37, 41)));
                ui.end_row();
            }
        });
}
