use egui::{
    vec2, Align2, Button, Color32, DragValue, FontId, Pos2, Rect, RichText, Slider, Stroke, Ui,
    Window,
};
use ndarray::Array1;
use num_complex::Complex64;

use crate::core::{
    apply_phase_and_extract_real, autophase_acme, compute_window_curve, process_raw_fid,
    AcquisitionMetadata, FtSettings, RawFid, WindowFunction,
};

pub struct FtDialogState {
    pub open: bool,
    pub settings: FtSettings,
    pub preview_ppm: Option<Array1<f64>>,
    pub preview_spectrum: Option<Array1<f64>>,
    last_settings: Option<FtSettings>,
}

impl Default for FtDialogState {
    fn default() -> Self {
        Self {
            open: false,
            settings: FtSettings::default(),
            preview_ppm: None,
            preview_spectrum: None,
            last_settings: None,
        }
    }
}

impl FtDialogState {
    pub fn reset_preview(&mut self) {
        self.preview_ppm = None;
        self.preview_spectrum = None;
        self.last_settings = None;
    }
}

/// FID波形およびWindow関数曲線のプレビュー描画
fn paint_fid_preview(
    ui: &mut Ui,
    rect: Rect,
    fid: &Array1<Complex64>,
    metadata: &AcquisitionMetadata,
    window: &WindowFunction,
) {
    let painter = ui.painter_at(rect);

    // 背景と枠線
    painter.rect_filled(rect, 3.0, Color32::WHITE);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230)));

    let n = fid.len();
    if n < 2 {
        return;
    }

    let plot_rect = rect.shrink2(vec2(12.0, 16.0));
    if plot_rect.width() <= 10.0 || plot_rect.height() <= 10.0 {
        return;
    }

    // FID実部の最大振幅を算出 (ベースライン y=0 を中心とする)
    let mut max_abs_y = 1e-12_f64;
    for c in fid.iter() {
        let abs_re = c.re.abs();
        if abs_re > max_abs_y {
            max_abs_y = abs_re;
        }
    }
    let y_limit = max_abs_y * 1.15;
    let center_y = plot_rect.center().y;

    // 水平中心線 (y=0)
    painter.line_segment(
        [Pos2::new(plot_rect.min.x, center_y), Pos2::new(plot_rect.max.x, center_y)],
        Stroke::new(1.0_f32, Color32::from_rgb(235, 238, 242)),
    );

    // 1. 生FID波形の描画 (Min/Max リサンプリング)
    let px_w = plot_rect.width();
    let mut fid_points: Vec<Pos2> = Vec::with_capacity((px_w as usize) * 2 + 10);

    let mut cur_px = -9999.0_f32;
    let mut min_sy = f32::MAX;
    let mut max_sy = f32::MIN;
    let mut first_sy = 0.0_f32;

    for i in 0..n {
        let frac = i as f32 / (n - 1) as f32;
        let sx = plot_rect.min.x + frac * plot_rect.width();
        let y_val = fid[i].re;
        let sy = center_y - ((y_val / y_limit) as f32) * (plot_rect.height() * 0.5);

        let px = sx.floor();
        if px != cur_px {
            if cur_px >= plot_rect.min.x - 1.0 && cur_px <= plot_rect.max.x + 1.0 {
                if (max_sy - min_sy).abs() < 1.0 {
                    fid_points.push(Pos2::new(cur_px, first_sy));
                } else {
                    fid_points.push(Pos2::new(cur_px, min_sy));
                    fid_points.push(Pos2::new(cur_px, max_sy));
                }
            }
            cur_px = px;
            min_sy = sy;
            max_sy = sy;
            first_sy = sy;
        } else {
            min_sy = min_sy.min(sy);
            max_sy = max_sy.max(sy);
        }
    }
    if cur_px >= plot_rect.min.x - 1.0 && cur_px <= plot_rect.max.x + 1.0 {
        if (max_sy - min_sy).abs() < 1.0 {
            fid_points.push(Pos2::new(cur_px, first_sy));
        } else {
            fid_points.push(Pos2::new(cur_px, min_sy));
            fid_points.push(Pos2::new(cur_px, max_sy));
        }
    }

    if fid_points.len() >= 2 {
        painter.add(egui::Shape::line(
            fid_points,
            Stroke::new(1.0_f32, Color32::from_rgb(37, 99, 235)), // 青色
        ));
    }

    // 2. Window関数の描画 (FIDのグラフに合わせて赤線で描画)
    let win_curve = compute_window_curve(window, n, metadata.spectral_width_hz);
    let n_steps = (plot_rect.width() as usize).clamp(100, 800);
    let mut win_points: Vec<Pos2> = Vec::with_capacity(n_steps + 1);
    let mut win_points_lower: Vec<Pos2> = Vec::with_capacity(n_steps + 1);

    for s in 0..=n_steps {
        let frac = s as f32 / n_steps as f32;
        let idx = ((frac * (n - 1) as f32).round() as usize).min(n - 1);
        let sx = plot_rect.min.x + frac * plot_rect.width();

        // 窓関数値 (通常 0.0 ~ 1.0) を生FIDのピーク振幅に合わせてスケーリング
        let w_val = win_curve[idx];
        let sy_upper = center_y - ((w_val * max_abs_y / y_limit) as f32) * (plot_rect.height() * 0.5);
        let sy_lower = center_y + ((w_val * max_abs_y / y_limit) as f32) * (plot_rect.height() * 0.5);

        win_points.push(Pos2::new(sx, sy_upper));
        win_points_lower.push(Pos2::new(sx, sy_lower));
    }

    // 上側包絡線 (鮮やかな赤線・太さ1.6px)
    painter.add(egui::Shape::line(
        win_points,
        Stroke::new(1.6_f32, Color32::from_rgb(220, 38, 38)),
    ));
    // 下側包絡線 (薄い半透明赤のガイド線)
    painter.add(egui::Shape::line(
        win_points_lower,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(220, 38, 38, 80)),
    ));

    // ヘッダー情報・バッジ
    let title_font = FontId::proportional(11.5);
    painter.text(
        Pos2::new(plot_rect.min.x + 4.0, plot_rect.min.y + 4.0),
        Align2::LEFT_TOP,
        "FID (Raw Data)",
        title_font,
        Color32::from_rgb(33, 37, 41),
    );

    let sw = metadata.spectral_width_hz;
    let total_time_ms = if sw > 0.0 { (n as f64 / sw) * 1000.0 } else { 0.0 };
    let time_label = format!("{:.1} ms ({} pts)", total_time_ms, n);
    painter.text(
        Pos2::new(plot_rect.max.x - 4.0, plot_rect.min.y + 4.0),
        Align2::RIGHT_TOP,
        time_label,
        FontId::proportional(10.5),
        Color32::from_rgb(108, 117, 125),
    );

    // 凡例バッジ (赤色: Window Function)
    let win_desc = match window {
        WindowFunction::None => "Window: None".to_string(),
        WindowFunction::Exponential { lb } => format!("Window: EM (LB={:.2} Hz)", lb),
        WindowFunction::Gaussian { g1, g2, g3 } => format!("Window: GM (g1={:.1}, g2={:.1}, g3={:.2})", g1, g2, g3),
    };
    painter.text(
        Pos2::new(plot_rect.max.x - 4.0, plot_rect.max.y - 4.0),
        Align2::RIGHT_BOTTOM,
        win_desc,
        FontId::proportional(10.5),
        Color32::from_rgb(220, 38, 38),
    );
}

