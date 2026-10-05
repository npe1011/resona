use crate::core::compute_integral;
use crate::gui::dialogs::print_dialog::PrintOrientation;
use crate::gui::dialogs::print_style_dialog::PrintStyleSettings;
use crate::multispec::settings::MultiSpecPrintSettings;
use crate::multispec::state::MultiSpecState;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// スタック比較スペクトルのベクター SVG 文字列を生成する
pub fn export_multispec_svg(
    state: &MultiSpecState,
    print_settings: &MultiSpecPrintSettings,
    _style: &PrintStyleSettings,
    ppm_range: (f64, f64),
) -> String {
    let (total_w, total_h) = match print_settings.orientation {
        PrintOrientation::Landscape => (1120.0, 792.0),
        PrintOrientation::Portrait => (792.0, 1120.0),
    };

    // mm to px (約 3.78 px/mm)
    let margin = (print_settings.margin_mm * 3.7795).max(15.0);

    let mut cur_y = margin;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="100%" height="100%">
<rect width="{w}" height="{h}" fill="#ffffff" />
"##,
        w = total_w,
        h = total_h,
    );

    // 1. タイトル
    let display_title = if !state.title.is_empty() {
        &state.title
    } else {
        &print_settings.title
    };
    if print_settings.show_title && !display_title.is_empty() {
        svg.push_str(&format!(
            r##"<text x="{x}" y="{y}" font-size="14" font-weight="bold" font-family="sans-serif" fill="#212529">{title}</text>
<line x1="{x}" y1="{ly}" x2="{lx2}" y2="{ly}" stroke="#ced4da" stroke-width="1.0" />
"##,
            x = margin,
            y = cur_y + 14.0,
            ly = cur_y + 22.0,
            lx2 = total_w - margin,
            title = html_escape(display_title),
        ));
        cur_y += 32.0;
    }

    // 2. 軸領域の高さ予約
    let footer_h = 0.0;
    let axis_h = if print_settings.show_axis { 35.0 } else { 10.0 };

    let plot_x = margin;
    let plot_w = total_w - margin * 2.0;
    let plot_bottom_y = total_h - margin - footer_h;
    let axis_y = plot_bottom_y - axis_h;
    let plot_h = (axis_y - cur_y).max(100.0);

    let p_min = ppm_range.0;
    let p_max = ppm_range.1;
    let p_span = (p_max - p_min).abs().max(1e-4);

    let ppm_to_x = |ppm: f64| -> f64 {
        let ratio = (p_max - ppm) / p_span;
        plot_x + ratio * plot_w
    };

    let visible_items: Vec<usize> = state
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.visible && it.project.is_some())
        .map(|(idx, _)| idx)
        .collect();

    let num_vis = visible_items.len();
    let slot_h = if num_vis > 0 { plot_h / num_vis as f64 } else { plot_h };

    // 3. 各スペクトルの描画 (Auto Stack または Overlay)
    for (step_idx, &item_idx) in visible_items.iter().enumerate() {
        let it = &state.items[item_idx];
        let proj = match it.project {
            Some(ref p) => p,
            None => continue,
        };
        let ppm = match proj.ppm {
            Some(ref p) => p,
            None => continue,
        };
        let spec = match proj.spectrum_real {
            Some(ref s) => s,
            None => continue,
        };

        let (base_y, unit_h, slot_top) = if state.is_overlay {
            let u_h = (plot_h * 0.72).max(10.0);
            let b_y = axis_y - 25.0 - it.y_offset;
            (b_y, u_h, cur_y)
        } else {
            let s_top = cur_y + step_idx as f64 * slot_h;
            let u_h = (slot_h * 0.72).max(10.0);
            let b_y = s_top + slot_h - 10.0 - it.y_offset;
            (b_y, u_h, s_top)
        };

        let max_val = proj.max_intensity().max(1e-6);
        let top_val = max_val * (100.0 / it.y_scale_max.max(1.0));
        let min_val = -max_val * (it.y_scale_min / 100.0);
        let val_span = (top_val - min_val).max(1e-6);

        let color_hex = format!("#{:02x}{:02x}{:02x}", it.color[0], it.color[1], it.color[2]);

        // 3.1 スペクトル名 (波形左上に直接色文字テキストで描画)
        if print_settings.show_spectrum_names && !it.name.is_empty() {
            let escaped_name = html_escape(&it.name);
            let tag_x = plot_x + 8.0;
            let tag_y = if state.is_overlay {
                cur_y + 14.0 + (step_idx as f64 * 18.0)
            } else {
                slot_top + 14.0
            };

            svg.push_str(&format!(
                r##"<text x="{x}" y="{y}" font-size="10.5" font-family="sans-serif" font-weight="600" fill="{color}">{name}</text>
"##,
                x = tag_x,
                y = tag_y,
                color = color_hex,
                name = escaped_name,
            ));
        }

        // 3.2 波形ポリライン (高解像度ベクター)
        let n_pts = ppm.len().min(spec.len());
        if n_pts > 1 {
            let mut points_str = String::with_capacity(n_pts * 12);
            let mut prev_x = -9999.0_f64;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;

            for k in 0..n_pts {
                let p = ppm[k];
                if (p < p_min && p < p_max) || (p > p_min && p > p_max) {
                    continue;
                }
                let v = spec[k];
                let sx = ppm_to_x(p);
                let y_norm = (v - min_val) / val_span;
                let sy = base_y - y_norm * unit_h;

                let px = (sx * 2.0).round() / 2.0; // 0.5px サンプリング
                if (px - prev_x).abs() >= 0.5 {
                    if prev_x >= plot_x - 5.0 {
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
                    r##"<polyline points="{pts}" fill="none" stroke="{color}" stroke-width="1.0" />
"##,
                    pts = points_str.trim_end(),
                    color = color_hex,
                ));
            }
        }

        // 3.3 積分曲線 & 90度回転縦書き数値ラベル
        if print_settings.show_integrals && it.show_integral && !proj.state.integrations.is_empty() {
            let ref_factor = proj.state.integration_ref_factor();

            for intg in &proj.state.integrations {
                if let Some(res) = compute_integral(
                    spec,
                    ppm,
                    intg,
                    proj.state.integration_scale,
                    ref_factor,
                    0.03,
                ) {
                    if res.curve_y.len() > 1 && res.curve_y.len() == res.ppm.len() {
                        let mut intg_pts = String::with_capacity(res.ppm.len() * 12);
                        let mut max_curve_y = f64::NEG_INFINITY;
                        for ki in 0..res.ppm.len() {
                            let p = res.ppm[ki];
                            let sx = ppm_to_x(p);
                            let y_norm = (res.curve_y[ki] - min_val) / val_span;
                            let sy = base_y - y_norm * unit_h;
                            intg_pts.push_str(&format!("{:.1},{:.1} ", sx, sy));
                            if res.curve_y[ki] > max_curve_y {
                                max_curve_y = res.curve_y[ki];
                            }
                        }
                        let intg_color = format!(
                            "#{:02x}{:02x}{:02x}",
                            (it.color[0] as f64 * 0.85) as u8,
                            (it.color[1] as f64 * 0.85) as u8,
                            (it.color[2] as f64 * 0.85) as u8,
                        );
                        svg.push_str(&format!(
                            r##"<polyline points="{pts}" fill="none" stroke="{color}" stroke-width="0.9" />
"##,
                            pts = intg_pts.trim_end(),
                            color = intg_color,
                        ));

                        // 積分値ラベル (曲線の上部に時計回り90度回転で配置)
                        let mid_p = (intg.start_ppm + intg.end_ppm) * 0.5;
                        let sx_mid = ppm_to_x(mid_p);
                        let y_norm_top = (max_curve_y - min_val) / val_span;
                        let sy_top = base_y - y_norm_top * unit_h;
                        let text_y = sy_top - 6.0;
                        let val_str = format!("{:.prec$}", res.normalized_value, prec = print_settings.integral_decimals);
                        svg.push_str(&format!(
                            r##"<text x="{x}" y="{y}" font-size="9" font-family="sans-serif" fill="{color}" transform="rotate(90, {x}, {y})" text-anchor="start">{val}</text>
"##,
                            x = sx_mid,
                            y = text_y,
                            color = color_hex,
                            val = val_str,
                        ));
                    }
                }
            }
        }
    }

    // 4. 最下部共通 X 軸 (ppm)
    if print_settings.show_axis {
        // 主軸線
        svg.push_str(&format!(
            r##"<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="#212529" stroke-width="1.2" />
"##,
            x1 = plot_x,
            y = axis_y,
            x2 = plot_x + plot_w,
        ));

        // 目盛り計算 (calc_ppm_ticks 共通ロジックに統一)
        let (step, dec) = crate::core::calc_ppm_ticks(p_span, print_settings.auto_ticks, print_settings.tick_major);
        let minor_n = print_settings.tick_minor.max(1);
        let minor_step = step / (minor_n as f64);

        let p_low = p_min.min(p_max);
        let p_high = p_min.max(p_max);
        let first_major = (p_low / step).floor() * step;
        let last_major = (p_high / step).ceil() * step;

        let mut cur_major = first_major - step;
        while cur_major <= last_major + step * 0.1 {
            // 主目盛り
            if cur_major >= p_low - 1e-9 && cur_major <= p_high + 1e-9 {
                let x = ppm_to_x(cur_major);
                if x >= plot_x - 1.0 && x <= plot_x + plot_w + 1.0 {
                    // 主目盛り線
                    svg.push_str(&format!(
                        r##"<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke="#212529" stroke-width="1.0" />
"##,
                        x = x,
                        y1 = axis_y,
                        y2 = axis_y + 6.0,
                    ));

                    // 数値ラベル
                    let label = format!("{:.prec$}", cur_major, prec = dec);
                    svg.push_str(&format!(
                        r##"<text x="{x}" y="{y}" font-size="10" font-family="sans-serif" text-anchor="middle" fill="#212529">{lbl}</text>
"##,
                        x = x,
                        y = axis_y + 17.0,
                        lbl = label,
                    ));
                }
            }

            // サブ目盛り (主目盛り外側の余白部も描画)
            if minor_n > 1 {
                for s in 1..minor_n {
                    let sub_ppm = cur_major + (s as f64) * minor_step;
                    if sub_ppm >= p_low - 1e-9 && sub_ppm <= p_high + 1e-9 {
                        let sx = ppm_to_x(sub_ppm);
                        if sx >= plot_x && sx <= plot_x + plot_w {
                            let tick_len = if minor_n % 2 == 0 && s == minor_n / 2 { 4.5 } else { 3.5 };
                            svg.push_str(&format!(
                                r##"<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke="#495057" stroke-width="0.8" />
"##,
                                x = sx,
                                y1 = axis_y,
                                y2 = axis_y + tick_len,
                            ));
                        }
                    }
                }
            }

            cur_major += step;
        }

        // "(ppm)" 単位ラベル
        svg.push_str(&format!(
            r##"<text x="{x}" y="{y}" font-size="10" font-family="sans-serif" text-anchor="end" fill="#495057">(ppm)</text>
"##,
            x = plot_x + plot_w,
            y = axis_y + 28.0,
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array1;
    use crate::core::Project;

    #[test]
    fn test_export_multispec_svg_structure() {
        let mut state = MultiSpecState::default();

        let mut p1 = Project::new();
        p1.ppm = Some(Array1::linspace(10.0, 0.0, 100));
        p1.spectrum_real = Some(Array1::ones(100) * 50.0);

        state.add_project("Sample A".to_string(), None, p1);

        let mut print_settings = MultiSpecPrintSettings::default();
        print_settings.title = "Test Stack".to_string();
        print_settings.show_title = true;
        print_settings.show_spectrum_names = true;
        print_settings.show_axis = true;

        let style_settings = PrintStyleSettings::default();
        let svg = export_multispec_svg(&state, &print_settings, &style_settings, (0.0, 10.0));

        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("Test Stack"));
        assert!(svg.contains("Sample A"));
        assert!(svg.contains("(ppm)"));

        // state.title が優先されることの確認
        state.title = "Custom State Title".to_string();
        let svg2 = export_multispec_svg(&state, &print_settings, &style_settings, (0.0, 10.0));
        assert!(svg2.contains("Custom State Title"));
    }
}
