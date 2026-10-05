use std::path::Path;
use ndarray::Array1;

use crate::core::{AcquisitionMetadata, FtSettings, IntegrationItem, JCouplingResultItem, MultiviewItem, PeakItem};
use crate::core::analysis::integral::compute_integral;
use crate::core::signal::scale::calc_ppm_ticks;
use crate::gui::plot::transform::PlotTransform;
use crate::gui::dialogs::print_style_dialog::PrintStyleSettings;
use crate::gui::dialogs::print_dialog::{PrintOrientation, PrintSettings};

/// ページ全体の完全なベクター SVG を生成 (互換用ラッパー: デフォルトスタイル使用)
pub fn generate_complete_page_svg(
    settings: &PrintSettings,
    main_transform: Option<&PlotTransform>,
    ppm: Option<&Array1<f64>>,
    spectrum: Option<&Array1<f64>>,
    peaks: &[PeakItem],
    integrations: &[IntegrationItem],
    integration_scale: f64,
    integration_offset: f64,
    integration_ref_factor: f64,
    multiviews: &[MultiviewItem],
    metadata: &AcquisitionMetadata,
    ft_settings: &FtSettings,
    j_couplings: &[JCouplingResultItem],
    current_filepath: Option<&Path>,
) -> String {
    let default_style = PrintStyleSettings::default();
    generate_complete_page_svg_with_style(
        settings,
        &default_style,
        main_transform,
        ppm,
        spectrum,
        peaks,
        integrations,
        integration_scale,
        integration_offset,
        integration_ref_factor,
        multiviews,
        metadata,
        ft_settings,
        j_couplings,
        current_filepath,
    )
}

