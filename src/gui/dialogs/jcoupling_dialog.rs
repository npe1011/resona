use egui::{
    vec2, Color32, DragValue, FontId, Pos2, Rect, RichText, Stroke, TextEdit, Ui, Window,
};
use crate::core::analysis::jcoupling::{evaluate_lorentzian_sum, generate_theoretical_peaks};
use crate::core::JCouplingCandidate;

pub struct JCouplingDialogState {
    pub open: bool,
    pub candidates: Vec<JCouplingCandidate>,
    pub selected_idx: usize,
    pub edited_text: String,
    pub center_ppm: f64,
    pub p_min: f64,
    pub p_max: f64,
    pub obs_freq_mhz: f64,
    pub real_ppm: Vec<f64>,
    pub real_spectrum: Vec<f64>,
    pub sim_fwhm_hz: f64,
}

impl Default for JCouplingDialogState {
    fn default() -> Self {
        Self {
            open: false,
            candidates: Vec::new(),
            selected_idx: 0,
            edited_text: String::new(),
            center_ppm: 0.0,
            p_min: 0.0,
            p_max: 0.0,
            obs_freq_mhz: 400.0,
            real_ppm: Vec::new(),
            real_spectrum: Vec::new(),
            sim_fwhm_hz: 1.5,
        }
    }
}

