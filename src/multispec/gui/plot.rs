use egui::{
    epaint::PathShape, vec2, Color32, FontFamily, FontId, Key, Pos2, Rect, ScrollArea, Stroke, Ui,
};

use crate::core::compute_integral;
use crate::multispec::gui::toolbar::MultiSpecZoomMode;
use crate::multispec::settings::MultiSpecSettings;
use crate::multispec::state::{MultiSpecItem, MultiSpecState, StackLayoutMode};

/// スタック比較プロットの描画とインタラクション
pub fn show_multispec_plot(
    ui: &mut Ui,
    state: &mut MultiSpecState,
    settings: &MultiSpecSettings,
    zoom_mode: &mut MultiSpecZoomMode,
    drag_start: &mut Option<Pos2>,
    drag_current: &mut Option<Pos2>,
) {
    let axis_h = 32.0_f32;
    let is_overlay = state.is_overlay;
    let is_scroll_mode = !is_overlay && state.stack_mode == StackLayoutMode::Scroll;

    let p_min = state.common_ppm_min;
    let p_max = state.common_ppm_max;
    let p_span = (p_max - p_min).abs().max(1e-4);

    let visible_items: Vec<usize> = state
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.visible && it.project.is_some())
        .map(|(idx, _)| idx)
        .collect();

    let num_vis = visible_items.len();

    if is_scroll_mode {
        // =========================================================
        // モードA: 固定スロット高さ + 縦スクロール (ScrollArea)
        // =========================================================
        ui.vertical(|ui| {
            let avail_w = ui.available_width();
            let avail_h = (ui.available_height() - axis_h).max(100.0);
            let slot_h = state.fixed_slot_height.max(60.0);
            let total_content_h = (slot_h * num_vis as f32).max(avail_h);

            // 1. スペクトル縦スクロール表示エリア
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(avail_h)
                .show(ui, |ui| {
                    let (rect, response) = ui.allocate_exact_size(vec2(avail_w, total_content_h), egui::Sense::click_and_drag());
                    let painter = ui.painter_at(rect);

                    painter.rect_filled(rect, 0.0, Color32::WHITE);

                    let ppm_to_x = |ppm: f64| -> f32 {
                        let ratio = ((p_max - ppm) / p_span) as f32;
                        rect.min.x + ratio * rect.width()
                    };

                    let x_to_ppm = |x: f32| -> f64 {
                        let ratio = ((x - rect.min.x) / rect.width()).clamp(0.0, 1.0) as f64;
                        p_max - ratio * p_span
                    };

                    // マウス・キーボード操作ハンドリング
                    handle_plot_inputs(
                        ui,
                        &response,
                        state,
                        zoom_mode,
                        drag_start,
                        drag_current,
                        p_span,
                        x_to_ppm,
                        rect,
                        rect.max.y,
                        &visible_items,
                        slot_h,
                        rect.min.y,
                    );

                    // 各スペクトルの描画
                    let selected_id = state.selected_id.clone();
                    for (step_idx, &item_idx) in visible_items.iter().enumerate() {
                        let it = &state.items[item_idx];
                        let slot_y0 = rect.min.y + step_idx as f32 * slot_h;
                        let slot_y1 = slot_y0 + slot_h;
                        let slot_rect = Rect::from_min_max(
                            Pos2::new(rect.min.x + 2.0, slot_y0 + 2.0),
                            Pos2::new(rect.max.x - 2.0, slot_y1 - 2.0),
                        );
                        let base_y = slot_y0 + slot_h * 0.85;
                        let unit_h = slot_h * 0.72;
                        let is_selected = selected_id.as_deref() == Some(&it.id);

                        paint_spectrum_slot(
                            &painter,
                            it,
                            step_idx,
                            false,
                            base_y,
                            unit_h,
                            slot_rect,
                            is_selected,
                            ppm_to_x,
                            p_min,
                            p_max,
                            settings.integral_decimals,
                        );
                    }

                    // Zoom X ラバーバンド描画 (ドラッグ中のみ描画、幅3px超)
                    paint_zoom_rubberband(&painter, *zoom_mode, *drag_start, *drag_current, rect.min.y, rect.max.y);

                    // プロット外枠
                    painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(215)));
                });

            // 2. 最下部固定 共通 PPM 軸 (常に画面下部に固定表示)
            let (axis_rect, _) = ui.allocate_exact_size(vec2(avail_w, axis_h), egui::Sense::hover());
            let axis_painter = ui.painter_at(axis_rect);
            axis_painter.rect_filled(axis_rect, 0.0, Color32::WHITE);
            paint_common_ppm_axis(&axis_painter, axis_rect, axis_rect.min.y + 4.0, p_min, p_max, settings);
        });
    } else {
        // =========================================================
        // モードB: 画面内に全スペクトル収まるように自動フィット (Fit / Overlay)
        // =========================================================
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 0.0, Color32::WHITE);

        let axis_y = rect.max.y - axis_h;
        let plot_top = rect.min.y + 10.0_f32;
        let plot_bottom = axis_y - 10.0_f32;
        let plot_h = (plot_bottom - plot_top).max(20.0);
        let slot_h = if num_vis > 0 { plot_h / num_vis as f32 } else { plot_h };

        let ppm_to_x = |ppm: f64| -> f32 {
            let ratio = ((p_max - ppm) / p_span) as f32;
            rect.min.x + ratio * rect.width()
        };

        let x_to_ppm = |x: f32| -> f64 {
            let ratio = ((x - rect.min.x) / rect.width()).clamp(0.0, 1.0) as f64;
            p_max - ratio * p_span
        };

        // マウス・キーボード操作ハンドリング
        handle_plot_inputs(
            ui,
            &response,
            state,
            zoom_mode,
            drag_start,
            drag_current,
            p_span,
            x_to_ppm,
            rect,
            axis_y,
            &visible_items,
            slot_h,
            plot_top,
        );

        // 各スペクトルの描画
        let selected_id = state.selected_id.clone();
        for (step_idx, &item_idx) in visible_items.iter().enumerate() {
            let it = &state.items[item_idx];
            let is_selected = selected_id.as_deref() == Some(&it.id);

            let (base_y, unit_h, slot_rect) = if is_overlay {
                let by = plot_bottom - 10.0;
                let uh = (plot_bottom - plot_top) * 0.85;
                let sr = Rect::from_min_max(Pos2::new(rect.min.x + 2.0, plot_top), Pos2::new(rect.max.x - 2.0, plot_bottom));
                (by, uh, sr)
            } else {
                let slot_y0 = plot_top + step_idx as f32 * slot_h;
                let slot_y1 = slot_y0 + slot_h;
                let by = slot_y0 + slot_h * 0.85;
                let uh = slot_h * 0.72;
                let sr = Rect::from_min_max(Pos2::new(rect.min.x + 2.0, slot_y0 + 2.0), Pos2::new(rect.max.x - 2.0, slot_y1 - 2.0));
                (by, uh, sr)
            };

            paint_spectrum_slot(
                &painter,
                it,
                step_idx,
                is_overlay,
                base_y,
                unit_h,
                slot_rect,
                is_selected,
                ppm_to_x,
                p_min,
                p_max,
                settings.integral_decimals,
            );
        }

        // 最下部共通 PPM 軸
        paint_common_ppm_axis(&painter, rect, axis_y, p_min, p_max, settings);

        // Zoom X ラバーバンド描画 (ドラッグ中のみ描画、幅3px超)
        paint_zoom_rubberband(&painter, *zoom_mode, *drag_start, *drag_current, rect.min.y, axis_y);

        // プロット外枠
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(215)));
    }
}

