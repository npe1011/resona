use egui::{
    epaint::PathShape, vec2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Ui,
};
use ndarray::Array1;
use crate::core::{compute_integral, IntegrationItem, MultiviewItem, PeakItem};

use super::transform::PlotTransform;

/// プロット描画設定
#[derive(Debug, Clone)]
pub struct PlotStyle {
    pub bg_color: Color32,
    pub axis_color: Color32,
    pub spectrum_color: Color32,
    pub spectrum_width: f32,
    pub peak_color: Color32,
    pub integral_color: Color32,
    pub multiview_color: Color32,
    pub threshold_color: Color32,
    pub rubberband_color: Color32,
    pub ppm_decimals: usize,
    pub integral_decimals: usize,
}

impl Default for PlotStyle {
    fn default() -> Self {
        Self {
            bg_color: Color32::WHITE,
            axis_color: Color32::from_rgb(80, 80, 80),
            spectrum_color: Color32::BLACK,
            spectrum_width: 1.2,
            peak_color: Color32::from_rgb(0, 100, 200),
            integral_color: Color32::from_rgb(200, 40, 40),
            multiview_color: Color32::from_rgb(0, 120, 255),
            threshold_color: Color32::from_rgb(220, 140, 0),
            rubberband_color: Color32::from_rgba_premultiplied(0, 120, 255, 60),
            ppm_decimals: 3,
            integral_decimals: 2,
        }
    }
}

/// スペクトルと関連要素を描画する
pub fn paint_spectrum(
    ui: &Ui,
    transform: &PlotTransform,
    ppm: &Array1<f64>,
    spectrum: &Array1<f64>,
    peaks: &[PeakItem],
    integrations: &[IntegrationItem],
    integration_scale: f64,
    integration_offset: f64,
    integration_ref_factor: f64,
    multiviews: &[MultiviewItem],
    multiview_ratio: f64,
    selected_multiview_id: Option<&str>,
    threshold: Option<f64>,
    style: &PlotStyle,
) {
    let painter = ui.painter_at(transform.screen_rect);
    let rect = transform.screen_rect;

    // 1. 背景
    painter.rect_filled(rect, 0.0, style.bg_color);

    // 2. グリッドと PPM 目盛り
    paint_ppm_axis(ui, transform, style);

    // 3. 閾値線 (設定されている場合)
    if let Some(thresh) = threshold {
        let sy = transform.y_to_screen_y(thresh);
        if sy >= rect.min.y && sy <= rect.max.y {
            painter.line_segment(
                [Pos2::new(rect.min.x, sy), Pos2::new(rect.max.x, sy)],
                Stroke::new(1.0_f32, style.threshold_color),
            );
        }
    }

    // 4. メインスペクトル曲線
    let n_pts = ppm.len().min(spectrum.len());
    if n_pts > 1 {
        let mut points: Vec<Pos2> = Vec::with_capacity(n_pts.min(4096));

        // 画面外の点は適度にクリッピング
        let span = (transform.ppm_max - transform.ppm_min).abs();
        let margin = span * 0.1;
        let p_min = transform.ppm_min.min(transform.ppm_max) - margin;
        let p_max = transform.ppm_min.max(transform.ppm_max) + margin;

        let mut prev_screen_x = -9999.0;
        let mut min_y_at_x = f32::MAX;
        let mut max_y_at_x = f32::MIN;

        for i in 0..n_pts {
            let p = ppm[i];
            if p < p_min || p > p_max {
                continue;
            }
            let pos = transform.data_to_screen(p, spectrum[i]);

            // ピクセル密度に応じた Min/Max リサンプリング (高速化)
            if (pos.x - prev_screen_x).abs() < 1.0 {
                min_y_at_x = min_y_at_x.min(pos.y);
                max_y_at_x = max_y_at_x.max(pos.y);
            } else {
                if prev_screen_x >= -1000.0 && min_y_at_x <= max_y_at_x {
                    points.push(Pos2::new(prev_screen_x, min_y_at_x));
                    if (max_y_at_x - min_y_at_x).abs() > 0.5 {
                        points.push(Pos2::new(prev_screen_x, max_y_at_x));
                    }
                }
                prev_screen_x = pos.x;
                min_y_at_x = pos.y;
                max_y_at_x = pos.y;
            }
        }
        if prev_screen_x >= -1000.0 && min_y_at_x <= max_y_at_x {
            points.push(Pos2::new(prev_screen_x, min_y_at_x));
            if (max_y_at_x - min_y_at_x).abs() > 0.5 {
                points.push(Pos2::new(prev_screen_x, max_y_at_x));
            }
        }

        if points.len() > 1 {
            let stroke = Stroke::new(style.spectrum_width, style.spectrum_color);
            painter.add(PathShape::line(points, stroke));
        }
    }

    // 5. ピークマーカー & PPM 数値ラベル
    let font_peak = FontId::new(10.0, FontFamily::Proportional);
    for peak in peaks {
        if peak.ppm < transform.ppm_min.min(transform.ppm_max)
            || peak.ppm > transform.ppm_min.max(transform.ppm_max)
        {
            continue;
        }
        let peak_pos = transform.data_to_screen(peak.ppm, peak.intensity);
        if rect.contains(peak_pos) {
            // ピーク頭頂部の小さな縦線
            painter.line_segment(
                [peak_pos, Pos2::new(peak_pos.x, peak_pos.y - 8.0)],
                Stroke::new(1.0_f32, style.peak_color),
            );

            // PPM テキスト (3桁)
            let text = format!("{:.1$}", peak.ppm, style.ppm_decimals);
            painter.text(
                Pos2::new(peak_pos.x, peak_pos.y - 12.0),
                egui::Align2::CENTER_BOTTOM,
                text,
                font_peak.clone(),
                style.peak_color,
            );
        }
    }

    // 6. 積分曲線 & 面積数値
    paint_integrations(
        ui,
        transform,
        ppm,
        spectrum,
        integrations,
        integration_scale,
        integration_offset,
        integration_ref_factor,
        style,
    );

    // 7. Multiview (インセットプロット)
    paint_multiviews(
        ui,
        transform,
        ppm,
        spectrum,
        multiviews,
        multiview_ratio,
        selected_multiview_id,
        style,
    );

    // 外枠
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, style.axis_color));
}

