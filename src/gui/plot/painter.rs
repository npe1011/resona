use std::collections::HashSet;
use egui::{
    epaint::PathShape, vec2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Ui,
};
use ndarray::Array1;
use crate::core::{calc_ppm_ticks, compute_integral, IntegrationItem, MultiviewItem, PeakItem};

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
    pub auto_ticks: bool,
    pub tick_major: f64,
    pub tick_minor: usize,
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
            integral_decimals: 3,
            auto_ticks: false,
            tick_major: 1.0,
            tick_minor: 10,
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
    _multiview_ratio: f64,
    selected_multiview_ids: &HashSet<String>,
    hovered_multiview_id: Option<&str>,
    is_multiview_edit_mode: bool,
    threshold: Option<f64>,
    is_peak_mode: bool,
    is_threshold_submode: bool,
    is_integrate_edit_mode: bool,
    ref_drag_range: Option<(f64, f64)>,
    ref_marker: Option<f64>,
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

    // 2.2 Reference 基準ピークの表示 (赤い縦線 + 画面上部の化学シフト値)
    if let Some(ref_ppm) = ref_marker {
        let sx = transform.ppm_to_screen_x(ref_ppm);
        if sx >= rect.min.x && sx <= rect.max.x {
            // 赤い縦線 (1.2px)
            let stroke_ref = Stroke::new(1.2_f32, Color32::from_rgb(220, 38, 38));
            painter.line_segment([Pos2::new(sx, rect.min.y), Pos2::new(sx, axis_y)], stroke_ref);

            // 画面上部の化学シフト値バッジ (白背景 + 赤色枠線 + 赤文字)
            let label_str = format!("{:.3} ppm", ref_ppm);
            let font_ref = FontId::new(10.5, FontFamily::Proportional);
            let text_color = Color32::from_rgb(220, 38, 38);
            let galley = painter.layout_no_wrap(label_str, font_ref, text_color);
            let badge_size = galley.size() + vec2(8.0, 4.0);
            let badge_center = Pos2::new(
                sx.clamp(rect.min.x + badge_size.x * 0.5 + 4.0, rect.max.x - badge_size.x * 0.5 - 4.0),
                rect.min.y + 12.0,
            );
            let badge_rect = Rect::from_center_size(badge_center, badge_size);

            painter.rect_filled(badge_rect, 3.0, Color32::from_rgba_premultiplied(255, 255, 255, 235));
            painter.rect_stroke(badge_rect, 3.0, Stroke::new(1.0_f32, text_color));
            painter.galley(
                Pos2::new(badge_rect.min.x + 4.0, badge_rect.min.y + 2.0),
                galley,
                text_color,
            );
        }
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
        ppm,
        spectrum,
        peaks,
        integrations,
        integration_ref_factor,
        multiviews,
        selected_multiview_ids,
        hovered_multiview_id,
        is_multiview_edit_mode,
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

    let font_axis = FontId::new(11.0, FontFamily::Proportional);

    let span = (transform.ppm_max - transform.ppm_min).abs();
    if span <= 1e-6 {
        return;
    }

    let (step, dec) = calc_ppm_ticks(span, style.auto_ticks, style.tick_major);

    let minor_n = style.tick_minor.max(1);
    let minor_step = step / (minor_n as f64);

    let start_ppm = (transform.ppm_min.min(transform.ppm_max) / step).floor() * step;
    let end_ppm = (transform.ppm_min.max(transform.ppm_max) / step).ceil() * step;

    let mut current_ppm = start_ppm;
    while current_ppm <= end_ppm + 1e-9 {
        let sx = transform.ppm_to_screen_x(current_ppm);
        if sx >= rect.min.x && sx <= rect.max.x {
            // 上向き目盛り線 (主目盛り: 5.0px)
            painter.line_segment(
                [Pos2::new(sx, axis_y), Pos2::new(sx, axis_y - 5.0)],
                Stroke::new(1.0_f32, style.axis_color),
            );

            // 目盛り数値 (軸の上部に表示)
            let val_str = format!("{:.1$}", current_ppm, dec);
            painter.text(
                Pos2::new(sx, axis_y - 7.0),
                egui::Align2::CENTER_BOTTOM,
                val_str,
                font_axis.clone(),
                style.axis_color,
            );
        }

        // サブ目盛り (StepをN分割した目盛りマーク: 2.5px、数字なし)
        if minor_n > 1 {
            for m in 1..minor_n {
                let sub_ppm = current_ppm + (m as f64) * minor_step;
                let sub_sx = transform.ppm_to_screen_x(sub_ppm);
                if sub_sx >= rect.min.x && sub_sx <= rect.max.x {
                    painter.line_segment(
                        [Pos2::new(sub_sx, axis_y), Pos2::new(sub_sx, axis_y - 2.5)],
                        Stroke::new(0.8_f32, style.axis_color),
                    );
                }
            }
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

    // 最小間隔 (ピクセル: 16.0 -> 11.0 に詰めて数値同士をすっきり整列)
    let min_gap = 11.0_f32;

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
        let tx = text_x[i];

        // 枠外にはみ出るものは clamp して端に固めるのではなく omit (スキップ)
        if tx < rect.min.x + 2.0 || tx > rect.max.x - 2.0 || px < rect.min.x || px > rect.max.x {
            continue;
        }

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
    _axis_y: f32,
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
                        let clamped_pos = Pos2::new(pos.x, pos.y.clamp(rect.min.y, rect.max.y));
                        pts.push(clamped_pos);
                        min_screen_y = min_screen_y.min(pos.y);
                    }

                    if pts.len() > 1 {
                        painter.add(PathShape::line(
                            pts,
                            Stroke::new(2.0_f32, style.integral_color),
                        ));
                    }

                    // 3. 積分値テキスト (化学シフト値と同様に時計回り90度回転の縦書き配置)
                    let mid_x = (sx_start + sx_end) * 0.5;
                    let val_text = format!("{:.1$}", res.normalized_value, style.integral_decimals);
                    let galley = painter.layout_no_wrap(val_text, font_intg.clone(), style.integral_color);
                    let text_len = galley.size().x;
                    let text_h = galley.size().y;
                    let start_y = (min_screen_y - 4.0 - text_len).max(rect.min.y + 4.0);
                    let text_pos = Pos2::new(mid_x + text_h * 0.5, start_y);
                    let ts = egui::epaint::TextShape::new(text_pos, galley, style.integral_color)
                        .with_angle(std::f32::consts::FRAC_PI_2);
                    painter.add(ts);
                }
            }
        }
    }
}