/// 単一スペクトルのスロット内描画 (波形、90度回転積分、選択枠、名タグ)
fn paint_spectrum_slot<F: Fn(f64) -> f32>(
    painter: &egui::Painter,
    item: &MultiSpecItem,
    step_idx: usize,
    is_overlay: bool,
    base_y: f32,
    unit_h: f32,
    slot_rect: Rect,
    is_selected: bool,
    ppm_to_x: F,
    p_min: f64,
    p_max: f64,
    integral_decimals: usize,
) {
    let proj = match item.project {
        Some(ref p) => p,
        None => return,
    };
    let ppm = match proj.ppm {
        Some(ref p) => p,
        None => return,
    };
    let spec = match proj.spectrum_real {
        Some(ref s) => s,
        None => return,
    };

    // 1. 選択中スペクトルの強調枠 (薄いハイライト枠)
    if is_selected {
        painter.rect_filled(slot_rect, 4.0, Color32::from_rgba_unmultiplied(13, 110, 253, 15));
        painter.rect_stroke(slot_rect, 4.0, Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(13, 110, 253, 160)));
    }

    let max_val = proj.max_intensity().max(1e-6);
    let top_val = max_val * (100.0 / item.y_scale_max.max(1.0));
    let min_val = -max_val * (item.y_scale_min / 100.0);
    let val_span = (top_val - min_val).max(1e-6);

    let color = Color32::from_rgb(item.color[0], item.color[1], item.color[2]);

    // 2. スペクトル名を左上に配置 (Overlay時は縦に並べて被りを解消、先頭番号は不要)
    let name_font = FontId::new(11.5, FontFamily::Proportional);
    let name_galley = painter.layout_no_wrap(item.name.clone(), name_font, color);
    let name_y = if is_overlay {
        slot_rect.min.y + 4.0 + (step_idx as f32 * 18.0)
    } else {
        (slot_rect.min.y + 4.0).max(slot_rect.min.y + 2.0)
    };
    painter.galley(Pos2::new(slot_rect.min.x + 8.0, name_y), name_galley, color);

    // 3. 波形プロット (高速スクリーンピクセル間引き)
    let n_pts = ppm.len().min(spec.len());
    if n_pts > 1 {
        let mut points: Vec<Pos2> = Vec::with_capacity((slot_rect.width() as usize * 2).min(n_pts));
        let mut prev_screen_x = -9999.0_f32;
        let mut min_y_at_x = f32::INFINITY;
        let mut max_y_at_x = f32::NEG_INFINITY;

        for k in 0..n_pts {
            let p = ppm[k];
            if (p < p_min && p < p_max) || (p > p_min && p > p_max) {
                continue;
            }

            let v = spec[k];
            let sx = ppm_to_x(p);
            let y_norm = ((v - min_val) / val_span) as f32;
            let sy = base_y - y_norm * unit_h;

            let px = sx.floor();
            if (px - prev_screen_x).abs() >= 1.0 {
                if prev_screen_x >= slot_rect.min.x - 10.0 {
                    points.push(Pos2::new(prev_screen_x, min_y_at_x));
                    if (max_y_at_x - min_y_at_x).abs() > 0.5 {
                        points.push(Pos2::new(prev_screen_x, max_y_at_x));
                    }
                }
                prev_screen_x = px;
                min_y_at_x = sy;
                max_y_at_x = sy;
            } else {
                if sy < min_y_at_x { min_y_at_x = sy; }
                if sy > max_y_at_x { max_y_at_x = sy; }
            }
        }

        if prev_screen_x >= slot_rect.min.x - 10.0 && min_y_at_x <= max_y_at_x {
            points.push(Pos2::new(prev_screen_x, min_y_at_x));
            if (max_y_at_x - min_y_at_x).abs() > 0.5 {
                points.push(Pos2::new(prev_screen_x, max_y_at_x));
            }
        }

        if points.len() > 1 {
            let stroke_spec = Stroke::new(if is_selected { 1.5_f32 } else { 1.1_f32 }, color);
            painter.add(PathShape::line(points, stroke_spec));
        }
    }

    // 4. 積分曲線 & 90度回転数値ラベル
    if item.show_integral && !proj.state.integrations.is_empty() {
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
                    let mut intg_pts = Vec::with_capacity(res.ppm.len());
                    let mut min_intg_screen_y = f32::MAX;
                    for ki in 0..res.ppm.len() {
                        let p = res.ppm[ki];
                        let sx = ppm_to_x(p);
                        let y_norm = ((res.curve_y[ki] - min_val) / val_span) as f32;
                        let sy = base_y - y_norm * unit_h;
                        if sy < min_intg_screen_y {
                            min_intg_screen_y = sy;
                        }
                        intg_pts.push(Pos2::new(sx, sy));
                    }
                    let intg_color = Color32::from_rgb(
                        (item.color[0] as f32 * 0.85) as u8,
                        (item.color[1] as f32 * 0.85) as u8,
                        (item.color[2] as f32 * 0.85) as u8,
                    );
                    painter.add(PathShape::line(intg_pts, Stroke::new(1.0_f32, intg_color)));

                    // 積分値テキスト (時計回り90度回転の縦書き配置)
                    let sx_start = ppm_to_x(intg.start_ppm);
                    let sx_end = ppm_to_x(intg.end_ppm);
                    let mid_x = (sx_start + sx_end) * 0.5;
                    let val_text = format!("{:.prec$}", res.normalized_value, prec = integral_decimals);
                    let font_intg = FontId::new(10.5, FontFamily::Proportional);
                    let galley = painter.layout_no_wrap(val_text, font_intg, intg_color);
                    let text_len = galley.size().x;
                    let text_h = galley.size().y;
                    let start_y = (min_intg_screen_y - 4.0 - text_len).max(slot_rect.min.y + 4.0);
                    let text_pos = Pos2::new(mid_x + text_h * 0.5, start_y);
                    let ts = egui::epaint::TextShape::new(text_pos, galley, intg_color)
                        .with_angle(std::f32::consts::FRAC_PI_2);
                    painter.add(ts);
                }
            }
        }
    }
}