/// FT後スペクトルのプレビュー描画
fn paint_spectrum_preview(
    ui: &mut Ui,
    rect: Rect,
    ppm: &Array1<f64>,
    spec: &Array1<f64>,
) {
    let painter = ui.painter_at(rect);

    // 背景と枠線
    painter.rect_filled(rect, 3.0, Color32::WHITE);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230)));

    let n = ppm.len().min(spec.len());
    if n < 2 {
        return;
    }

    let plot_rect = rect.shrink2(vec2(12.0, 16.0));
    if plot_rect.width() <= 10.0 || plot_rect.height() <= 10.0 {
        return;
    }

    let ppm_start = ppm[0];
    let ppm_end = ppm[n - 1];
    let (p_max, p_min) = if ppm_start > ppm_end {
        (ppm_start, ppm_end)
    } else {
        (ppm_end, ppm_start)
    };
    let ppm_span = (p_max - p_min).abs().max(1e-6);

    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    for &y in spec.iter() {
        if y < y_min { y_min = y; }
        if y > y_max { y_max = y; }
    }
    if y_min >= y_max {
        y_min = 0.0;
        y_max = 1.0;
    }
    let y_span = y_max - y_min;
    let eff_y_min = y_min - y_span * 0.05;
    let eff_y_max = y_max + y_span * 0.12;
    let eff_y_span = (eff_y_max - eff_y_min).max(1e-12);

    // スペクトル曲線の Min/Max リサンプリング描画
    let px_w = plot_rect.width();
    let mut spec_points: Vec<Pos2> = Vec::with_capacity((px_w as usize) * 2 + 10);

    let mut cur_px = -9999.0_f32;
    let mut min_sy = f32::MAX;
    let mut max_sy = f32::MIN;
    let mut first_sy = 0.0_f32;

    for i in 0..n {
        let p = ppm[i];
        let frac = ((p_max - p) / ppm_span) as f32;
        let sx = plot_rect.min.x + frac * plot_rect.width();
        let y_val = spec[i];
        let sy = plot_rect.max.y - (((y_val - eff_y_min) / eff_y_span) as f32) * plot_rect.height();

        let px = sx.floor();
        if px != cur_px {
            if cur_px >= plot_rect.min.x - 1.0 && cur_px <= plot_rect.max.x + 1.0 {
                if (max_sy - min_sy).abs() < 1.0 {
                    spec_points.push(Pos2::new(cur_px, first_sy));
                } else {
                    spec_points.push(Pos2::new(cur_px, min_sy));
                    spec_points.push(Pos2::new(cur_px, max_sy));
                }
            }
            cur_px = px;
            min_sy = sy;
            max_sy = sy;
            first_sy = sy;
        } else {
            min_sy = min_sy.min(sy);
            max_sy = max_sy.max(sy);
        }
    }
    if cur_px >= plot_rect.min.x - 1.0 && cur_px <= plot_rect.max.x + 1.0 {
        if (max_sy - min_sy).abs() < 1.0 {
            spec_points.push(Pos2::new(cur_px, first_sy));
        } else {
            spec_points.push(Pos2::new(cur_px, min_sy));
            spec_points.push(Pos2::new(cur_px, max_sy));
        }
    }

    if spec_points.len() >= 2 {
        painter.add(egui::Shape::line(
            spec_points,
            Stroke::new(1.2_f32, Color32::from_rgb(17, 24, 39)), // 黒色
        ));
    }

    // タイトルと PPM 範囲
    painter.text(
        Pos2::new(plot_rect.min.x + 4.0, plot_rect.min.y + 4.0),
        Align2::LEFT_TOP,
        "Transformed Spectrum Preview",
        FontId::proportional(11.5),
        Color32::from_rgb(33, 37, 41),
    );

    let info_txt = format!("{:.2} ~ {:.2} ppm ({} pts)", p_max, p_min, n);
    painter.text(
        Pos2::new(plot_rect.max.x - 4.0, plot_rect.min.y + 4.0),
        Align2::RIGHT_TOP,
        info_txt,
        FontId::proportional(10.5),
        Color32::from_rgb(108, 117, 125),
    );

    // PPM軸の簡単な目盛り表示 (左右端および中央)
    let axis_y = plot_rect.max.y - 2.0;
    let ticks = [0.0_f32, 0.25, 0.5, 0.75, 1.0];
    for &frac in &ticks {
        let sx = plot_rect.min.x + frac * plot_rect.width();
        let ppm_val = p_max - (frac as f64) * ppm_span;
        painter.line_segment([Pos2::new(sx, axis_y - 3.0), Pos2::new(sx, axis_y)], Stroke::new(1.0_f32, Color32::from_rgb(173, 181, 189)));
        painter.text(
            Pos2::new(sx, axis_y + 1.0),
            Align2::CENTER_TOP,
            format!("{:.1}", ppm_val),
            FontId::proportional(9.0),
            Color32::from_rgb(108, 117, 125),
        );
    }
}