/// PPM 軸と目盛りの描画
fn paint_ppm_axis(ui: &Ui, transform: &PlotTransform, style: &PlotStyle) {
    let painter = ui.painter_at(transform.screen_rect);
    let rect = transform.screen_rect;
    let axis_y = rect.max.y - 20.0;

    // 軸線
    painter.line_segment(
        [Pos2::new(rect.min.x, axis_y), Pos2::new(rect.max.x, axis_y)],
        Stroke::new(1.0_f32, style.axis_color),
    );

    // PPM ラベル
    let font_axis = FontId::new(11.0, FontFamily::Proportional);
    painter.text(
        Pos2::new(rect.min.x + 30.0, axis_y + 12.0),
        egui::Align2::LEFT_CENTER,
        "ppm",
        font_axis.clone(),
        style.axis_color,
    );

    // 適切な目盛り間隔の算出
    let span = (transform.ppm_max - transform.ppm_min).abs();
    if span <= 1e-6 {
        return;
    }

    let approx_ticks = 8.0;
    let rough_step = span / approx_ticks;
    let exponent = (rough_step.log10().floor()) as i32;
    let base = 10.0_f64.powi(exponent);
    let fraction = rough_step / base;

    let step = if fraction < 1.5 {
        1.0 * base
    } else if fraction < 3.0 {
        2.0 * base
    } else if fraction < 7.0 {
        5.0 * base
    } else {
        10.0 * base
    };

    let start_ppm = (transform.ppm_min.min(transform.ppm_max) / step).floor() * step;
    let end_ppm = (transform.ppm_min.max(transform.ppm_max) / step).ceil() * step;

    let mut current_ppm = start_ppm;
    while current_ppm <= end_ppm + 1e-9 {
        let sx = transform.ppm_to_screen_x(current_ppm);
        if sx >= rect.min.x && sx <= rect.max.x {
            // 目盛り線
            painter.line_segment(
                [Pos2::new(sx, axis_y), Pos2::new(sx, axis_y + 5.0)],
                Stroke::new(1.0_f32, style.axis_color),
            );

            // 目盛り数値
            let dec = (-exponent).max(0) as usize;
            let val_str = format!("{:.1$}", current_ppm, dec);
            painter.text(
                Pos2::new(sx, axis_y + 8.0),
                egui::Align2::CENTER_TOP,
                val_str,
                font_axis.clone(),
                style.axis_color,
            );
        }
        current_ppm += step;
    }
}

