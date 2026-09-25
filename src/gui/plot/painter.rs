use egui::{
    epaint::PathShape, vec2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Ui,
};
use ndarray::Array1;
use crate::core::{compute_integral, IntegrationItem, MultiviewItem, PeakItem};

use super::transform::PlotTransform;

/// プロット描画設定 (ezNMR / 科学NMR仕様の洗練されたライトテーマ)
#[derive(Debug, Clone)]
pub struct PlotStyle {
    pub bg_color: Color32,
    pub axis_color: Color32,
    pub spectrum_color: Color32,
    pub spectrum_width: f32,
    pub peak_mark_color: Color32,
    pub peak_label_color: Color32,
    pub peak_line_color: Color32,
    pub integral_color: Color32,
    pub integral_baseline_color: Color32,
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
            axis_color: Color32::from_rgb(30, 30, 30),
            spectrum_color: Color32::BLACK,
            spectrum_width: 1.2,
            peak_mark_color: Color32::from_rgb(220, 38, 38),   // 赤色縦マーク
            peak_label_color: Color32::from_rgb(20, 20, 20),   // 黒色PPMテキスト
            peak_line_color: Color32::from_rgb(140, 140, 140), // 引き出し線
            integral_color: Color32::from_rgb(225, 29, 72),    // 積分カーブ (赤)
            integral_baseline_color: Color32::from_rgb(59, 130, 246), // 局所ベースライン (青)
            multiview_color: Color32::from_rgb(147, 51, 234),  // Multiview (紫)
            threshold_color: Color32::from_rgb(217, 119, 6),   // 閾値線 (オレンジ)
            rubberband_color: Color32::from_rgba_premultiplied(13, 110, 253, 40),
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
    is_peak_mode: bool,
    is_threshold_submode: bool,
    is_integrate_edit_mode: bool,
    ref_drag_range: Option<(f64, f64)>,
    style: &PlotStyle,
) {
    let painter = ui.painter_at(transform.screen_rect);
    let rect = transform.screen_rect;

    // 1. 純白の背景
    painter.rect_filled(rect, 0.0, style.bg_color);

    // X軸の画面Y位置 (下部80pxはピークラベル用)
    let axis_y = transform.axis_y();

    // 2. Reference モードのドラッグ選択ハイライト
    if let Some((p1, p2)) = ref_drag_range {
        let sx1 = transform.ppm_to_screen_x(p1);
        let sx2 = transform.ppm_to_screen_x(p2);
        let min_x = sx1.min(sx2).clamp(rect.min.x, rect.max.x);
        let max_x = sx1.max(sx2).clamp(rect.min.x, rect.max.x);
        let sel_rect = Rect::from_min_max(Pos2::new(min_x, rect.min.y), Pos2::new(max_x, axis_y));
        painter.rect_filled(sel_rect, 0.0, Color32::from_rgba_unmultiplied(13, 110, 253, 35));
        painter.rect_stroke(sel_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(13, 110, 253)));
    }

    // 3. ピーク検出閾値線 (Peak モード時に正負の水平破線を描画)
    if is_peak_mode {
        if let Some(thresh) = threshold {
            if thresh > 0.0 {
                let (stroke_thresh, line_color) = if is_threshold_submode {
                    // Threshold サブモード時は太く強調表示
                    (
                        Stroke::new(2.2_f32, Color32::from_rgb(234, 88, 12)),
                        Color32::from_rgb(234, 88, 12),
                    )
                } else {
                    (
                        Stroke::new(1.2_f32, style.threshold_color),
                        style.threshold_color,
                    )
                };

                // 正の閾値
                let sy_pos = transform.y_to_screen_y(thresh);
                if sy_pos >= rect.min.y && sy_pos <= axis_y {
                    let mut cur_x = rect.min.x;
                    while cur_x < rect.max.x {
                        let next_x = (cur_x + 6.0).min(rect.max.x);
                        painter.line_segment([Pos2::new(cur_x, sy_pos), Pos2::new(next_x, sy_pos)], stroke_thresh);
                        cur_x += 10.0;
                    }

                    let label_str = if is_threshold_submode {
                        format!("+Threshold {:.1} (Drag to adjust)", thresh)
                    } else {
                        format!("+Threshold {:.1}", thresh)
                    };

                    painter.text(
                        Pos2::new(rect.max.x - 4.0, sy_pos - 3.0),
                        egui::Align2::RIGHT_BOTTOM,
                        label_str,
                        FontId::new(if is_threshold_submode { 10.5 } else { 9.5 }, FontFamily::Proportional),
                        line_color,
                    );
                }

                // 負の閾値
                let sy_neg = transform.y_to_screen_y(-thresh);
                if sy_neg >= rect.min.y && sy_neg <= axis_y {
                    let mut cur_x = rect.min.x;
                    while cur_x < rect.max.x {
                        let next_x = (cur_x + 6.0).min(rect.max.x);
                        painter.line_segment([Pos2::new(cur_x, sy_neg), Pos2::new(next_x, sy_neg)], stroke_thresh);
                        cur_x += 10.0;
                    }
                }
            }
        }
    }

    // 3. メインスペクトル曲線 (黒色, 1.2px)
    let n_pts = ppm.len().min(spectrum.len());
    if n_pts > 1 {
        let mut points: Vec<Pos2> = Vec::with_capacity(n_pts.min(4096));

        let span = (transform.ppm_max - transform.ppm_min).abs();
        let margin = span * 0.1;
        let p_min = transform.ppm_min.min(transform.ppm_max) - margin;
        let p_max = transform.ppm_min.max(transform.ppm_max) + margin;

        let mut prev_screen_x = -9999.0_f32;
        let mut min_y_at_x = f32::MAX;
        let mut max_y_at_x = f32::MIN;

        for i in 0..n_pts {
            let p = ppm[i];
            if p < p_min || p > p_max {
                continue;
            }
            let pos = transform.data_to_screen(p, spectrum[i]);

            // ピクセル密度に応じた Min/Max リサンプリング
            if (pos.x - prev_screen_x).abs() < 1.0 {
                min_y_at_x = min_y_at_x.min(pos.y);
                max_y_at_x = max_y_at_x.max(pos.y);
            } else {
                if prev_screen_x >= -1000.0 && min_y_at_x <= max_y_at_x {
                    points.push(Pos2::new(prev_screen_x, min_y_at_x.min(axis_y)));
                    if (max_y_at_x - min_y_at_x).abs() > 0.5 {
                        points.push(Pos2::new(prev_screen_x, max_y_at_x.min(axis_y)));
                    }
                }
                prev_screen_x = pos.x;
                min_y_at_x = pos.y;
                max_y_at_x = pos.y;
            }
        }
        if prev_screen_x >= -1000.0 && min_y_at_x <= max_y_at_x {
            points.push(Pos2::new(prev_screen_x, min_y_at_x.min(axis_y)));
            if (max_y_at_x - min_y_at_x).abs() > 0.5 {
                points.push(Pos2::new(prev_screen_x, max_y_at_x.min(axis_y)));
            }
        }

        if points.len() > 1 {
            let stroke = Stroke::new(style.spectrum_width, style.spectrum_color);
            painter.add(PathShape::line(points, stroke));
        }
    }

    // 4. 積分曲線 & 浮遊数値ラベル (ezNMR仕様: 局所ベースライン破線 + 赤色S字カーブ + カーブ真上ラベル)
    paint_integrations(
        ui,
        transform,
        ppm,
        spectrum,
        integrations,
        integration_scale,
        integration_offset,
        integration_ref_factor,
        axis_y,
        is_integrate_edit_mode,
        style,
    );

    // 5. PPM 軸と目盛り (X軸)
    paint_ppm_axis(ui, transform, axis_y, style);

    // 6. ピーク表示 (ezNMR仕様: 頭頂部に赤色縦マーク、X軸下にアンチコリジョン引き出し線 + 縦向きPPM値)
    paint_peaks_eznmr(ui, transform, peaks, axis_y, rect.max.y, is_peak_mode, style);

    // 7. Multiview (インセット拡大窓)
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
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(210)));
}