pub fn show_ft_dialog(
    ctx: &egui::Context,
    state: &mut FtDialogState,
    fid_raw: Option<&Array1<Complex64>>,
    metadata: Option<&AcquisitionMetadata>,
    group_delay: Option<f64>,
) -> Option<FtSettings> {
    let mut applied_settings = None;
    if !state.open {
        return None;
    }

    // 設定変更時または初回にプレビューを再計算 (ACME Autophase を自動適用)
    if state.last_settings.as_ref() != Some(&state.settings) || state.preview_spectrum.is_none() {
        if let (Some(fid), Some(meta)) = (fid_raw, metadata) {
            let raw = RawFid {
                data: fid.clone(),
                metadata: meta.clone(),
                group_delay,
            };
            if let Ok(processed) = process_raw_fid(&raw, &state.settings, 0.0, 0.0) {
                let (prev_p0, prev_p1) = autophase_acme(&processed.complex_spectrum_unphased);
                let preview_real = apply_phase_and_extract_real(
                    &processed.complex_spectrum_unphased,
                    prev_p0,
                    prev_p1,
                );
                state.preview_ppm = Some(processed.ppm);
                state.preview_spectrum = Some(preview_real);
            }
        }
        state.last_settings = Some(state.settings.clone());
    }

    Window::new("Fourier Transform Settings")
        .collapsible(false)
        .resizable(false)
        .default_size([960.0, 580.0])
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let graph_w = 660.0_f32;
                let right_panel_w = 260.0_f32;
                let half_h = 240.0_f32;

                ui.vertical(|ui| {
                    ui.set_width(graph_w);

                    // 1. 上段: FID プレビュー
                    let (fid_rect, _) = ui.allocate_exact_size(vec2(graph_w, half_h), egui::Sense::hover());
                    if let (Some(fid), Some(meta)) = (fid_raw, metadata) {
                        paint_fid_preview(ui, fid_rect, fid, meta, &state.settings.window);
                    } else {
                        ui.painter().rect_filled(fid_rect, 3.0, Color32::from_rgb(248, 249, 250));
                        ui.painter().text(
                            fid_rect.center(),
                            Align2::CENTER_CENTER,
                            "No FID raw data available",
                            FontId::proportional(13.0),
                            Color32::from_rgb(108, 117, 125),
                        );
                    }

                    ui.add_space(8.0);

                    // 2. 下段: FT後スペクトル プレビュー
                    let (spec_rect, _) = ui.allocate_exact_size(vec2(graph_w, half_h), egui::Sense::hover());
                    if let (Some(ppm), Some(spec)) = (&state.preview_ppm, &state.preview_spectrum) {
                        paint_spectrum_preview(ui, spec_rect, ppm, spec);
                    } else {
                        ui.painter().rect_filled(spec_rect, 3.0, Color32::from_rgb(248, 249, 250));
                        ui.painter().text(
                            spec_rect.center(),
                            Align2::CENTER_CENTER,
                            "No spectrum preview available",
                            FontId::proportional(13.0),
                            Color32::from_rgb(108, 117, 125),
                        );
                    }
                });

                ui.separator();

                // 右側: パラメータ設定パネル
                ui.vertical(|ui| {
                    ui.set_width(right_panel_w);
                    ui.heading("Processing Parameters");
                    ui.add_space(4.0);

                    // 1. 窓関数
                    ui.group(|ui| {
                        ui.set_width(right_panel_w - 10.0);
                        ui.label(RichText::new("Window Function").strong());

                        let is_none = matches!(state.settings.window, WindowFunction::None);
                        let is_em = matches!(state.settings.window, WindowFunction::Exponential { .. });
                        let is_gm = matches!(state.settings.window, WindowFunction::Gaussian { .. });

                        ui.horizontal(|ui| {
                            if ui.selectable_label(is_none, "None").clicked() {
                                state.settings.window = WindowFunction::None;
                            }
                            if ui.selectable_label(is_em, "Exp (EM)").clicked() {
                                state.settings.window = WindowFunction::Exponential { lb: 0.3 };
                            }
                            if ui.selectable_label(is_gm, "Gauss (GM)").clicked() {
                                state.settings.window = WindowFunction::Gaussian { g1: 0.0, g2: 2.0, g3: 0.0 };
                            }
                        });

                        ui.add_space(4.0);
                        match &mut state.settings.window {
                            WindowFunction::Exponential { lb } => {
                                ui.horizontal(|ui| {
                                    ui.label("LB (Hz)");
                                    ui.add(DragValue::new(lb).speed(0.05).range(0.01..=50.0));
                                });
                                ui.add(Slider::new(lb, 0.01..=20.0).show_value(false));
                            }
                            WindowFunction::Gaussian { g1, g2, g3 } => {
                                ui.horizontal(|ui| {
                                    ui.label("g1 (Hz)");
                                    ui.add(DragValue::new(g1).speed(0.05).range(0.0..=20.0));
                                });
                                ui.add(Slider::new(g1, 0.0..=10.0).show_value(false));

                                ui.horizontal(|ui| {
                                    ui.label("g2 (Hz)");
                                    ui.add(DragValue::new(g2).speed(0.05).range(0.01..=50.0));
                                });
                                ui.add(Slider::new(g2, 0.1..=20.0).show_value(false));

                                ui.horizontal(|ui| {
                                    ui.label("g3");
                                    ui.add(DragValue::new(g3).speed(0.01).range(0.0..=1.0));
                                });
                                ui.add(Slider::new(g3, 0.0..=1.0).show_value(false));
                            }
                            WindowFunction::None => {}
                        }
                    });

                    ui.add_space(4.0);

                    // 2. ゼロフィリング
                    ui.group(|ui| {
                        ui.set_width(right_panel_w - 10.0);
                        ui.label(RichText::new("Zero Filling").strong());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let factors = [1, 2, 4, 8, 16];
                            for f in factors {
                                let label = if f == 1 { "1x".to_string() } else { format!("{}x", f) };
                                if ui.selectable_label(state.settings.zf_factor == f, label).clicked() {
                                    state.settings.zf_factor = f;
                                }
                            }
                        });

                        let orig_pts = metadata.map(|m| m.points).unwrap_or(0);
                        let zf_pts = orig_pts * state.settings.zf_factor.max(1);
                        if orig_pts > 0 {
                            ui.label(
                                RichText::new(format!("Points: {} → {} pts", orig_pts, zf_pts))
                                    .size(11.0)
                                    .color(Color32::from_rgb(108, 117, 125)),
                            );
                        }
                    });

                    ui.add_space(4.0);

                    // 3. その他オプション
                    ui.group(|ui| {
                        ui.set_width(right_panel_w - 10.0);
                        ui.label(RichText::new("Options").strong());
                        ui.checkbox(&mut state.settings.remove_digital_filter, "Remove Digital Filter");
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // アクションボタン (Apply FT -> OK)
                    ui.horizontal(|ui| {
                        let ok_btn = Button::new(
                            RichText::new("OK").strong().size(13.0).color(Color32::WHITE),
                        )
                        .min_size(vec2(80.0, 26.0))
                        .fill(Color32::from_rgb(126, 34, 206)) // 紫系
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(107, 33, 168)))
                        .rounding(3.0);

                        if ui.add(ok_btn).clicked() {
                            applied_settings = Some(state.settings.clone());
                            state.open = false;
                        }

                        let cancel_btn = Button::new(RichText::new("Cancel").size(13.0))
                            .min_size(vec2(70.0, 26.0))
                            .fill(Color32::WHITE)
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)))
                            .rounding(3.0);

                        if ui.add(cancel_btn).clicked() {
                            state.open = false;
                        }
                    });
                });
            });
        });

    applied_settings
}