/// 積分曲線の描画
fn paint_integrations(
    ui: &Ui,
    transform: &PlotTransform,
    ppm: &Array1<f64>,
    spectrum: &Array1<f64>,
    integrations: &[IntegrationItem],
    scale: f64,
    offset: f64,
    ref_factor: f64,
    style: &PlotStyle,
) {
    let painter = ui.painter_at(transform.screen_rect);
    let rect = transform.screen_rect;
    let font_intg = FontId::new(11.0, FontFamily::Proportional);

    for intg in integrations {
        let p_start = intg.start_ppm.max(intg.end_ppm);
        let p_end = intg.start_ppm.min(intg.end_ppm);

        let sx_start = transform.ppm_to_screen_x(p_start);
        let sx_end = transform.ppm_to_screen_x(p_end);

        // 積分区間の薄い縦帯
        if sx_end > rect.min.x && sx_start < rect.max.x {
            let band_rect = Rect::from_min_max(
                Pos2::new(sx_start.max(rect.min.x), rect.min.y),
                Pos2::new(sx_end.min(rect.max.x), rect.max.y - 20.0),
            );
            painter.rect_filled(
                band_rect,
                0.0,
                Color32::from_rgba_premultiplied(255, 230, 230, 40),
            );

            // 累積積分の計算と描画
            if let Some(res) = compute_integral(spectrum, ppm, intg, scale, ref_factor, offset) {
                if res.ppm.len() > 1 && res.ppm.len() == res.curve_y.len() {
                    let pts: Vec<Pos2> = (0..res.ppm.len())
                        .map(|i| transform.data_to_screen(res.ppm[i], res.curve_y[i]))
                        .filter(|p| rect.contains(*p))
                        .collect();

                    if pts.len() > 1 {
                        painter.add(PathShape::line(
                            pts,
                            Stroke::new(1.2_f32, style.integral_color),
                        ));
                    }
                }

                // 面積数値テキスト (区間の中央下部)
                let center_x = (sx_start + sx_end) * 0.5;
                let label_pos = Pos2::new(center_x, rect.max.y - 35.0);
                let val_text = format!("{:.1$}", res.normalized_value, style.integral_decimals);
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    val_text,
                    font_intg.clone(),
                    style.integral_color,
                );
            }
        }
    }
}

/// Multiview (インセットプロット) の描画
fn paint_multiviews(
    ui: &Ui,
    _main_transform: &PlotTransform,
    ppm: &Array1<f64>,
    spectrum: &Array1<f64>,
    multiviews: &[MultiviewItem],
    ratio: f64,
    selected_id: Option<&str>,
    style: &PlotStyle,
) {
    for mv in multiviews {
        let inset_rect = Rect::from_min_size(
            Pos2::new(mv.geometry.x, mv.geometry.y),
            vec2(mv.geometry.w, mv.geometry.h),
        );

        let is_selected = selected_id == Some(&mv.id);
        let painter = ui.painter_at(inset_rect);

        // 背景
        painter.rect_filled(inset_rect, 0.0, Color32::WHITE);

        // インセットプロット用の局所座標変換
        let src_min = mv.src_x_min.min(mv.src_x_max);
        let src_max = mv.src_x_min.max(mv.src_x_max);

        // 切り出し範囲の Y 最小・最大を取得
        let mut local_y_min = f64::MAX;
        let mut local_y_max = f64::MIN;
        for i in 0..ppm.len().min(spectrum.len()) {
            let p = ppm[i];
            if p >= src_min && p <= src_max {
                local_y_min = local_y_min.min(spectrum[i]);
                local_y_max = local_y_max.max(spectrum[i]);
            }
        }
        if local_y_min >= local_y_max {
            local_y_min = 0.0;
            local_y_max = 1.0;
        }

        let y_span = (local_y_max - local_y_min).max(1e-6);
        let eff_y_max = local_y_min + y_span / ratio.max(0.1);

        let inset_transform = PlotTransform::new(
            inset_rect,
            src_min,
            src_max,
            local_y_min - y_span * 0.05,
            eff_y_max,
        );

        // インセット内部のスペクトル描画
        let mut pts: Vec<Pos2> = Vec::new();
        for i in 0..ppm.len().min(spectrum.len()) {
            let p = ppm[i];
            if p >= src_min && p <= src_max {
                let pos = inset_transform.data_to_screen(p, spectrum[i]);
                if inset_rect.contains(pos) {
                    pts.push(pos);
                }
            }
        }
        if pts.len() > 1 {
            painter.add(PathShape::line(
                pts,
                Stroke::new(1.0_f32, style.multiview_color),
            ));
        }

        // 枠線
        let border_stroke = if is_selected {
            Stroke::new(2.5_f32, Color32::from_rgb(0, 100, 255))
        } else {
            Stroke::new(1.0_f32, Color32::from_gray(160))
        };
        painter.rect_stroke(inset_rect, 0.0, border_stroke);
    }
}