/// PPM 軸と目盛りの描画
fn paint_ppm_axis(ui: &Ui, transform: &PlotTransform, axis_y: f32, style: &PlotStyle) {
    let painter = ui.painter_at(transform.screen_rect);
    let rect = transform.screen_rect;

    // X軸線
    painter.line_segment(
        [Pos2::new(rect.min.x, axis_y), Pos2::new(rect.max.x, axis_y)],
        Stroke::new(1.0_f32, style.axis_color),
    );

    // PPM 単位ラベル
    let font_axis = FontId::new(11.0, FontFamily::Proportional);
    painter.text(
        Pos2::new(rect.max.x - 28.0, axis_y - 8.0),
        egui::Align2::RIGHT_BOTTOM,
        "ppm",
        font_axis.clone(),
        style.axis_color,
    );

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
            // 上向き目盛り線 (主目盛り)
            painter.line_segment(
                [Pos2::new(sx, axis_y), Pos2::new(sx, axis_y - 5.0)],
                Stroke::new(1.0_f32, style.axis_color),
            );

            // 目盛り数値 (軸の上部に表示)
            let dec = (-exponent).max(0) as usize;
            let val_str = format!("{:.1$}", current_ppm, dec);
            painter.text(
                Pos2::new(sx, axis_y - 7.0),
                egui::Align2::CENTER_BOTTOM,
                val_str,
                font_axis.clone(),
                style.axis_color,
            );
        }
        current_ppm += step;
    }
}