/// Multiview (インセットプロット) の描画 (ezNMR仕様準拠)
fn paint_multiviews(
    ui: &Ui,
    ppm: &Array1<f64>,
    spectrum: &Array1<f64>,
    peaks: &[PeakItem],
    integrations: &[IntegrationItem],
    integration_ref_factor: f64,
    multiviews: &[MultiviewItem],
    selected_ids: &HashSet<String>,
    hovered_id: Option<&str>,
    is_edit_mode: bool,
    style: &PlotStyle,
) {
    for mv in multiviews {
        let inset_rect = Rect::from_min_size(
            Pos2::new(mv.geometry.x, mv.geometry.y),
            vec2(mv.geometry.w, mv.geometry.h),
        );

        if inset_rect.width() < 30.0 || inset_rect.height() < 30.0 {
            continue;
        }

        let is_selected = selected_ids.contains(&mv.id);
        let is_hovered = is_edit_mode && hovered_id == Some(&mv.id);
        let painter = ui.painter_at(inset_rect.expand(2.0));

        // 1. 薄い影 (ドロップシャドウ) と 白背景
        let shadow_rect = inset_rect.translate(vec2(1.5, 1.5));
        painter.rect_filled(shadow_rect, 1.0, Color32::from_rgba_premultiplied(0, 0, 0, 20));
        painter.rect_filled(inset_rect, 0.0, Color32::WHITE);

        // 2. プロット内部領域と X 軸領域の分離
        // X 軸の高さを 18px とし、スペクトルベースラインと X 軸の間に少しだけ上品な隙間 (5px) を空ける
        let axis_h = 18.0_f32;
        let axis_y = inset_rect.max.y - axis_h;
        let plot_rect = Rect::from_min_max(
            inset_rect.min,
            Pos2::new(inset_rect.max.x, (axis_y - 5.0).max(inset_rect.min.y + 10.0)),
        );

        // 3. 拡大対象 PPM 範囲の計算
        let src_min = mv.src_x_min.min(mv.src_x_max);
        let src_max = mv.src_x_min.max(mv.src_x_max);

        let mut local_y_min = f64::INFINITY;
        let mut local_y_max = f64::NEG_INFINITY;
        for i in 0..ppm.len().min(spectrum.len()) {
            let p = ppm[i];
            if p >= src_min && p <= src_max {
                let v = spectrum[i];
                if v < local_y_min { local_y_min = v; }
                if v > local_y_max { local_y_max = v; }
            }
        }
        if local_y_min >= local_y_max {
            local_y_min = 0.0;
            local_y_max = 1.0;
        }

        // スペクトルと X 軸をぴったり近づけるため、下部は local_y_min.min(0.0) を基準にし、
        // 下部余白はわずか 2% (ノイズがX軸直上に着地する程度) に抑える。
        // 上部はピーク引き出し線と縦書き化学シフト値のスペースとして 35% の余裕を確保。
        let y_min = mv.src_y_min.unwrap_or_else(|| local_y_min.min(0.0));
        let y_max = mv.src_y_max.unwrap_or(local_y_max);
        let h = (y_max - y_min).max(1e-6);
        let y_min_adj = y_min - 0.02 * h;
        let y_max_adj = y_max + 0.40 * h;

        // インセット専用の座標変換 (bottom_margin を 0.0 にすることでプロット下端を X 軸に一致させる)
        let inset_transform = PlotTransform::new(
            plot_rect,
            src_min,
            src_max,
            y_min_adj,
            y_max_adj,
        ).with_bottom_margin(0.0);

        // プロット領域専用のクリッピング painter (はみ出し防止)
        let plot_painter = painter.with_clip_rect(plot_rect);

        // 4. スペクトル曲線
        let mut pts: Vec<Pos2> = Vec::new();
        for i in 0..ppm.len().min(spectrum.len()) {
            let p = ppm[i];
            if p >= src_min && p <= src_max {
                let pos = inset_transform.data_to_screen(p, spectrum[i]);
                pts.push(pos);
            }
        }
        if pts.len() > 1 {
            plot_painter.add(PathShape::line(
                pts,
                Stroke::new(1.5_f32, style.multiview_color),
            ));
        }

        // 5. 積分曲線 & プロトン数テキスト (ezNMR仕様)
        for integ in integrations {
            let i_min = integ.min_ppm();
            let i_max = integ.max_ppm();
            if i_max < src_min || i_min > src_max {
                continue;
            }

            if let Some(res) = compute_integral(spectrum, ppm, integ, 1.0, integration_ref_factor, 0.0) {
                if res.ppm.len() > 1 && res.total_area.abs() > 1e-12 {
                    // 局所スケーリング: インセットの 25% 〜 70% に収める
                    let mut intg_pts: Vec<Pos2> = Vec::new();
                    for (&p, &cy) in res.ppm.iter().zip(res.curve_y.iter()) {
                        if p >= src_min && p <= src_max {
                            let bl = integ.baseline_y_at(p);
                            let cum = cy - bl;
                            let norm_y = (cum / res.total_area).clamp(0.0, 1.0);
                            let target_data_y = y_min_adj + 0.20 * h + norm_y * (0.45 * h);
                            intg_pts.push(inset_transform.data_to_screen(p, target_data_y));
                        }
                    }
                    if intg_pts.len() > 1 {
                        plot_painter.add(PathShape::line(
                            intg_pts,
                            Stroke::new(1.5_f32, style.integral_color),
                        ));

                        // 積分値テキスト (時計回り90度回転の縦書き配置)
                        let mid_p = (i_min.max(src_min) + i_max.min(src_max)) * 0.5;
                        let text_x = inset_transform.ppm_to_screen_x(mid_p);
                        let top_pos = inset_transform.data_to_screen(mid_p, y_min_adj + 0.68 * h);
                        let val_text = format!("{:.1$}", res.normalized_value, style.integral_decimals);
                        let font_intg = egui::FontId::proportional(9.5);
                        let galley = plot_painter.layout_no_wrap(val_text, font_intg, style.integral_color);
                        let text_len = galley.size().x;
                        let text_h = galley.size().y;
                        let start_y = (top_pos.y - text_len).max(plot_rect.min.y + 4.0);
                        let text_pos = Pos2::new(text_x + text_h * 0.5, start_y);
                        let ts = egui::epaint::TextShape::new(text_pos, galley, style.integral_color)
                            .with_angle(std::f32::consts::FRAC_PI_2);
                        plot_painter.add(ts);
                    }
                }
            }
        }

        // 6. ピーク引き出し線 & 化学シフト値 (潰れ防止: 時計回り90度回転の縦書き配置)
        let sub_peaks: Vec<&PeakItem> = peaks
            .iter()
            .filter(|pk| pk.ppm >= src_min && pk.ppm <= src_max)
            .collect();

        if !sub_peaks.is_empty() {
            let mut sorted_peaks = sub_peaks.clone();
            if sorted_peaks.len() > 20 {
                sorted_peaks.sort_by(|a, b| b.intensity.partial_cmp(&a.intensity).unwrap_or(std::cmp::Ordering::Equal));
                sorted_peaks.truncate(20);
            }
            // PPM 降順 (左から右) にソートして線の交差を防止
            sorted_peaks.sort_by(|a, b| b.ppm.partial_cmp(&a.ppm).unwrap_or(std::cmp::Ordering::Equal));

            // 画面 X 座標ベースの物理リラクゼーション (縦書きテキスト幅約 10px に合わせた min_gap = 9.5px)
            let mut screen_x_list: Vec<f32> = sorted_peaks
                .iter()
                .map(|p| inset_transform.ppm_to_screen_x(p.ppm))
                .collect();

            let min_gap_px = 9.5_f32; // 12.0 -> 9.5 に詰める
            for _ in 0..100 {
                let mut moved = false;
                for i in 1..screen_x_list.len() {
                    let diff = screen_x_list[i] - screen_x_list[i - 1];
                    if diff < min_gap_px {
                        let overlap = min_gap_px - diff;
                        screen_x_list[i - 1] -= overlap * 0.5;
                        screen_x_list[i] += overlap * 0.5;
                        moved = true;
                    }
                }
                if !moved {
                    break;
                }
            }

            // テキスト上端と折れ曲がり Y 座標 (画面上25%以内で引出線を完結させる)
            let font_peak = FontId::new(9.0, FontFamily::Proportional);
            let peak_color = Color32::from_rgb(20, 20, 20);
            let plot_h = axis_y - plot_rect.min.y;
            let text_start_y = plot_rect.min.y + 4.0;
            let text_len = 24.0_f32;
            let text_bottom_y = text_start_y + text_len;
            let elbow_y = text_bottom_y + 3.0;
            let max_lead_y = (plot_rect.min.y + plot_h * 0.25).max(elbow_y + 4.0);

            for (i, pk) in sorted_peaks.iter().enumerate() {
                let px = inset_transform.ppm_to_screen_x(pk.ppm);
                let tx = screen_x_list[i];

                // 枠外にはみ出るものは clamp せずに omit
                if tx < plot_rect.min.x + 2.0 || tx > plot_rect.max.x - 2.0 || px < plot_rect.min.x || px > plot_rect.max.x {
                    continue;
                }

                let peak_screen = inset_transform.data_to_screen(pk.ppm, pk.intensity);
                let clearance = 8.0_f32;
                // ピークが上部25%より上にある場合はピーク直前で止め、低いピークの場合は上部25%位置からスタート
                let line_start_y = (peak_screen.y - clearance).min(max_lead_y);

                // 引き出し線 (スペクトル上方 -> 折れ曲がりY -> テキスト位置)
                if line_start_y > elbow_y + 1.0 {
                    let stroke = Stroke::new(0.85_f32, Color32::from_gray(100));
                    plot_painter.line_segment([Pos2::new(px, line_start_y), Pos2::new(px, elbow_y)], stroke);
                    plot_painter.line_segment([Pos2::new(px, elbow_y), Pos2::new(tx, text_bottom_y + 1.5)], stroke);
                    plot_painter.line_segment([Pos2::new(tx, text_bottom_y + 1.5), Pos2::new(tx, text_bottom_y)], stroke);
                }

                // 時計回り90度回転の縦書き化学シフト値ラベル (style.ppm_decimals を反映)
                let shift_str = format!("{:.1$}", pk.ppm, style.ppm_decimals);
                let galley = plot_painter.layout_no_wrap(shift_str, font_peak.clone(), peak_color);
                let text_h = galley.size().y;
                let text_pos = Pos2::new(tx + text_h * 0.5, text_start_y);
                let ts = egui::epaint::TextShape::new(text_pos, galley, peak_color)
                    .with_angle(std::f32::consts::FRAC_PI_2);
                plot_painter.add(ts);
            }
        }

        // 7. X 軸目盛りと PPM 数値ラベル (プロット直下にピタッと配置)
        painter.line_segment(
            [Pos2::new(inset_rect.min.x, axis_y), Pos2::new(inset_rect.max.x, axis_y)],
            Stroke::new(1.0_f32, Color32::BLACK),
        );

        let ppm_span = src_max - src_min;
        let inset_w = inset_rect.width();
        // ラベル1個あたり約 45〜55px の間隔を確保して重なりを防止
        let target_ticks = (inset_w / 50.0).clamp(3.0, 7.0) as f64;
        let rough_step = (ppm_span / target_ticks).max(1e-6);

        // 1, 2, 5 系列のきりの良いステップに丸める
        let exponent = rough_step.log10().floor();
        let frac = rough_step / 10.0_f64.powf(exponent);
        let nice_frac = if frac <= 1.5 {
            1.0
        } else if frac <= 3.0 {
            2.0
        } else if frac <= 7.0 {
            5.0
        } else {
            10.0
        };
        let step = nice_frac * 10.0_f64.powf(exponent);

        let start_tick = (src_min / step).ceil() as i64;
        let end_tick = (src_max / step).floor() as i64;

        // 小数点桁数の自動決定
        let decimals = if step < 0.0099 {
            3
        } else if step < 0.099 {
            2
        } else if step < 0.99 {
            1
        } else {
            0
        };

        let minor_step = step / 10.0;
        let start_minor = (src_min / minor_step).ceil() as i64;
        let end_minor = (src_max / minor_step).floor() as i64;

        // 7a. サブ目盛り (10分割、数値ラベルなし、短め)
        for m_idx in start_minor..=end_minor {
            if m_idx % 10 == 0 {
                continue; // メイン目盛りと一致する位置はスキップ
            }
            let m_ppm = m_idx as f64 * minor_step;
            let m_x = inset_transform.ppm_to_screen_x(m_ppm);
            if m_x >= inset_rect.min.x && m_x <= inset_rect.max.x {
                painter.line_segment(
                    [Pos2::new(m_x, axis_y), Pos2::new(m_x, axis_y + 1.8)],
                    Stroke::new(0.75_f32, Color32::from_gray(100)),
                );
            }
        }

        // 7b. メイン目盛り (長め、数値ラベルあり)
        for t_idx in start_tick..=end_tick {
            let tick_ppm = t_idx as f64 * step;
            let tick_x = inset_transform.ppm_to_screen_x(tick_ppm);
            if tick_x >= inset_rect.min.x && tick_x <= inset_rect.max.x {
                // 目盛り線
                painter.line_segment(
                    [Pos2::new(tick_x, axis_y), Pos2::new(tick_x, axis_y + 3.5)],
                    Stroke::new(1.0_f32, Color32::BLACK),
                );
                // 枠線の左右 10px 以内は文字がはみ出さないようスキップ
                if tick_x >= inset_rect.min.x + 10.0 && tick_x <= inset_rect.max.x - 10.0 {
                    let label = format!("{:.1$}", tick_ppm, decimals);
                    painter.text(
                        Pos2::new(tick_x, axis_y + 10.0),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(8.5),
                        Color32::from_gray(30),
                    );
                }
            }
        }

        // 8. 枠線 (通常時: 薄いグレー枠 #94a3b8 / gray 160、選択/ホバー時: ブルー太枠)
        let border_stroke = if is_selected || is_hovered {
            Stroke::new(2.5_f32, Color32::from_rgb(13, 110, 253))
        } else {
            Stroke::new(1.2_f32, Color32::from_gray(160))
        };
        painter.rect_stroke(inset_rect, 0.0, border_stroke);

        // 9. Edit モード時のリサイズハンドル
        if is_edit_mode && is_selected {
            let handle_size = 6.0_f32;
            let h_stroke = Stroke::new(1.0_f32, Color32::WHITE);
            let h_color = Color32::from_rgb(13, 110, 253);

            let corners = [
                inset_rect.left_top(),
                inset_rect.right_top(),
                inset_rect.left_bottom(),
                inset_rect.right_bottom(),
                Pos2::new(inset_rect.center().x, inset_rect.top()),
                Pos2::new(inset_rect.center().x, inset_rect.bottom()),
                Pos2::new(inset_rect.left(), inset_rect.center().y),
                Pos2::new(inset_rect.right(), inset_rect.center().y),
            ];
            for p in corners {
                let h_rect = Rect::from_center_size(p, vec2(handle_size, handle_size));
                painter.rect_filled(h_rect, 1.0, h_color);
                painter.rect_stroke(h_rect, 1.0, h_stroke);
            }
        }
    }
}