/// スタイル指定付きでページ全体の完全なベクター SVG を生成 (プロット + パラメータ表 + ヘッダー)
/// ユーザーが「Export SVG...」を押した際にファイル保存される完全な出版品質ドキュメント
pub fn generate_complete_page_svg_with_style(
    settings: &PrintSettings,
    style: &PrintStyleSettings,
    main_transform: Option<&PlotTransform>,
    ppm: Option<&Array1<f64>>,
    spectrum: Option<&Array1<f64>>,
    peaks: &[PeakItem],
    integrations: &[IntegrationItem],
    integration_scale: f64,
    integration_offset: f64,
    integration_ref_factor: f64,
    multiviews: &[MultiviewItem],
    metadata: &AcquisitionMetadata,
    ft_settings: &FtSettings,
    j_couplings: &[JCouplingResultItem],
    current_filepath: Option<&Path>,
) -> String {
    let (total_w, total_h) = match settings.orientation {
        PrintOrientation::Landscape => (1120.0, 792.0), // A4 Landscape比率
        PrintOrientation::Portrait => (792.0, 1120.0),  // A4 Portrait比率
    };

    let margin = 14.0;
    let mut cur_y = margin;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="100%" height="100%">
<rect width="{w}" height="{h}" fill="#ffffff" />
"##,
        w = total_w,
        h = total_h,
    );

    // 1. ヘッダー (ファイル名: 高さを固定して File Name の有無でプロットが動かないようにする)
    let header_h = 24.0;
    if settings.filename {
        let name_str = current_filepath
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled Spectrum".to_string());
        svg.push_str(&format!(
            r##"<text x="{x}" y="{y}" font-size="10" font-weight="600" font-family="sans-serif" fill="#495057">{title}</text>
<line x1="{x}" y1="{ly}" x2="{lx2}" y2="{ly}" stroke="#dee2e6" stroke-width="1.0" />
"##,
            x = margin,
            y = cur_y + 10.0,
            ly = cur_y + 16.0,
            lx2 = total_w - margin,
            title = html_escape(&name_str),
        ));
    }
    cur_y += header_h;

    let has_side = settings.info || (settings.jcoupling && !j_couplings.is_empty());
    let side_w = if has_side { 145.0 } else { 0.0 };
    let side_gap = if has_side { 10.0 } else { 0.0 };
    let plot_w = total_w - margin * 2.0 - side_w - side_gap;
    let plot_h = total_h - cur_y - margin;

    // 2. プロット部分 (SVG)
    if let (Some(ppm_arr), Some(spec_arr)) = (ppm, spectrum) {
        let plot_svg_inner = generate_plot_svg_content(
            settings,
            style,
            main_transform,
            ppm_arr,
            spec_arr,
            peaks,
            integrations,
            integration_scale,
            integration_offset,
            integration_ref_factor,
            multiviews,
            margin,
            cur_y,
            plot_w,
            plot_h,
        );
        svg.push_str(&plot_svg_inner);
    }

    // 3. 右側パラメータ表 (SVG ベクターテーブル)
    if has_side {
        let side_x = total_w - margin - side_w;
        let mut side_y = cur_y;

        if settings.info {
            svg.push_str(&format!(
                r##"<text x="{x}" y="{y}" font-size="9.5" font-weight="bold" font-family="sans-serif" fill="#212529">Experimental Parameters</text>
<line x1="{x}" y1="{ly}" x2="{lx2}" y2="{ly}" stroke="#0d6efd" stroke-width="1.5" />
"##,
                x = side_x,
                y = side_y + 10.0,
                ly = side_y + 14.0,
                lx2 = side_x + side_w,
            ));
            side_y += 20.0;

            let exp_rows = metadata.to_display_rows();
            for (k, v) in exp_rows {
                svg.push_str(&format!(
                    r##"<rect x="{x}" y="{y}" width="{w}" height="13" fill="#f8f9fa" stroke="#dee2e6" stroke-width="0.5" />
<text x="{tx1}" y="{ty}" font-size="7" font-weight="600" font-family="sans-serif" fill="#495057">{key}</text>
<text x="{tx2}" y="{ty}" font-size="7" font-family="sans-serif" fill="#212529">{val}</text>
"##,
                    x = side_x,
                    y = side_y,
                    w = side_w,
                    tx1 = side_x + 4.0,
                    tx2 = side_x + 58.0,
                    ty = side_y + 9.5,
                    key = k,
                    val = html_escape(&v),
                ));
                side_y += 13.0;
            }

            side_y += 10.0;
            svg.push_str(&format!(
                r##"<text x="{x}" y="{y}" font-size="10" font-weight="bold" font-family="sans-serif" fill="#212529">FT Settings</text>
<line x1="{x}" y1="{ly}" x2="{lx2}" y2="{ly}" stroke="#0d6efd" stroke-width="1.5" />
"##,
                x = side_x,
                y = side_y + 10.0,
                ly = side_y + 14.0,
                lx2 = side_x + side_w,
            ));
            side_y += 20.0;

            let eff_points = (metadata.points * ft_settings.zf_factor).max(1);
            let dig_res = format!("{:.4} Hz/pt", metadata.spectral_width_hz / (eff_points as f64));
            let ft_rows = [
                ("Window", match ft_settings.window {
                    crate::core::WindowFunction::None => "None".to_string(),
                    crate::core::WindowFunction::Exponential { lb } => format!("Exp ({} Hz)", lb),
                    crate::core::WindowFunction::Gaussian { g1, g2, g3 } => format!("Gauss ({},{},{})", g1, g2, g3),
                }),
                ("Zero Fill", format!("{}x", ft_settings.zf_factor)),
                ("Digital Res.", dig_res),
                ("Group Delay", if ft_settings.remove_digital_filter { "Removed" } else { "Kept" }.to_string()),
            ];

            for (k, v) in ft_rows {
                svg.push_str(&format!(
                    r##"<rect x="{x}" y="{y}" width="{w}" height="14" fill="#f8f9fa" stroke="#dee2e6" stroke-width="0.5" />
<text x="{tx1}" y="{ty}" font-size="7.5" font-weight="600" font-family="sans-serif" fill="#495057">{key}</text>
<text x="{tx2}" y="{ty}" font-size="7.5" font-family="sans-serif" fill="#212529">{val}</text>
"##,
                    x = side_x,
                    y = side_y,
                    w = side_w,
                    tx1 = side_x + 4.0,
                    tx2 = side_x + 58.0,
                    ty = side_y + 10.0,
                    key = k,
                    val = html_escape(&v),
                ));
                side_y += 14.0;
            }
        }

        if settings.jcoupling && !j_couplings.is_empty() {
            side_y += 12.0;
            svg.push_str(&format!(
                r##"<text x="{x}" y="{y}" font-size="10" font-weight="bold" font-family="sans-serif" fill="#212529">J Coupling</text>
<line x1="{x}" y1="{ly}" x2="{lx2}" y2="{ly}" stroke="#0d6efd" stroke-width="1.5" />
"##,
                x = side_x,
                y = side_y + 10.0,
                ly = side_y + 14.0,
                lx2 = side_x + side_w,
            ));
            side_y += 20.0;

            for jc in j_couplings.iter() {
                svg.push_str(&format!(
                    r##"<text x="{tx1}" y="{ty}" font-size="8" font-family="sans-serif" fill="#212529">{text}</text>
"##,
                    tx1 = side_x + 4.0,
                    ty = side_y + 10.0,
                    text = html_escape(&jc.text),
                ));
                side_y += 14.0;
            }
        }
    }

    svg.push_str("</svg>");
    svg
}