/// 実測スペクトルと選択候補の理論シミュレーション（ローレンツ曲線＋スティック棒）を重ねて描画するプレビュープロット
fn paint_jcoupling_preview(ui: &mut Ui, state: &JCouplingDialogState) {
    let (response, painter) = ui.allocate_painter(vec2(380.0, 240.0), egui::Sense::hover());
    let rect = response.rect;

    // 1. 純白背景と薄いグレー枠線
    painter.rect_filled(rect, 4.0, Color32::WHITE);
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230)),
    );

    // プロット描画領域 (上部 28px: 凡例、下部 28px: 目盛り、左右 16px)
    let plot_rect = Rect::from_min_max(
        Pos2::new(rect.min.x + 16.0, rect.min.y + 28.0),
        Pos2::new(rect.max.x - 16.0, rect.max.y - 28.0),
    );

    if plot_rect.width() <= 10.0 || plot_rect.height() <= 10.0 {
        return;
    }

    // 2. X軸 (ppm) 座標系: NMR慣例に従い左が高ppm、右が低ppm
    let p_min = state.p_min.min(state.p_max);
    let p_max = state.p_min.max(state.p_max);
    let p_span = (p_max - p_min).abs().max(1e-6);

    // ピーク端の張り付きを防ぐため 5% マージン
    let disp_p_min = p_min - p_span * 0.05;
    let disp_p_max = p_max + p_span * 0.05;
    let disp_p_span = (disp_p_max - disp_p_min).abs().max(1e-6);

    let ppm_to_x = |ppm: f64| -> f32 {
        let norm = ((disp_p_max - ppm) / disp_p_span) as f32;
        plot_rect.min.x + norm * plot_rect.width()
    };

    let x_to_ppm = |x: f32| -> f64 {
        let norm = ((x - plot_rect.min.x) / plot_rect.width()) as f64;
        disp_p_max - norm * disp_p_span
    };

    // 3. Y軸 (強度) 座標系
    let mut real_y_min = f64::MAX;
    let mut real_y_max = f64::MIN;
    for &y in &state.real_spectrum {
        if y < real_y_min {
            real_y_min = y;
        }
        if y > real_y_max {
            real_y_max = y;
        }
    }
    if real_y_min >= real_y_max {
        real_y_min = 0.0;
        real_y_max = 1.0;
    }

    let y_height = (real_y_max - real_y_min).max(1e-12);
    let base_y = real_y_min; // ベースライン基準

    let disp_y_min = base_y - y_height * 0.08;
    let disp_y_max = base_y + y_height * 1.15;
    let disp_y_span = (disp_y_max - disp_y_min).max(1e-12);

    let val_to_y = |val: f64| -> f32 {
        let norm = ((val - disp_y_min) / disp_y_span) as f32;
        plot_rect.max.y - norm * plot_rect.height()
    };

    // 4. X軸線と目盛り (Ticks)
    let axis_y = val_to_y(base_y);
    painter.line_segment(
        [
            Pos2::new(plot_rect.min.x, axis_y),
            Pos2::new(plot_rect.max.x, axis_y),
        ],
        Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)),
    );

    // 4〜5 本の目盛り
    let num_ticks = 5;
    let font_tick = FontId::proportional(10.0);
    let color_tick = Color32::from_rgb(108, 117, 125);
    for i in 0..num_ticks {
        let frac = i as f64 / (num_ticks - 1) as f64;
        let tick_ppm = disp_p_max - frac * disp_p_span;
        let tx = ppm_to_x(tick_ppm);

        // 刻み線 (3px 下方向)
        painter.line_segment(
            [Pos2::new(tx, axis_y), Pos2::new(tx, axis_y + 3.0)],
            Stroke::new(1.0_f32, color_tick),
        );

        // PPM ラベル
        let label = format!("{:.3}", tick_ppm);
        let galley = painter.layout_no_wrap(label, font_tick.clone(), color_tick);
        let text_pos = Pos2::new(tx - galley.size().x * 0.5, axis_y + 5.0);
        painter.galley(text_pos, galley, color_tick);
    }

    // 5. 実測スペクトルの描画 (青色曲線)
    let n_pts = state.real_ppm.len().min(state.real_spectrum.len());
    if n_pts >= 2 {
        let mut real_pts: Vec<Pos2> = Vec::with_capacity(n_pts);
        for i in 0..n_pts {
            let px = ppm_to_x(state.real_ppm[i]);
            let py = val_to_y(state.real_spectrum[i]);
            real_pts.push(Pos2::new(px, py));
        }
        let stroke_real = Stroke::new(1.2_f32, Color32::from_rgb(37, 99, 235)); // #2563eb
        painter.add(egui::Shape::line(real_pts, stroke_real));
    }

    // 6. 現在選択された候補のシミュレーション描画
    let selected_cand = state.candidates.get(state.selected_idx);
    let mut has_sim = false;

    if let Some(cand) = selected_cand {
        if cand.pattern == "m" {
            // multiplet (m) はシミュレーションなし
            let msg = "No simulation for multiplet (m)";
            let font_msg = FontId::proportional(12.0);
            let galley = painter.layout_no_wrap(
                msg.to_string(),
                font_msg,
                Color32::from_rgb(148, 163, 184),
            );
            let center_pos = Pos2::new(
                plot_rect.center().x - galley.size().x * 0.5,
                plot_rect.center().y - galley.size().y * 0.5,
            );
            painter.galley(center_pos, galley, Color32::from_rgb(148, 163, 184));
        } else {
            let theo_peaks = generate_theoretical_peaks(&cand.pattern, &cand.j_vals);
            if !theo_peaks.is_empty() {
                has_sim = true;
                let freq = state.obs_freq_mhz.max(1.0);

                // 各理論ピークの最大ローレンツ値を算出（正規化基準）
                let mut max_theo_lor = 0.0_f64;
                for &(p_hz, _) in &theo_peaks {
                    let v = evaluate_lorentzian_sum(&theo_peaks, p_hz, state.sim_fwhm_hz);
                    if v > max_theo_lor {
                        max_theo_lor = v;
                    }
                }
                if max_theo_lor <= 0.0 {
                    max_theo_lor = 1.0;
                }

                // 6.1 ローレンツ連続曲線の描画 (赤色曲線)
                let num_samples = (plot_rect.width().round() as usize).max(60);
                let mut sim_pts: Vec<Pos2> = Vec::with_capacity(num_samples + 1);
                for i in 0..=num_samples {
                    let sx = plot_rect.min.x + (i as f32 / num_samples as f32) * plot_rect.width();
                    let p = x_to_ppm(sx);
                    let delta_hz = (p - state.center_ppm) * freq;
                    let lor_val =
                        evaluate_lorentzian_sum(&theo_peaks, delta_hz, state.sim_fwhm_hz);
                    let scaled_val = base_y + (lor_val / max_theo_lor) * y_height;
                    let sy = val_to_y(scaled_val);
                    sim_pts.push(Pos2::new(sx, sy));
                }
                let stroke_sim = Stroke::new(1.5_f32, Color32::from_rgb(220, 38, 38)); // #dc2626
                painter.add(egui::Shape::line(sim_pts, stroke_sim));

                // 6.2 理論ピーク位置のスティック (縦線 & 上端ドット)
                let stroke_stick = Stroke::new(1.4_f32, Color32::from_rgb(192, 38, 211)); // #c026d3
                let dot_radius = 2.5_f32;
                let color_dot = Color32::from_rgb(192, 38, 211);

                for &(p_hz, weight) in &theo_peaks {
                    let peak_ppm = state.center_ppm + p_hz / freq;
                    let px = ppm_to_x(peak_ppm);
                    let peak_val = base_y + weight * y_height;
                    let py_top = val_to_y(peak_val);

                    if px >= plot_rect.min.x && px <= plot_rect.max.x {
                        painter.line_segment(
                            [Pos2::new(px, axis_y), Pos2::new(px, py_top)],
                            stroke_stick,
                        );
                        painter.circle_filled(Pos2::new(px, py_top), dot_radius, color_dot);
                    }
                }
            }
        }
    }

    // 7. 凡例 (プロット上部の余白領域)
    let legend_y = rect.min.y + 14.0;
    let font_legend = FontId::proportional(11.0);
    let color_legend_txt = Color32::from_rgb(73, 80, 87);

    let mut legend_x = plot_rect.min.x;

    // Real: 青線
    painter.line_segment(
        [
            Pos2::new(legend_x, legend_y),
            Pos2::new(legend_x + 14.0, legend_y),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(37, 99, 235)),
    );
    legend_x += 18.0;
    let g_real = painter.layout_no_wrap(
        "Real Spectrum".to_string(),
        font_legend.clone(),
        color_legend_txt,
    );
    let w_real = g_real.size().x;
    painter.galley(Pos2::new(legend_x, legend_y - 6.0), g_real, color_legend_txt);
    legend_x += w_real + 16.0;

    if has_sim {
        // Sim: 赤線
        painter.line_segment(
            [
                Pos2::new(legend_x, legend_y),
                Pos2::new(legend_x + 14.0, legend_y),
            ],
            Stroke::new(2.0_f32, Color32::from_rgb(220, 38, 38)),
        );
        legend_x += 18.0;
        let g_sim = painter.layout_no_wrap(
            "Simulation".to_string(),
            font_legend.clone(),
            color_legend_txt,
        );
        let w_sim = g_sim.size().x;
        painter.galley(Pos2::new(legend_x, legend_y - 6.0), g_sim, color_legend_txt);
        legend_x += w_sim + 16.0;

        // Stick: 紫線 + 丸
        painter.line_segment(
            [
                Pos2::new(legend_x, legend_y + 4.0),
                Pos2::new(legend_x, legend_y - 4.0),
            ],
            Stroke::new(1.8_f32, Color32::from_rgb(192, 38, 211)),
        );
        painter.circle_filled(
            Pos2::new(legend_x, legend_y - 4.0),
            2.2,
            Color32::from_rgb(192, 38, 211),
        );
        legend_x += 6.0;
        let g_stick = painter.layout_no_wrap("Stick".to_string(), font_legend, color_legend_txt);
        painter.galley(Pos2::new(legend_x, legend_y - 6.0), g_stick, color_legend_txt);
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
            ui.horizontal(|ui| {
                // ==========================================
                // 左カラム: 候補選択 & テキスト編集 (幅 320px)
                // ==========================================
                ui.vertical(|ui| {
                    ui.set_width(320.0);
                    ui.heading("Select Multiplet Candidate");
                    ui.separator();

                    if state.candidates.is_empty() {
                        ui.label("No candidates found.");
                    } else {
                        for (i, cand) in state.candidates.iter().enumerate() {
                            let err_str = if cand.error.is_finite() {
                                format!(" [Error {:.3}]", cand.error)
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

                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(RichText::new("Edit Output Text").strong());
                    ui.add(TextEdit::singleline(&mut state.edited_text).desired_width(310.0));

                    ui.add_space(10.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        let add_btn = egui::Button::new(
                            RichText::new("Add to Results").strong().color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(37, 99, 235))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(29, 78, 216)))
                        .rounding(3.0);

                        if ui.add(add_btn).clicked() {
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

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // ==========================================
                // 右カラム: シミュレーション & スペクトル重ね合わせ (幅 380px)
                // ==========================================
                ui.vertical(|ui| {
                    ui.set_width(380.0);
                    ui.horizontal(|ui| {
                        ui.heading("Simulation & Spectrum");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                DragValue::new(&mut state.sim_fwhm_hz)
                                    .speed(0.05)
                                    .range(0.2..=10.0)
                                    .suffix(" Hz"),
                            );
                            ui.label(
                                RichText::new("FWHM:")
                                    .size(11.5)
                                    .color(Color32::from_rgb(108, 117, 125)),
                            );
                        });
                    });
                    ui.separator();

                    paint_jcoupling_preview(ui, state);
                });
            });
        });

    confirmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jcoupling_dialog_state_default() {
        let state = JCouplingDialogState::default();
        assert!(!state.open);
        assert_eq!(state.selected_idx, 0);
        assert_eq!(state.sim_fwhm_hz, 1.5);
        assert!(state.candidates.is_empty());
        assert!(state.real_ppm.is_empty());
        assert!(state.real_spectrum.is_empty());
    }

    #[test]
    fn test_jcoupling_dialog_state_with_candidate() {
        let mut state = JCouplingDialogState {
            open: true,
            candidates: vec![
                JCouplingCandidate {
                    pattern: "d".to_string(),
                    j_vals: vec![7.8],
                    error: 0.05,
                    text: "7.26 (d, J = 7.8 Hz, 1H)".to_string(),
                },
                JCouplingCandidate {
                    pattern: "m".to_string(),
                    j_vals: Vec::new(),
                    error: 999.0,
                    text: "7.27-7.25 (m, 1H)".to_string(),
                },
            ],
            selected_idx: 0,
            edited_text: "7.26 (d, J = 7.8 Hz, 1H)".to_string(),
            center_ppm: 7.26,
            p_min: 7.25,
            p_max: 7.27,
            obs_freq_mhz: 400.0,
            real_ppm: vec![7.25, 7.26, 7.27],
            real_spectrum: vec![10.0, 100.0, 15.0],
            sim_fwhm_hz: 1.5,
        };

        assert_eq!(state.candidates.len(), 2);
        assert_eq!(state.candidates[0].pattern, "d");
        state.selected_idx = 1;
        state.edited_text = state.candidates[1].text.clone();
        assert_eq!(state.edited_text, "7.27-7.25 (m, 1H)");
    }
}