/// ezNMR仕様のピークピッキング描画
/// - スペクトル頭頂部: 赤色の明瞭な縦マーク (|) (Peak pick モード時のみ表示)
/// - 引き出し線: ピーク頂上付近からX軸を越えて折れ曲がり、下部のPPM値へスムーズに接続
/// - X軸下部領域: アンチコリジョン左右反発アルゴリズム + 縦向き太字PPMラベル
fn paint_peaks_eznmr(
    ui: &Ui,
    transform: &PlotTransform,
    peaks: &[PeakItem],
    axis_y: f32,
    bottom_y: f32,
    is_peak_mode: bool,
    style: &PlotStyle,
) {
    if peaks.is_empty() {
        return;
    }

    let painter = ui.painter_at(transform.screen_rect);
    let rect = transform.screen_rect;

    let p_min = transform.ppm_min.min(transform.ppm_max);
    let p_max = transform.ppm_min.max(transform.ppm_max);

    // 1. スペクトル頭頂部の赤縦マーク描画 (Peak pick モード時のみ表示)
    if is_peak_mode {
        for peak in peaks {
            if peak.ppm < p_min || peak.ppm > p_max {
                continue;
            }
            let peak_pos = transform.data_to_screen(peak.ppm, peak.intensity);
            if peak_pos.x >= rect.min.x && peak_pos.x <= rect.max.x && peak_pos.y <= axis_y {
                painter.line_segment(
                    [
                        Pos2::new(peak_pos.x, peak_pos.y - 6.0),
                        Pos2::new(peak_pos.x, peak_pos.y + 6.0),
                    ],
                    Stroke::new(2.0_f32, style.peak_mark_color),
                );
            }
        }
    }

    // 2. 画面内に見えるピークを PPM 降順（画面左から右）でソート
    let mut visible_peaks: Vec<&PeakItem> = peaks
        .iter()
        .filter(|p| p.ppm >= p_min && p.ppm <= p_max)
        .collect();
    visible_peaks.sort_by(|a, b| b.ppm.partial_cmp(&a.ppm).unwrap_or(std::cmp::Ordering::Equal));

    if visible_peaks.is_empty() {
        return;
    }

    // アンチコリジョン用 X 座標リスト (画面ピクセル単位)
    let mut text_x: Vec<f32> = visible_peaks
        .iter()
        .map(|p| transform.ppm_to_screen_x(p.ppm))
        .collect();

    // 最小間隔 (ピクセル: フォントサイズ拡大に合わせて余裕を持たせる)
    let min_gap = 16.0_f32;

    // 対称緩和 (Symmetric relaxation) アンチコリジョン
    for _ in 0..300 {
        let mut moved = false;
        for i in 1..text_x.len() {
            let diff = text_x[i] - text_x[i - 1]; // PPM降順なので画面Xは昇順
            if diff < min_gap {
                let overlap = min_gap - diff;
                text_x[i - 1] -= overlap * 0.5;
                text_x[i] += overlap * 0.5;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    // 引き出し線のYレベル
    let y_elbow = axis_y + 12.0;
    let y_text_start = axis_y + 24.0;

    // より大きく、くっきりと読みやすいフォント
    let font_peak = FontId::new(10.5, FontFamily::Proportional);
    let peak_text_color = Color32::from_rgb(15, 23, 42); // 濃い黒

    for (i, peak) in visible_peaks.iter().enumerate() {
        let px = transform.ppm_to_screen_x(peak.ppm);
        let tx = text_x[i].clamp(rect.min.x + 4.0, rect.max.x - 4.0);

        // ピーク頭頂部の画面位置 (X軸を8pxほど跨いでスペクトルのふもとから引き出す)
        let peak_pos = transform.data_to_screen(peak.ppm, peak.intensity);
        let y_start = (axis_y - 8.0).max(peak_pos.y + 2.0);

        // 引き出し線 (X軸上8px付近 -> X軸通過 -> 折れ曲がり斜め -> 縦)
        let stroke_line = Stroke::new(0.85_f32, style.peak_line_color);
        painter.line_segment([Pos2::new(px, y_start), Pos2::new(px, y_elbow)], stroke_line);
        painter.line_segment([Pos2::new(px, y_elbow), Pos2::new(tx, y_text_start - 3.0)], stroke_line);
        painter.line_segment([Pos2::new(tx, y_text_start - 3.0), Pos2::new(tx, y_text_start)], stroke_line);

        // PPM 数値ラベル (ezNMR仕様: 横書き文字列を時計回り90度回転させて描画)
        // 小数点(.)やマイナス記号(-)のカーニングが完璧に保たれ、出版水準の美しさになる
        let val_str = format!("{:.1$}", peak.ppm, style.ppm_decimals);
        let galley = painter.layout_no_wrap(val_str, font_peak.clone(), peak_text_color);
        let text_h = galley.size().y;
        let pos = Pos2::new(tx + text_h * 0.5, y_text_start + 3.0);
        if pos.y < bottom_y {
            let ts = egui::epaint::TextShape::new(pos, galley, peak_text_color)
                .with_angle(std::f32::consts::FRAC_PI_2);
            painter.add(ts);
        }
    }
}

/// ezNMR仕様の積分描画
/// - 局所ベースライン: 青い破線
/// - 積分曲線: 赤色実線 (#ef4444, 2.0px)
/// - 積分数値ラベル: 積分カーブの真上 (中央よりやや左) に赤色太字で配置
fn paint_integrations(
    ui: &Ui,
    transform: &PlotTransform,
    ppm: &Array1<f64>,
    spectrum: &Array1<f64>,
    integrations: &[IntegrationItem],
    scale: f64,
    offset: f64,
    ref_factor: f64,
    axis_y: f32,
    is_edit_mode: bool,
    style: &PlotStyle,
) {
    let painter = ui.painter_at(transform.screen_rect);
    let rect = transform.screen_rect;
    let font_intg = FontId::new(11.5, FontFamily::Proportional);

    for intg in integrations {
        let p_start = intg.start_ppm.max(intg.end_ppm);
        let p_end = intg.start_ppm.min(intg.end_ppm);

        let sx_start = transform.ppm_to_screen_x(p_start);
        let sx_end = transform.ppm_to_screen_x(p_end);

        if sx_end > rect.min.x && sx_start < rect.max.x {
            // 1. 局所ベースライン (青色破線)
            let bl_start = transform.data_to_screen(intg.start_ppm, intg.y_start);
            let bl_end = transform.data_to_screen(intg.end_ppm, intg.y_end);
            painter.line_segment(
                [bl_start, bl_end],
                Stroke::new(1.0_f32, style.integral_baseline_color),
            );

            // Edit モード時の端点ハンドル描画 (ezNMR仕様: 青色丸ハンドル + 白色枠線)
            if is_edit_mode {
                painter.circle_filled(bl_start, 4.5, Color32::from_rgb(37, 99, 235));
                painter.circle_stroke(bl_start, 4.5, Stroke::new(1.5_f32, Color32::WHITE));

                painter.circle_filled(bl_end, 4.5, Color32::from_rgb(37, 99, 235));
                painter.circle_stroke(bl_end, 4.5, Stroke::new(1.5_f32, Color32::WHITE));
            }

            // 2. 累積積分カーブ (赤色, 2.0px)
            if let Some(res) = compute_integral(spectrum, ppm, intg, scale, ref_factor, offset) {
                if res.ppm.len() > 1 && res.ppm.len() == res.curve_y.len() {
                    let mut pts: Vec<Pos2> = Vec::with_capacity(res.ppm.len());
                    let mut min_screen_y = f32::MAX; // 画面上の最小Y = カーブの最も高い位置

                    for i in 0..res.ppm.len() {
                        let pos = transform.data_to_screen(res.ppm[i], res.curve_y[i]);
                        let clamped_pos = Pos2::new(pos.x, pos.y.min(axis_y));
                        pts.push(clamped_pos);
                        min_screen_y = min_screen_y.min(pos.y);
                    }

                    if pts.len() > 1 {
                        painter.add(PathShape::line(
                            pts,
                            Stroke::new(2.0_f32, style.integral_color),
                        ));
                    }

                    // 3. 積分値テキスト (ezNMR仕様: カーブの真上、中央よりやや左寄り)
                    let mid_x = (sx_start + sx_end) * 0.5;
                    let text_x = mid_x - (sx_end - sx_start).abs() * 0.15; // やや左
                    let text_y = (min_screen_y - 12.0).max(rect.min.y + 10.0);

                    let val_text = format!("{:.1$}", res.normalized_value, style.integral_decimals);

                    // 読みやすさのための背景白抜き
                    let text_pos = Pos2::new(text_x, text_y);
                    painter.text(
                        text_pos,
                        egui::Align2::CENTER_CENTER,
                        val_text,
                        font_intg.clone(),
                        style.integral_color,
                    );
                }
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

        // 背景 (白)
        painter.rect_filled(inset_rect, 0.0, Color32::WHITE);

        // インセットプロット用の局所座標変換
        let src_min = mv.src_x_min.min(mv.src_x_max);
        let src_max = mv.src_x_min.max(mv.src_x_max);

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

        // 枠線 (通常: グレー、選択中: ブルー)
        let border_stroke = if is_selected {
            Stroke::new(2.0_f32, Color32::from_rgb(13, 110, 253))
        } else {
            Stroke::new(1.0_f32, Color32::from_gray(180))
        };
        painter.rect_stroke(inset_rect, 0.0, border_stroke);
    }
}