/// Zoom X ラバーバンド描画 (ドラッグ中のみ描画、幅が3px未満の縦線は描画しない)
fn paint_zoom_rubberband(
    painter: &egui::Painter,
    zoom_mode: MultiSpecZoomMode,
    drag_start: Option<Pos2>,
    drag_current: Option<Pos2>,
    top_y: f32,
    bottom_y: f32,
) {
    if zoom_mode == MultiSpecZoomMode::ZoomX {
        if let (Some(s), Some(c)) = (drag_start, drag_current) {
            let width = (s.x - c.x).abs();
            if width > 3.0 {
                let band_rect = Rect::from_min_max(
                    Pos2::new(s.x.min(c.x), top_y),
                    Pos2::new(s.x.max(c.x), bottom_y),
                );
                painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(13, 110, 253, 35));
                painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(13, 110, 253)));
            }
        }
    }
}

/// プロット領域に対するキーボード・マウス入力ハンドリング
fn handle_plot_inputs<F: Fn(f32) -> f64>(
    ui: &Ui,
    response: &egui::Response,
    state: &mut MultiSpecState,
    zoom_mode: &mut MultiSpecZoomMode,
    drag_start: &mut Option<Pos2>,
    drag_current: &mut Option<Pos2>,
    p_span: f64,
    x_to_ppm: F,
    _rect: Rect,
    _axis_y: f32,
    visible_items: &[usize],
    slot_h: f32,
    plot_top: f32,
) {
    if !ui.is_enabled() {
        return;
    }

    let is_ctrl = ui.input(|i| i.modifiers.ctrl);
    let is_shift = ui.input(|i| i.modifiers.shift);
    let mut any_changed = false;

    // 1. 上下矢印キー: Y-Scale 調整 (ホイールはスクロール専用とし干渉を防止)
    let up_pressed = ui.input(|i| i.key_pressed(Key::ArrowUp));
    let down_pressed = ui.input(|i| i.key_pressed(Key::ArrowDown));
    if up_pressed || down_pressed {
        if is_shift {
            let delta = if up_pressed { 5.0 } else { -5.0 };
            if is_ctrl {
                state.adjust_all_y_scale_min(delta);
            } else if let Some(item) = state.selected_item_mut() {
                item.y_scale_min = (item.y_scale_min + delta).clamp(0.0, 10000.0);
            }
            any_changed = true;
        } else {
            let factor = if up_pressed { 1.15 } else { 1.0 / 1.15 };
            if is_ctrl {
                state.adjust_all_y_scale(factor);
            } else if let Some(item) = state.selected_item_mut() {
                item.y_scale_max = (item.y_scale_max * factor).clamp(1.0, 10000.0);
            }
            any_changed = true;
        }
    }

    // 3. 左右矢印キー: X軸スクロール (全域範囲外への飛び出しをブロック)
    let (full_min, full_max) = state.get_full_ppm_range();
    let left_pressed = ui.input(|i| i.key_pressed(Key::ArrowLeft) || i.key_down(Key::ArrowLeft));
    let right_pressed = ui.input(|i| i.key_pressed(Key::ArrowRight) || i.key_down(Key::ArrowRight));
    if left_pressed || right_pressed {
        let shift_mult = if is_shift { 3.0 } else { 1.0 };
        let dt_step = (p_span * 0.02 * shift_mult) as f64;
        let dx = if left_pressed { dt_step } else { -dt_step };
        let new_min = state.common_ppm_min + dx;
        let new_max = state.common_ppm_max + dx;
        if new_min >= full_min - 0.5 && new_max <= full_max + 0.5 {
            state.common_ppm_min = new_min;
            state.common_ppm_max = new_max;
        }
    }

    if any_changed {
        state.push_history();
    }

    // 4. クリックによるスペクトル選択
    let pointer_pos = response.hover_pos().or(response.interact_pointer_pos());
    if response.clicked() {
        // 単なるクリックのときはドラッグ用座標をクリア (青い縦線が残らないように)
        *drag_start = None;
        *drag_current = None;

        if let Some(pos) = pointer_pos {
            if !state.is_overlay && !visible_items.is_empty() {
                let slot_idx = (((pos.y - plot_top) / slot_h).floor() as usize).min(visible_items.len() - 1);
                let clicked_item_idx = visible_items[slot_idx];
                state.selected_id = Some(state.items[clicked_item_idx].id.clone());
            } else if let Some(&first_idx) = visible_items.first() {
                if state.selected_id.is_none() {
                    state.selected_id = Some(state.items[first_idx].id.clone());
                }
            }
        }
    }

    // 5. ドラッグ操作 (Zoom X ラバーバンド)
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pos) = pointer_pos {
            *drag_start = Some(pos);
            *drag_current = Some(pos);

            // ドラッグ開始時にもスペクトルを選択
            if !state.is_overlay && !visible_items.is_empty() {
                let slot_idx = (((pos.y - plot_top) / slot_h).floor() as usize).min(visible_items.len() - 1);
                let clicked_item_idx = visible_items[slot_idx];
                state.selected_id = Some(state.items[clicked_item_idx].id.clone());
            }
        }
    }

    if response.dragged_by(egui::PointerButton::Primary) {
        if let Some(pos) = pointer_pos {
            *drag_current = Some(pos);
        }
    }

    if response.drag_stopped_by(egui::PointerButton::Primary) {
        if *zoom_mode == MultiSpecZoomMode::ZoomX {
            if let (Some(s), Some(e)) = (*drag_start, *drag_current) {
                let dx = (s.x - e.x).abs();
                if dx > 10.0 {
                    let p1 = x_to_ppm(s.x);
                    let p2 = x_to_ppm(e.x);
                    state.push_zoom(p1.min(p2), p1.max(p2));
                    state.push_history();
                }
            }
        }
        *drag_start = None;
        *drag_current = None;
    }

    // 6. ダブルクリック または Backspace: 拡大を 1 つ戻す (Undo Zoom)
    let backspace_pressed = ui.input(|i| i.key_pressed(Key::Backspace));
    if response.double_clicked() || backspace_pressed {
        if state.undo_zoom() {
            state.push_history();
        }
    }

    // 7. Home または Ctrl + 0 キー: 全体表示にリセット
    let is_ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
    if ui.input(|i| i.key_pressed(Key::Home) || (is_ctrl && i.key_pressed(Key::Num0))) {
        state.reset_zoom();
        state.push_history();
    }
}