/// プロット部分の内部ベクター SVG 生成
fn generate_plot_svg_content(
    settings: &PrintSettings,
    style: &PrintStyleSettings,
    main_transform: Option<&PlotTransform>,
    ppm: &Array1<f64>,
    spectrum: &Array1<f64>,
    peaks: &[PeakItem],
    integrations: &[IntegrationItem],
    integration_scale: f64,
    integration_offset: f64,
    integration_ref_factor: f64,
    multiviews: &[MultiviewItem],
    plot_x: f64,
    plot_y: f64,
    plot_w: f64,
    plot_h: f64,
) -> String {
    let (p_min, p_max, y_min, y_max, main_screen_w, main_screen_h, main_screen_min_x, main_screen_min_y) = if let Some(t) = main_transform {
        (
            t.ppm_min,
            t.ppm_max,
            t.y_min,
            t.y_max,
            t.screen_rect.width() as f64,
            t.screen_rect.height() as f64,
            t.screen_rect.min.x as f64,
            t.screen_rect.min.y as f64,
        )
    } else {
        let p_min_data = ppm.iter().cloned().fold(f64::INFINITY, f64::min);
        let p_max_data = ppm.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let y_max_data = spectrum.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(1.0);
        (p_min_data, p_max_data, -0.05 * y_max_data, y_max_data * 1.1, plot_w, plot_h, 0.0, 0.0)
    };

    let bottom_margin = 65.0;
    let actual_plot_h = (plot_h - bottom_margin).max(10.0);
    let axis_y = plot_y + actual_plot_h;

    let ppm_to_x = |p: f64| -> f64 {
        plot_x + (p_max - p) / (p_max - p_min).max(1e-6) * plot_w
    };
    let y_to_y = |y: f64| -> f64 {
        axis_y - (y - y_min) / (y_max - y_min).max(1e-6) * actual_plot_h
    };

    let p_low = p_min.min(p_max);
    let p_high = p_min.max(p_max);

    let mut svg = String::new();

    // 0. プロット領域のクリッピングパス定義 (はみ出し防止)
    svg.push_str(&format!(
        r##"<defs><clipPath id="main-plot-clip"><rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" /></clipPath></defs>
"##,
        x = plot_x,
        y = plot_y,
        w = plot_w,
        h = actual_plot_h + 1.0,
    ));

    // 1. スペクトル曲線 (Min/Max リサンプリングで微細ピークの欠落を防止)
    if settings.spectrum && !ppm.is_empty() && !spectrum.is_empty() {
        let n_pts = ppm.len().min(spectrum.len());
        let mut points_str = String::with_capacity(n_pts.min(16384) * 12);
        let mut prev_x = -9999.0_f64;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for i in 0..n_pts {
            let p = ppm[i];
            if p < p_low || p > p_high {
                continue;
            }
            let sx = ppm_to_x(p);
            let sy = y_to_y(spectrum[i]).min(axis_y);

            let px = (sx * 2.0).round() / 2.0; // 0.5px サンプリング
            if (px - prev_x).abs() >= 0.5 {
                if prev_x >= plot_x - 5.0 && min_y <= max_y {
                    points_str.push_str(&format!("{:.1},{:.1} ", prev_x, min_y));
                    if (max_y - min_y).abs() > 0.3 {
                        points_str.push_str(&format!("{:.1},{:.1} ", prev_x, max_y));
                    }
                }
                prev_x = px;
                min_y = sy;
                max_y = sy;
            } else {
                if sy < min_y { min_y = sy; }
                if sy > max_y { max_y = sy; }
            }
        }
        if prev_x >= plot_x - 5.0 && min_y <= max_y {
            points_str.push_str(&format!("{:.1},{:.1} ", prev_x, min_y));
            if (max_y - min_y).abs() > 0.3 {
                points_str.push_str(&format!("{:.1},{:.1} ", prev_x, max_y));
            }
        }

        if !points_str.is_empty() {
            svg.push_str(&format!(
                r##"<polyline points="{pts}" fill="none" stroke="{color}" stroke-width="{w:.2}" clip-path="url(#main-plot-clip)" />
"##,
                pts = points_str.trim_end(),
                color = style.main_spectrum.line_color.to_hex(),
                w = style.main_spectrum.line_width,
            ));
        }
    }

    // 2. X軸 (PPM軸 & 目盛り & ラベル)
    svg.push_str(&format!(
        r##"<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="#212529" stroke-width="1.0" />
"##,
        x1 = plot_x,
        x2 = plot_x + plot_w,
        y = axis_y,
    ));

    let span = (p_max - p_min).abs();
    let (tick_interval, tick_dec) = calc_ppm_ticks(span, settings.auto_ticks, settings.tick_major);

    let minor_n = settings.tick_minor.max(1);
    let minor_step = tick_interval / (minor_n as f64);

    let first_tick = (p_low / tick_interval).ceil() * tick_interval;
    let mut cur_tick = first_tick;
    while cur_tick <= p_high {
        let tx = ppm_to_x(cur_tick);
        if tx >= plot_x && tx <= plot_x + plot_w {
            svg.push_str(&format!(
                r##"<line x1="{x:.1}" y1="{y1}" x2="{x:.1}" y2="{y2}" stroke="#212529" stroke-width="1.0" />
<text x="{x:.1}" y="{ty}" font-size="9" text-anchor="middle" font-family="sans-serif" fill="#212529">{val:.prec$}</text>
"##,
                x = tx,
                y1 = axis_y,
                y2 = axis_y + 4.0,
                ty = axis_y + 14.0,
                val = cur_tick,
                prec = tick_dec,
            ));
        }

        // サブ目盛り (N分割、数字なし)
        if minor_n > 1 {
            for m in 1..minor_n {
                let sub_tick = cur_tick + (m as f64) * minor_step;
                if sub_tick <= p_high {
                    let stx = ppm_to_x(sub_tick);
                    if stx >= plot_x && stx <= plot_x + plot_w {
                        svg.push_str(&format!(
                            r##"<line x1="{x:.1}" y1="{y1}" x2="{x:.1}" y2="{y2}" stroke="#6c757d" stroke-width="0.6" />
"##,
                            x = stx,
                            y1 = axis_y,
                            y2 = axis_y + 2.0,
                        ));
                    }
                }
            }
        }

        cur_tick += tick_interval;
    }

    // 3. ピーク表示 (頭頂部マーク & アンチコリジョン引き出し線 & 縦向きPPM値)
    if settings.peak {
        let mut visible_peaks: Vec<&PeakItem> = peaks
            .iter()
            .filter(|p| p.ppm >= p_low && p.ppm <= p_high)
            .collect();
        visible_peaks.sort_by(|a, b| b.ppm.partial_cmp(&a.ppm).unwrap_or(std::cmp::Ordering::Equal));

        if !visible_peaks.is_empty() {
            let mut text_x: Vec<f64> = visible_peaks
                .iter()
                .map(|p| ppm_to_x(p.ppm))
                .collect();

            let min_gap = 10.0;
            for _ in 0..200 {
                let mut moved = false;
                for i in 1..text_x.len() {
                    let diff = text_x[i] - text_x[i - 1];
                    if diff < min_gap {
                        let overlap = min_gap - diff;
                        text_x[i - 1] -= overlap * 0.5;
                        text_x[i] += overlap * 0.5;
                        moved = true;
                    }
                }
                if !moved { break; }
            }

            let y_elbow = axis_y + 15.0;
            let y_text_start = axis_y + 28.0;

            for (i, pk) in visible_peaks.iter().enumerate() {
                let px = ppm_to_x(pk.ppm);
                let tx = text_x[i];

                // 枠外にはみ出るものはスキップ (omit)
                if tx < plot_x + 2.0 || tx > plot_x + plot_w - 2.0 || px < plot_x || px > plot_x + plot_w {
                    continue;
                }

                let pk_sy = y_to_y(pk.intensity);
                let y_start = (axis_y - 8.0).max(pk_sy + 2.0);

                svg.push_str(&format!(
                    r##"<line x1="{px:.1}" y1="{y_start:.1}" x2="{px:.1}" y2="{y_elbow:.1}" stroke="{col}" stroke-width="{w:.2}" />
<line x1="{px:.1}" y1="{y_elbow:.1}" x2="{tx:.1}" y2="{y_text_start_pre:.1}" stroke="{col}" stroke-width="{w:.2}" />
<line x1="{tx:.1}" y1="{y_text_start_pre:.1}" x2="{tx:.1}" y2="{y_text_start:.1}" stroke="{col}" stroke-width="{w:.2}" />
<g transform="translate({tx:.1}, {ty:.1}) rotate(90)"><text x="0" y="0" font-size="7.5" text-anchor="start" dominant-baseline="central" font-family="sans-serif" fill="#000000">{val:.prec$}</text></g>
"##,
                    px = px,
                    y_start = y_start,
                    y_elbow = y_elbow,
                    y_text_start_pre = y_text_start - 3.0,
                    tx = tx,
                    y_text_start = y_text_start,
                    ty = y_text_start + 2.0,
                    val = pk.ppm,
                    prec = settings.ppm_decimals,
                    col = style.main_spectrum.peak_lead_color.to_hex(),
                    w = style.main_spectrum.peak_lead_width,
                ));
            }
        }
    }

    // 4. 積分
    if settings.integrate {
        for it in integrations {
            let p_start = it.start_ppm.max(it.end_ppm);
            let p_end = it.start_ppm.min(it.end_ppm);
            let sx_start = ppm_to_x(p_start);
            let sx_end = ppm_to_x(p_end);

            if sx_end > plot_x && sx_start < plot_x + plot_w {
                let bl_y1 = y_to_y(it.y_start);
                let bl_y2 = y_to_y(it.y_end);
                svg.push_str(&format!(
                    r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#3b82f6" stroke-width="0.9" stroke-dasharray="3,3" />
"##,
                    x1 = sx_start,
                    y1 = bl_y1,
                    x2 = sx_end,
                    y2 = bl_y2,
                ));

                if let Some(res) = compute_integral(spectrum, ppm, it, integration_scale, integration_ref_factor, integration_offset) {
                    if res.ppm.len() > 1 && res.ppm.len() == res.curve_y.len() {
                        let mut curve_d = String::new();
                        let mut min_sy = f64::MAX;
                        let mut first = true;

                        for idx in 0..res.ppm.len() {
                            let p = res.ppm[idx];
                            let sx = ppm_to_x(p);
                            let sy = y_to_y(res.curve_y[idx]).min(axis_y);
                            if sy < min_sy { min_sy = sy; }

                            if first {
                                curve_d.push_str(&format!("M {:.1} {:.1} ", sx, sy));
                                first = false;
                            } else {
                                curve_d.push_str(&format!("L {:.1} {:.1} ", sx, sy));
                            }
                        }

                        svg.push_str(&format!(
                            r##"<path d="{}" stroke="{col}" stroke-width="{w:.2}" fill="none" />
<g transform="translate({mx:.1}, {ty:.1}) rotate(90)"><text x="0" y="0" font-size="8.0" text-anchor="end" dominant-baseline="central" font-family="sans-serif" fill="{col}">{val:.prec$}</text></g>
"##,
                            curve_d,
                            mx = (sx_start + sx_end) * 0.5,
                            ty = min_sy - 3.0,
                            val = res.normalized_value,
                            prec = settings.integral_decimals,
                            col = style.main_spectrum.integral_color.to_hex(),
                            w = style.main_spectrum.integral_width,
                        ));
                    }
                }
            }
        }
    }

    // 5. マルチビュー (拡大スペクトル、積分、ピーク引き出し線、X軸目盛り)
    if settings.multiview && !multiviews.is_empty() {
        for mv in multiviews {
            let rel_x = (mv.geometry.x as f64 - main_screen_min_x) / main_screen_w.max(1.0);
            let rel_y = (mv.geometry.y as f64 - main_screen_min_y) / main_screen_h.max(1.0);
            let rel_w = (mv.geometry.w as f64) / main_screen_w.max(1.0);
            let rel_h = (mv.geometry.h as f64) / main_screen_h.max(1.0);

            let inset_x = plot_x + rel_x * plot_w;
            let inset_y = plot_y + rel_y * plot_h;
            let inset_w = rel_w * plot_w;
            let inset_h = rel_h * plot_h;

            if inset_w > 30.0 && inset_h > 30.0 {
                svg.push_str(&format!(
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="#ffffff" stroke="#888888" stroke-width="1.1" />
"##,
                    x = inset_x,
                    y = inset_y,
                    w = inset_w,
                    h = inset_h,
                ));

                let mv_src_min = mv.src_x_min.min(mv.src_x_max);
                let mv_src_max = mv.src_x_min.max(mv.src_x_max);

                let mut mv_y_min = f64::INFINITY;
                let mut mv_y_max = f64::NEG_INFINITY;
                for idx in 0..ppm.len().min(spectrum.len()) {
                    let p = ppm[idx];
                    if p >= mv_src_min && p <= mv_src_max {
                        let v = spectrum[idx];
                        if v < mv_y_min { mv_y_min = v; }
                        if v > mv_y_max { mv_y_max = v; }
                    }
                }
                if mv_y_min >= mv_y_max {
                    mv_y_min = 0.0;
                    mv_y_max = 1.0;
                }

                let y_min_val = mv.src_y_min.unwrap_or_else(|| mv_y_min.min(0.0));
                let y_max_val = mv.src_y_max.unwrap_or(mv_y_max);
                let h_diff = (y_max_val - y_min_val).max(1e-6);
                let y_min_adj = y_min_val - 0.02 * h_diff;
                let y_max_adj = y_max_val + 0.40 * h_diff;

                let inset_axis_y = inset_y + inset_h - 16.0;
                let inset_plot_h = (inset_axis_y - inset_y - 6.0).max(10.0);
                let inset_plot_w = (inset_w - 12.0).max(10.0);

                let mv_ppm_to_x = |p: f64| -> f64 {
                    inset_x + 6.0 + (mv_src_max - p) / (mv_src_max - mv_src_min).max(1e-6) * inset_plot_w
                };
                let mv_y_to_y = |y: f64| -> f64 {
                    inset_axis_y - (y - y_min_adj) / (y_max_adj - y_min_adj).max(1e-6) * inset_plot_h
                };

                // 1. 拡大スペクトル曲線 (黒色)
                let mut mv_d = String::new();
                let mut first = true;
                for idx in 0..ppm.len().min(spectrum.len()) {
                    let p = ppm[idx];
                    if p >= mv_src_min && p <= mv_src_max {
                        let sx = mv_ppm_to_x(p);
                        let sy = mv_y_to_y(spectrum[idx]);
                        if first {
                            mv_d.push_str(&format!("M {:.1} {:.1} ", sx, sy.min(inset_axis_y)));
                            first = false;
                        } else {
                            mv_d.push_str(&format!("L {:.1} {:.1} ", sx, sy.min(inset_axis_y)));
                        }
                    }
                }
                if !mv_d.is_empty() {
                    svg.push_str(&format!(
                        r##"<path d="{}" stroke="{}" stroke-width="{:.2}" fill="none" />
"##,
                        mv_d,
                        style.multiview.line_color.to_hex(),
                        style.multiview.line_width,
                    ));
                }

                // 2. 積分 (マルチビュー内)
                if settings.integrate {
                    for integ in integrations {
                        let i_min = integ.min_ppm();
                        let i_max = integ.max_ppm();
                        if i_max < mv_src_min || i_min > mv_src_max {
                            continue;
                        }
                        if let Some(res) = compute_integral(spectrum, ppm, integ, 1.0, integration_ref_factor, 0.0) {
                            if res.ppm.len() > 1 && res.total_area.abs() > 1e-12 {
                                let mut intg_d = String::new();
                                let mut first_intg = true;
                                for (&p, &cy) in res.ppm.iter().zip(res.curve_y.iter()) {
                                    if p >= mv_src_min && p <= mv_src_max {
                                        let bl = integ.baseline_y_at(p);
                                        let cum = cy - bl;
                                        let norm_y = (cum / res.total_area).clamp(0.0, 1.0);
                                        let target_data_y = y_min_adj + 0.20 * h_diff + norm_y * (0.45 * h_diff);
                                        let sx = mv_ppm_to_x(p);
                                        let sy = mv_y_to_y(target_data_y);
                                        if first_intg {
                                            intg_d.push_str(&format!("M {:.1} {:.1} ", sx, sy));
                                            first_intg = false;
                                        } else {
                                            intg_d.push_str(&format!("L {:.1} {:.1} ", sx, sy));
                                        }
                                    }
                                }
                                if !intg_d.is_empty() {
                                    svg.push_str(&format!(
                                        r##"<path d="{}" stroke="{col}" stroke-width="{w:.2}" fill="none" />
"##,
                                        intg_d,
                                        col = style.multiview.integral_color.to_hex(),
                                        w = style.multiview.integral_width,
                                    ));
                                    let mid_p = (i_min.max(mv_src_min) + i_max.min(mv_src_max)) * 0.5;
                                    let tx = mv_ppm_to_x(mid_p);
                                    let ty = mv_y_to_y(y_min_adj + 0.70 * h_diff);
                                    svg.push_str(&format!(
                                        r##"<g transform="translate({tx:.1}, {ty:.1}) rotate(90)"><text x="0" y="0" font-size="7.5" text-anchor="end" dominant-baseline="central" font-family="sans-serif" fill="{col}">{val:.prec$}</text></g>
"##,
                                        tx = tx,
                                        ty = ty,
                                        val = res.normalized_value,
                                        prec = settings.integral_decimals,
                                        col = style.multiview.integral_color.to_hex(),
                                    ));
                                }
                            }
                        }
                    }
                }

                // 3. ピーク引き出し線 & 縦書き化学シフト値 (マルチビュー内)
                if settings.peak {
                    let sub_peaks: Vec<&PeakItem> = peaks
                        .iter()
                        .filter(|pk| pk.ppm >= mv_src_min && pk.ppm <= mv_src_max)
                        .collect();
                    if !sub_peaks.is_empty() {
                        let mut sorted_peaks = sub_peaks.clone();
                        sorted_peaks.sort_by(|a, b| b.ppm.partial_cmp(&a.ppm).unwrap_or(std::cmp::Ordering::Equal));

                        let mut mv_text_x: Vec<f64> = sorted_peaks.iter().map(|p| mv_ppm_to_x(p.ppm)).collect();
                        let min_gap = 7.5;
                        for _ in 0..100 {
                            let mut moved = false;
                            for i in 1..mv_text_x.len() {
                                let diff = mv_text_x[i] - mv_text_x[i - 1];
                                if diff < min_gap {
                                    let overlap = min_gap - diff;
                                    mv_text_x[i - 1] -= overlap * 0.5;
                                    mv_text_x[i] += overlap * 0.5;
                                    moved = true;
                                }
                            }
                            if !moved { break; }
                        }

                        let mv_text_start_y = inset_y + 4.0;
                        let text_len = 16.0;
                        let text_bottom_y = mv_text_start_y + text_len;
                        let mv_elbow_y = text_bottom_y + 3.0;
                        let max_lead_y = (inset_y + inset_plot_h * 0.25).max(mv_elbow_y + 3.0);

                        for (i, pk) in sorted_peaks.iter().enumerate() {
                            let px = mv_ppm_to_x(pk.ppm);
                            let tx = mv_text_x[i];

                            // 枠外にはみ出るものはスキップ (omit)
                            if tx < inset_x + 3.0 || tx > inset_x + inset_w - 3.0 || px < inset_x || px > inset_x + inset_w {
                                continue;
                            }

                            let py = mv_y_to_y(pk.intensity);
                            let clearance = 6.0;
                            let line_start_y = (py - clearance).min(max_lead_y);

                            if line_start_y > mv_elbow_y + 1.0 {
                                svg.push_str(&format!(
                                    r##"<line x1="{px:.1}" y1="{line_start_y:.1}" x2="{px:.1}" y2="{mv_elbow_y:.1}" stroke="{col}" stroke-width="{w:.2}" />
<line x1="{px:.1}" y1="{mv_elbow_y:.1}" x2="{tx:.1}" y2="{y_elbow_t:.1}" stroke="{col}" stroke-width="{w:.2}" />
<line x1="{tx:.1}" y1="{y_elbow_t:.1}" x2="{tx:.1}" y2="{y_start_t:.1}" stroke="{col}" stroke-width="{w:.2}" />
"##,
                                    px = px,
                                    line_start_y = line_start_y,
                                    mv_elbow_y = mv_elbow_y,
                                    tx = tx,
                                    y_elbow_t = text_bottom_y + 1.5,
                                    y_start_t = text_bottom_y,
                                    col = style.multiview.peak_lead_color.to_hex(),
                                    w = style.multiview.peak_lead_width,
                                ));
                            }

                            svg.push_str(&format!(
                                r##"<g transform="translate({tx:.1}, {ty:.1}) rotate(90)"><text x="0" y="0" font-size="6" text-anchor="start" dominant-baseline="central" font-family="sans-serif" fill="#000000">{val:.prec$}</text></g>
"##,
                                tx = tx,
                                ty = mv_text_start_y,
                                val = pk.ppm,
                                prec = settings.ppm_decimals,
                            ));
                        }
                    }
                }

                // 4. X 軸目盛り線 & PPM 数値ラベル (マルチビュー内)
                svg.push_str(&format!(
                    r##"<line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}" stroke="#212529" stroke-width="0.8" />
"##,
                    x1 = inset_x + 6.0,
                    x2 = inset_x + inset_w - 6.0,
                    y = inset_axis_y,
                ));

                let ppm_span = mv_src_max - mv_src_min;
                let target_ticks = (inset_plot_w / 40.0).clamp(2.0, 5.0);
                let rough_step = (ppm_span / target_ticks).max(1e-6);
                let exponent = rough_step.log10().floor();
                let frac = rough_step / 10.0_f64.powf(exponent);
                let nice_frac = if frac <= 1.5 { 1.0 } else if frac <= 3.0 { 2.0 } else if frac <= 7.0 { 5.0 } else { 10.0 };
                let step = nice_frac * 10.0_f64.powf(exponent);
                let start_tick = (mv_src_min / step).ceil() as i64;
                let end_tick = (mv_src_max / step).floor() as i64;
                let decimals = if step < 0.0099 { 3 } else if step < 0.099 { 2 } else if step < 0.99 { 1 } else { 0 };

                let minor_step = step / 10.0;
                let start_minor = (mv_src_min / minor_step).ceil() as i64;
                let end_minor = (mv_src_max / minor_step).floor() as i64;

                // サブ目盛り (10分割、短め、ラベルなし)
                for m_idx in start_minor..=end_minor {
                    if m_idx % 10 == 0 { continue; }
                    let m_ppm = m_idx as f64 * minor_step;
                    let m_x = mv_ppm_to_x(m_ppm);
                    if m_x >= inset_x + 6.0 && m_x <= inset_x + inset_w - 6.0 {
                        svg.push_str(&format!(
                            r##"<line x1="{tx:.1}" y1="{y1:.1}" x2="{tx:.1}" y2="{y2:.1}" stroke="#6c757d" stroke-width="0.5" />
"##,
                            tx = m_x,
                            y1 = inset_axis_y,
                            y2 = inset_axis_y + 1.5,
                        ));
                    }
                }

                // メイン目盛り
                for t_idx in start_tick..=end_tick {
                    let tick_ppm = t_idx as f64 * step;
                    let tick_x = mv_ppm_to_x(tick_ppm);
                    if tick_x >= inset_x + 6.0 && tick_x <= inset_x + inset_w - 6.0 {
                        svg.push_str(&format!(
                            r##"<line x1="{tx:.1}" y1="{y1:.1}" x2="{tx:.1}" y2="{y2:.1}" stroke="#212529" stroke-width="0.7" />
"##,
                            tx = tick_x,
                            y1 = inset_axis_y,
                            y2 = inset_axis_y + 3.0,
                        ));
                        if tick_x >= inset_x + 10.0 && tick_x <= inset_x + inset_w - 10.0 {
                            svg.push_str(&format!(
                                r##"<text x="{tx:.1}" y="{ty:.1}" font-size="7" text-anchor="middle" font-family="sans-serif" fill="#212529">{val:.prec$}</text>
"##,
                                tx = tick_x,
                                ty = inset_axis_y + 11.0,
                                val = tick_ppm,
                                prec = decimals,
                            ));
                        }
                    }
                }
            }
        }
    }

    svg
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