/// 最下部共通 PPM 軸と目盛りの描画
fn paint_common_ppm_axis(
    painter: &egui::Painter,
    rect: Rect,
    axis_y: f32,
    p_min: f64,
    p_max: f64,
    settings: &MultiSpecSettings,
) {
    let stroke_axis = Stroke::new(1.0_f32, Color32::from_rgb(33, 37, 41));
    painter.line_segment([Pos2::new(rect.min.x, axis_y), Pos2::new(rect.max.x, axis_y)], stroke_axis);

    let span = (p_max - p_min).abs();
    if span < 1e-4 {
        return;
    }

    let (step, dec) = crate::core::calc_ppm_ticks(span, settings.auto_ticks, settings.tick_major);

    let font_axis = FontId::new(11.0, FontFamily::Proportional);

    let ppm_to_x = |ppm: f64| -> f32 {
        let ratio = ((p_max - ppm) / span) as f32;
        rect.min.x + ratio * rect.width()
    };

    let minor_n = settings.tick_minor.max(1);
    let minor_step = step / (minor_n as f64);

    let p_low = p_min.min(p_max);
    let p_high = p_min.max(p_max);
    let start_ppm = (p_low / step).floor() * step;
    let end_ppm = (p_high / step).ceil() * step;

    let stroke_sub = Stroke::new(0.8_f32, Color32::from_gray(90));

    let mut current_ppm = start_ppm - step;
    while current_ppm <= end_ppm + step * 0.1 {
        // 主目盛り
        if current_ppm >= p_low - 1e-9 && current_ppm <= p_high + 1e-9 {
            let sx = ppm_to_x(current_ppm);
            if sx >= rect.min.x + 2.0 && sx <= rect.max.x - 2.0 {
                // メイン目盛り (5.0px)
                painter.line_segment([Pos2::new(sx, axis_y), Pos2::new(sx, axis_y + 5.0)], stroke_axis);

                let label = format!("{:.prec$}", current_ppm, prec = dec);
                painter.text(
                    Pos2::new(sx, axis_y + 8.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    font_axis.clone(),
                    Color32::from_rgb(33, 37, 41),
                );
            }
        }

        // サブ目盛り (3.5px / 中央は4.5px, 主目盛りの外側も描画)
        if minor_n > 1 {
            for m in 1..minor_n {
                let sub_ppm = current_ppm + (m as f64) * minor_step;
                if sub_ppm >= p_low - 1e-9 && sub_ppm <= p_high + 1e-9 {
                    let sub_sx = ppm_to_x(sub_ppm);
                    if sub_sx >= rect.min.x && sub_sx <= rect.max.x {
                        let sub_len = if minor_n % 2 == 0 && m == minor_n / 2 { 4.5 } else { 3.5 };
                        painter.line_segment([Pos2::new(sub_sx, axis_y), Pos2::new(sub_sx, axis_y + sub_len)], stroke_sub);
                    }
                }
            }
        }

        current_ppm += step;
    }

    // 単位表示 (ppm)
    painter.text(
        Pos2::new(rect.max.x - 6.0, axis_y + 8.0),
        egui::Align2::RIGHT_TOP,
        "ppm",
        font_axis,
        Color32::from_rgb(108, 117, 125),
    );
}
