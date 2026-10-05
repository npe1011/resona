use egui::{Color32, Context, Key, Pos2, Rect, Stroke};
use crate::core::{
    add_peak_in_range, analyze_multiplet, compute_integral, IntegrationItem, MultiviewItem, RectF,
};
use crate::gui::dialogs::MultiviewYScaleDialogState;
use crate::gui::mode::{AppMode, IntegrateSubMode, MultiviewSubMode, PeakSubMode, ZoomTool};
use crate::gui::plot::PlotTransform;
use super::actions::find_non_overlapping_multiview_pos;
use super::ResonaApp;

/// Integrate Edit モードでのドラッグ対象
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntegrateDragTarget {
    StartHandle(usize),
    EndHandle(usize),
    Scale { start_scale: f64, start_y: f32 },
    Offset { start_offset: f64, start_y: f32 },
}

/// Integrate Edit モードでの曲線の操作種別
#[derive(Debug, Clone, Copy, PartialEq)]
enum IntgEditTarget {
    Scale,
    Offset,
}

/// 点 p から線分 ab への画面上での最短距離
fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq <= 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).length()
}

/// 積分曲線のホバー／ドラッグ対象判定
/// - 積分曲線の上端付近: Scale (拡大率変更)
/// - 積分曲線の線上付近: Offset (平行移動)
/// - 線分への最短距離ベースで判定し、急峻な立ち上がり部や端点付近でも余裕を持って掴めるようにする
fn detect_integrate_edit_target(
    pos: Pos2,
    ppm: f64,
    intg: &IntegrationItem,
    t: &PlotTransform,
    spec: &ndarray::Array1<f64>,
    ppm_arr: &ndarray::Array1<f64>,
    scale: f64,
    offset: f64,
    ref_factor: f64,
) -> Option<IntgEditTarget> {
    // 画面 X 座標での範囲判定 (左右に 16px の余裕マージンを持たせる)
    let s_x = t.ppm_to_screen_x(intg.start_ppm);
    let e_x = t.ppm_to_screen_x(intg.end_ppm);
    let min_x = s_x.min(e_x) - 16.0;
    let max_x = s_x.max(e_x) + 16.0;
    if pos.x < min_x || pos.x > max_x {
        return None;
    }

    let res = compute_integral(spec, ppm_arr, intg, scale, ref_factor, offset)?;
    if res.ppm.len() < 2 || res.curve_y.len() < 2 {
        return None;
    }

    // 曲線の画面座標点列
    let curve_pts: Vec<Pos2> = res.ppm
        .iter()
        .zip(res.curve_y.iter())
        .map(|(&p, &y)| t.data_to_screen(p, y))
        .collect();

    // 曲線全体への最短距離 & 頂点画面位置
    let mut min_dist = f32::INFINITY;
    let mut min_curve_screen_y = f32::INFINITY;
    let mut top_pos = curve_pts[0];

    for i in 0..curve_pts.len() {
        let pt = curve_pts[i];
        if pt.y < min_curve_screen_y {
            min_curve_screen_y = pt.y;
            top_pos = pt;
        }
        if i + 1 < curve_pts.len() {
            let d = dist_to_segment(pos, curve_pts[i], curve_pts[i + 1]);
            if d < min_dist {
                min_dist = d;
            }
        }
    }

    let bl_screen_y = t.data_to_screen(ppm, intg.baseline_y_at(ppm)).y;

    // 1. ベースラインより大幅に下（18px以上下）は判定外（誤操作防止）
    if pos.y > bl_screen_y + 18.0 {
        return None;
    }

    // 2. 曲線より遥か上空（28px以上上）も判定外
    if pos.y < min_curve_screen_y - 28.0 {
        return None;
    }

    // 3. 掴む許容距離 (22px: 従来の14pxから大幅に拡大し掴みやすくする)
    let hit_tolerance = 22.0_f32;
    if min_dist > hit_tolerance && (pos - top_pos).length() > hit_tolerance {
        return None;
    }

    // 4. 積分曲線の上端判定 (頂点付近、または上部近傍) -> Scale
    if (pos - top_pos).length() <= hit_tolerance + 4.0
        || (pos.y <= min_curve_screen_y + 16.0 && min_dist <= hit_tolerance)
    {
        Some(IntgEditTarget::Scale)
    } else {
        // 5. それ以外の積分曲線付近 -> Offset
        Some(IntgEditTarget::Offset)
    }
}

/// Multiview Edit モードでのドラッグ対象
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MultiviewDragMode {
    Move,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone)]
pub struct MultiviewDragItemState {
    pub id: String,
    pub start_rect: RectF,
}

#[derive(Debug, Clone)]
pub struct MultiviewDragState {
    pub item_id: String,
    pub mode: MultiviewDragMode,
    pub start_rect: RectF,
    pub start_pointer: Pos2,
    pub items: Vec<MultiviewDragItemState>,
}

/// インセットの境界判定 (外枠付近でのリサイズ検知)
pub fn detect_multiview_drag_mode(pos: Pos2, geom: &RectF) -> Option<MultiviewDragMode> {
    let r = Rect::from_min_size(Pos2::new(geom.x, geom.y), egui::vec2(geom.w, geom.h));
    let margin = 10.0_f32; // リサイズハンドルの判定幅 (外側8px + 内側2px)
    let outer = r.expand(8.0);
    if !outer.contains(pos) {
        return None;
    }

    let left = (pos.x - r.min.x).abs() <= margin;
    let right = (pos.x - r.max.x).abs() <= margin;
    let top = (pos.y - r.min.y).abs() <= margin;
    let bottom = (pos.y - r.max.y).abs() <= margin;

    if top && left {
        Some(MultiviewDragMode::TopLeft)
    } else if top && right {
        Some(MultiviewDragMode::TopRight)
    } else if bottom && left {
        Some(MultiviewDragMode::BottomLeft)
    } else if bottom && right {
        Some(MultiviewDragMode::BottomRight)
    } else if left {
        Some(MultiviewDragMode::Left)
    } else if right {
        Some(MultiviewDragMode::Right)
    } else if top {
        Some(MultiviewDragMode::Top)
    } else if bottom {
        Some(MultiviewDragMode::Bottom)
    } else {
        Some(MultiviewDragMode::Move)
    }
}

impl ResonaApp {
    /// メインプロット領域におけるマウスインタラクション・ズーム・パン・各モード操作の処理
    pub(crate) fn handle_plot_interaction(
        &mut self,
        ctx: &Context,
        ui: &mut egui::Ui,
        plot_rect: Rect,
        is_modal_active: bool,
    ) {
        let mut do_reset_zoom = false;

        if let Some(ref mut t) = self.transform {
            // マウスインタラクション (クリック & ドラッグ)
            // ※ ホイールズーム・中クリック・右ドラッグは廃止
            let response = ui.allocate_rect(plot_rect, egui::Sense::click_and_drag());
            let pointer_pos = response.hover_pos().or_else(|| ctx.input(|i| i.pointer.latest_pos()));
            let axis_y = t.axis_y();

            if is_modal_active {
                self.drag_start = None;
                self.drag_current = None;
                self.integrate_drag = None;
                self.multiview_drag = None;
                self.is_dragging_threshold = false;
            } else {
                // ダブルクリックで拡大を1つ前に戻す (Zoom モード時)
                if response.double_clicked() && self.active_zoom.is_some() {
                    if let Some((p_min, p_max, y_min, y_max)) = self.zoom_history.pop() {
                        t.ppm_min = p_min;
                        t.ppm_max = p_max;
                        t.y_min = y_min;
                        t.y_max = y_max;
                        self.last_transform_y = Some((t.y_min, t.y_max));
                        self.status_message = "Zoom undone (double-click)".to_string();
                    } else {
                        do_reset_zoom = true;
                        self.status_message = "Zoom reset (double-click)".to_string();
                    }
                }

                // --- キーボード＆マウスホイール操作 ---
                let wants_kbd = ctx.wants_keyboard_input();
                if !wants_kbd {
                    // 1. Backspace で Zoom を 1 つ戻す
                    if ctx.input(|i| i.key_pressed(Key::Backspace)) {
                        if let Some((p_min, p_max, y_min, y_max)) = self.zoom_history.pop() {
                            t.ppm_min = p_min;
                            t.ppm_max = p_max;
                            t.y_min = y_min;
                            t.y_max = y_max;
                            self.last_transform_y = Some((t.y_min, t.y_max));
                            self.status_message = "Zoom undone (Backspace)".to_string();
                        } else {
                            do_reset_zoom = true;
                            self.status_message = "Zoom reset (Backspace)".to_string();
                        }
                    }

                    // 2. Z / X / S で一時 Zoom (押している間だけアクティブ、離すと復帰、他キーや修飾キーなし時限定)
                    let has_modifiers = ctx.input(|i| i.modifiers.any());
                    if !has_modifiers {
                        let z_down = ctx.input(|i| i.key_down(Key::Z));
                        let x_down = ctx.input(|i| i.key_down(Key::X));
                        let s_down = ctx.input(|i| i.key_down(Key::S));

                        if z_down || x_down || s_down {
                            if self.temp_zoom_saved.is_none() {
                                self.temp_zoom_saved = Some(self.active_zoom);
                            }
                            let target_tool = if z_down {
                                ZoomTool::Rect
                            } else if x_down {
                                ZoomTool::X
                            } else {
                                ZoomTool::Y
                            };
                            if self.active_zoom != Some(target_tool) {
                                self.active_zoom = Some(target_tool);
                                self.action_state.clear_submodes();
                            }
                        } else if self.drag_start.is_none() {
                            // ドラッグ中でなければキーを離した瞬間に復帰
                            if let Some(saved) = self.temp_zoom_saved.take() {
                                self.active_zoom = saved;
                            }
                        }
                    } else if self.drag_start.is_none() {
                        if let Some(saved) = self.temp_zoom_saved.take() {
                            self.active_zoom = saved;
                        }
                    }

                    // 3. 矢印キー（← / →）によるデータ範囲内スクロール、および（↑ / ↓）による Y-Scale (%) 調整
                    let is_left = ctx.input(|i| i.key_down(Key::ArrowLeft));
                    let is_right = ctx.input(|i| i.key_down(Key::ArrowRight));
                    let is_up = ctx.input(|i| i.key_down(Key::ArrowUp));
                    let is_down = ctx.input(|i| i.key_down(Key::ArrowDown));

                    let pressed_left = ctx.input(|i| i.key_pressed(Key::ArrowLeft));
                    let pressed_right = ctx.input(|i| i.key_pressed(Key::ArrowRight));
                    let pressed_up = ctx.input(|i| i.key_pressed(Key::ArrowUp));
                    let pressed_down = ctx.input(|i| i.key_pressed(Key::ArrowDown));

                    let any_arrow_down = is_left || is_right || is_up || is_down;
                    let dt = (ctx.input(|i| i.stable_dt) as f64).min(0.1);

                    if any_arrow_down {
                        ctx.request_repaint();
                        self.arrow_key_hold_time += dt;

                        let is_shift = ctx.input(|i| i.modifiers.shift);
                        let shift_mult = if is_shift { 3.0 } else { 1.0 };

                        // 長押し時の加速 (0.15秒後から最大3倍まで加速)
                        let accel_mult = if self.arrow_key_hold_time > 0.15 {
                            let progress = ((self.arrow_key_hold_time - 0.15) / 0.8).clamp(0.0, 1.0);
                            1.0 + (progress * progress) * 2.0
                        } else {
                            1.0
                        };

                        // 3a. 左右キー: データ範囲 (PPM端) を超えないようにスクロール & クランプ
                        if is_left || is_right {
                            // データ全体の最小・最大 PPM を算出
                            let (data_min_ppm, data_max_ppm) = match self.project.ppm {
                                Some(ref p) if !p.is_empty() => {
                                    let mut mn = p[0];
                                    let mut mx = p[0];
                                    for &val in p.iter() {
                                        if val < mn { mn = val; }
                                        if val > mx { mx = val; }
                                    }
                                    (mn, mx)
                                }
                                _ => (-10.0, 200.0),
                            };

                            let cur_min = t.ppm_min.min(t.ppm_max);
                            let cur_max = t.ppm_min.max(t.ppm_max);
                            let view_width = cur_max - cur_min;
                            let data_width = data_max_ppm - data_min_ppm;

                            // 表示幅がデータ幅未満のときのみスクロールを許可し、端でクランプ
                            if view_width < data_width - 1e-5 {
                                let x_span = (t.ppm_max - t.ppm_min).abs();
                                let continuous_rate = 0.6 * shift_mult * accel_mult * dt;
                                let step_x_cont = x_span * continuous_rate;
                                let discrete_rate = 0.05 * shift_mult;
                                let step_x_disc = x_span * discrete_rate;

                                let dx = if is_left {
                                    if pressed_left { step_x_disc } else { step_x_cont }
                                } else {
                                    -(if pressed_right { step_x_disc } else { step_x_cont })
                                };

                                t.ppm_min += dx;
                                t.ppm_max += dx;

                                let is_desc = t.ppm_min > t.ppm_max;
                                let new_min = t.ppm_min.min(t.ppm_max);
                                let new_max = t.ppm_min.max(t.ppm_max);

                                if new_max > data_max_ppm {
                                    let clamped_max = data_max_ppm;
                                    let clamped_min = data_max_ppm - view_width;
                                    if is_desc {
                                        t.ppm_min = clamped_max;
                                        t.ppm_max = clamped_min;
                                    } else {
                                        t.ppm_min = clamped_min;
                                        t.ppm_max = clamped_max;
                                    }
                                } else if new_min < data_min_ppm {
                                    let clamped_min = data_min_ppm;
                                    let clamped_max = data_min_ppm + view_width;
                                    if is_desc {
                                        t.ppm_min = clamped_max;
                                        t.ppm_max = clamped_min;
                                    } else {
                                        t.ppm_min = clamped_min;
                                        t.ppm_max = clamped_max;
                                    }
                                }
                            }
                        }

                        // 3b. 上下キー: Y-Scale (%) Max / Min 調整
                        if is_up || is_down {
                            let max_intensity = self.project.max_intensity().max(1e-6);
                            if is_shift {
                                // Shift + 上下: Y-scale Min(%) を加速調整
                                let step = if pressed_up || pressed_down { 2.0 } else { 25.0 * accel_mult * dt };
                                if is_up {
                                    self.y_min_scale = (self.y_min_scale + step).clamp(0.0, 10000.0);
                                } else {
                                    self.y_min_scale = (self.y_min_scale - step).clamp(0.0, 10000.0);
                                }
                            } else {
                                // 上下: Y-scale Max(%) を加速調整
                                let step = if pressed_up || pressed_down { 2.5 } else { 35.0 * accel_mult * dt };
                                if is_up {
                                    self.y_max_scale = (self.y_max_scale + step).clamp(1.0, 10000.0);
                                } else {
                                    self.y_max_scale = (self.y_max_scale - step).clamp(1.0, 10000.0);
                                }
                            }
                            let top_pct = self.y_max_scale.max(1.0);
                            t.y_max = max_intensity * (100.0 / top_pct);
                            t.y_min = -max_intensity * (self.y_min_scale / 100.0);
                            self.last_transform_y = Some((t.y_min, t.y_max));
                        }
                    } else {
                        self.arrow_key_hold_time = 0.0;
                    }
                }

                // 4. マウスホイール (通常: Y-scale Max, Shift: Y-scale Min)
                if response.hovered() {
                    let raw_delta = ctx.input(|i| i.raw_scroll_delta);
                    let is_shift = ctx.input(|i| i.modifiers.shift);
                    // Shift 押下時は OS により上下ホイールが横スクロール (x) に変換される場合があるため、
                    // x と y のうち絶対値が大きい方を採用する
                    let scroll_val = if is_shift {
                        if raw_delta.x.abs() > raw_delta.y.abs() {
                            raw_delta.x as f64
                        } else {
                            raw_delta.y as f64
                        }
                    } else {
                        raw_delta.y as f64
                    };

                    if scroll_val.abs() > 0.1 {
                        let max_intensity = self.project.max_intensity().max(1e-6);
                        if is_shift {
                            // Shift + ホイール: Min (%) 調整
                            let delta = (scroll_val / 30.0).clamp(-10.0, 10.0) * 2.0;
                            let new_min = (self.y_min_scale + delta).clamp(0.0, 10000.0);
                            if (new_min - self.y_min_scale).abs() > 1e-4 {
                                self.y_min_scale = new_min;
                                let top_pct = self.y_max_scale.max(1.0);
                                t.y_max = max_intensity * (100.0 / top_pct);
                                t.y_min = -max_intensity * (self.y_min_scale / 100.0);
                                self.last_transform_y = Some((t.y_min, t.y_max));
                            }
                        } else {
                            // 通常ホイール: Max (%) 調整
                            let factor = 1.10_f64.powf(scroll_val / 50.0);
                            let new_max_scale = (self.y_max_scale * factor).clamp(1.0, 10000.0);
                            if (new_max_scale - self.y_max_scale).abs() > 1e-4 {
                                self.y_max_scale = new_max_scale;
                                let top_pct = self.y_max_scale.max(1.0);
                                t.y_max = max_intensity * (100.0 / top_pct);
                                t.y_min = -max_intensity * (self.y_min_scale / 100.0);
                                self.last_transform_y = Some((t.y_min, t.y_max));
                            }
                        }
                    }
                }

                // Phase モード時の判定
                let is_phase_mode = self.mode == Some(AppMode::Phase) && self.active_zoom.is_none();
                let is_phase_pivot_mode = is_phase_mode && self.action_state.phase_pivot_active;
                if is_phase_pivot_mode {
                    if let Some(pos) = pointer_pos {
                        if plot_rect.contains(pos) && pos.y <= axis_y {
                            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                            let shift_down = ctx.input(|i| i.modifiers.shift);
                            if shift_down {
                                self.status_message = "Click to set pivot (Free position; Shift held)".to_string();
                            } else {
                                self.status_message = "Click to set pivot (Peak snap enabled; hold Shift to disable)".to_string();
                            }
                        }
                    }
                }

                // スレッショルドバーのホバー／ドラッグ判定 (Peak pick モード時)
                let is_peak_mode = self.mode == Some(AppMode::Peak) && self.active_zoom.is_none();
                let is_thresh_submode = is_peak_mode && self.action_state.peak_submode == PeakSubMode::Threshold;
                let is_integrate_edit_mode = self.mode == Some(AppMode::Integrate)
                    && self.action_state.integrate_submode == IntegrateSubMode::Edit
                    && self.active_zoom.is_none();
                let is_integrate_delete_mode = self.mode == Some(AppMode::Integrate)
                    && self.action_state.integrate_submode == IntegrateSubMode::Delete
                    && self.active_zoom.is_none();
                let is_multiview_mode = self.mode == Some(AppMode::Multiview) && self.active_zoom.is_none();
                let is_multiview_delete_mode = is_multiview_mode && self.action_state.multiview_submode == MultiviewSubMode::Delete;
                let thresh_val = self.action_state.peak_threshold;
                let mut near_threshold = false;

                // Multiview ホバー判定 (外側8px枠線ゾーンまで検知)
                self.hovered_multiview_id = None;
                if is_multiview_mode {
                    if let Some(pos) = pointer_pos {
                        for mv in self.project.state.multiviews.iter().rev() {
                            if is_multiview_delete_mode {
                                let rect = Rect::from_min_size(Pos2::new(mv.geometry.x, mv.geometry.y), egui::vec2(mv.geometry.w, mv.geometry.h));
                                if rect.expand(6.0).contains(pos) {
                                    self.hovered_multiview_id = Some(mv.id.clone());
                                    ctx.set_cursor_icon(egui::CursorIcon::Default);
                                    break;
                                }
                            } else {
                                if let Some(mode) = detect_multiview_drag_mode(pos, &mv.geometry) {
                                    self.hovered_multiview_id = Some(mv.id.clone());
                                    let cursor = match mode {
                                        MultiviewDragMode::TopLeft | MultiviewDragMode::BottomRight => egui::CursorIcon::ResizeNwSe,
                                        MultiviewDragMode::TopRight | MultiviewDragMode::BottomLeft => egui::CursorIcon::ResizeNeSw,
                                        MultiviewDragMode::Left | MultiviewDragMode::Right => egui::CursorIcon::ResizeHorizontal,
                                        MultiviewDragMode::Top | MultiviewDragMode::Bottom => egui::CursorIcon::ResizeVertical,
                                        MultiviewDragMode::Move => egui::CursorIcon::Move,
                                    };
                                    ctx.set_cursor_icon(cursor);
                                    break;
                                }
                            }
                        }
                    }
                }

                if is_peak_mode {
                    if let Some(pos) = pointer_pos {
                        let in_plot = plot_rect.contains(pos) && pos.y <= axis_y && response.hovered();
                        if is_thresh_submode {
                            if in_plot {
                                near_threshold = true;
                                ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                            }
                        } else if thresh_val > 0.0 && in_plot {
                            let sy_pos = t.y_to_screen_y(thresh_val);
                            let sy_neg = t.y_to_screen_y(-thresh_val);
                            if (pos.y - sy_pos).abs() <= 7.0 || (pos.y - sy_neg).abs() <= 7.0 {
                                near_threshold = true;
                                ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                            }
                        }
                    }
                    if self.is_dragging_threshold {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }
                }

                if response.double_clicked() {
                    if let Some(pos) = pointer_pos {
                        if is_multiview_mode && !is_multiview_delete_mode {
                            for mv in self.project.state.multiviews.iter().rev() {
                                let rect = Rect::from_min_size(Pos2::new(mv.geometry.x, mv.geometry.y), egui::vec2(mv.geometry.w, mv.geometry.h));
                                if rect.contains(pos) {
                                    let p_min = mv.src_x_min.min(mv.src_x_max);
                                    let p_max = mv.src_x_min.max(mv.src_x_max);
                                    let inset_max = if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                        let mut mx = 0.0_f64;
                                        for i in 0..ppm.len().min(spec.len()) {
                                            let p = ppm[i];
                                            if p >= p_min && p <= p_max {
                                                if spec[i] > mx {
                                                    mx = spec[i];
                                                }
                                            }
                                        }
                                        if mx > 1e-6 { mx } else { self.project.max_intensity().max(1.0) }
                                    } else {
                                        1.0
                                    };

                                    let (y_max_scale, y_min_scale) = if let (Some(cur_ymax), Some(cur_ymin)) = (mv.src_y_max, mv.src_y_min) {
                                        let top = if cur_ymax > 1e-6 { (100.0 * inset_max / cur_ymax).clamp(1.0, 10000.0) } else { 80.0 };
                                        let min = (100.0 * (-cur_ymin) / inset_max).clamp(0.0, 10000.0);
                                        (top, min)
                                    } else {
                                        (80.0, 10.0)
                                    };

                                    self.multiview_yscale_dialog_state = MultiviewYScaleDialogState {
                                        open: true,
                                        target_id: Some(mv.id.clone()),
                                        target_label: format!("Inset: {:.3} ~ {:.3} ppm", mv.src_x_max, mv.src_x_min),
                                        auto_y: mv.src_y_max.is_none(),
                                        max_peak_intensity: inset_max,
                                        y_max_scale,
                                        y_min_scale,
                                    };
                                    break;
                                }
                            }
                        }
                    }
                } else if response.clicked_by(egui::PointerButton::Primary) {
                    if let Some(pos) = pointer_pos {
                        if plot_rect.contains(pos) && pos.y <= axis_y {
                            let (click_ppm, _) = t.screen_to_data(pos);
                            if is_phase_pivot_mode {
                                let shift_down = ctx.input(|i| i.modifiers.shift);
                                let target_ppm = if shift_down {
                                    click_ppm
                                } else if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                    let p1 = t.screen_to_data(Pos2::new(pos.x - 20.0, pos.y)).0;
                                    let p2 = t.screen_to_data(Pos2::new(pos.x + 20.0, pos.y)).0;
                                    crate::core::signal::phase::find_highest_peak_in_range(spec, ppm, p1, p2).unwrap_or(click_ppm)
                                } else {
                                    click_ppm
                                };
                                self.project.set_pivot_ppm(target_ppm);
                                self.project.push_history();
                                self.is_dirty = true;
                                self.status_message = format!("Set pivot to {:.3} ppm", self.project.pivot_ppm());
                            } else if is_thresh_submode {
                                let new_thresh = t.screen_y_to_y(pos.y).abs();
                                self.action_state.peak_threshold = new_thresh;
                                self.project.state.peak_threshold = Some(new_thresh);
                            } else if is_multiview_mode {
                                if is_multiview_delete_mode {
                                    if let Some(ref hid) = self.hovered_multiview_id.clone() {
                                        self.project.state.multiviews.retain(|m| &m.id != hid);
                                        self.selected_multiview_ids.remove(hid);
                                        self.project.push_history();
                                        self.is_dirty = true;
                                        self.status_message = "Deleted multiview inset".to_string();
                                    }
                                } else {
                                    let is_shift = ctx.input(|i| i.modifiers.shift);
                                    if let Some(ref hid) = self.hovered_multiview_id {
                                        if is_shift {
                                            // Shift+クリック: 複数選択のトグル
                                            if self.selected_multiview_ids.contains(hid) {
                                                self.selected_multiview_ids.remove(hid);
                                            } else {
                                                self.selected_multiview_ids.insert(hid.clone());
                                            }
                                        } else {
                                            // 通常クリック: 単一選択
                                            self.selected_multiview_ids.clear();
                                            self.selected_multiview_ids.insert(hid.clone());
                                        }
                                    } else if !is_shift {
                                        // 何もないところを通常クリックした場合は選択全解除
                                        self.selected_multiview_ids.clear();
                                    }
                                }
                            } else if self.mode == Some(AppMode::Peak) && self.active_zoom.is_none() {
                                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                    let dt = if ppm.len() > 1 { (ppm[0] - ppm[ppm.len() - 1]).abs() / (ppm.len() - 1) as f64 } else { 0.001 };
                                    match self.action_state.peak_submode {
                                        PeakSubMode::Add => {
                                            let p_low = click_ppm - dt * 5.0;
                                            let p_high = click_ppm + dt * 5.0;
                                            let before_count = self.project.state.peaks.len();
                                            self.project.state.peaks = add_peak_in_range(spec, ppm, p_low, p_high, &self.project.state.peaks);
                                            if self.project.state.peaks.len() > before_count {
                                                self.project.push_history();
                                                self.is_dirty = true;
                                                self.status_message = format!("Added peak near {:.3} ppm", click_ppm);
                                            }
                                        }
                                        PeakSubMode::Delete => {
                                            let tol = dt * 5.0;
                                            let before_count = self.project.state.peaks.len();
                                            self.project.state.peaks.retain(|pk| (pk.ppm - click_ppm).abs() > tol);
                                            if self.project.state.peaks.len() < before_count {
                                                self.project.push_history();
                                                self.is_dirty = true;
                                                self.status_message = format!("Deleted peak near {:.3} ppm", click_ppm);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            } else if self.mode == Some(AppMode::Integrate) && self.active_zoom.is_none() {
                                match self.action_state.integrate_submode {
                                    IntegrateSubMode::Delete => {
                                        let before_count = self.project.state.integrations.len();
                                        self.project.state.integrations.retain(|item| !item.contains_ppm(click_ppm));
                                        if self.project.state.integrations.len() < before_count {
                                            self.project.push_history();
                                            self.is_dirty = true;
                                            self.status_message = "Deleted integration".to_string();
                                        }
                                    }
                                    IntegrateSubMode::Split => {
                                        let mut split_idx = None;
                                        for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                            if item.contains_ppm(click_ppm) {
                                                split_idx = Some(idx);
                                                break;
                                            }
                                        }
                                        if let Some(idx) = split_idx {
                                            let item = self.project.state.integrations.remove(idx);
                                            let (x1, x2) = (item.start_ppm, item.end_ppm);
                                            let (y1, y2) = (item.y_start, item.y_end);
                                            let y_split = item.baseline_y_at(click_ppm);
                                            let d1 = IntegrationItem {
                                                id: format!("intg-{}", self.project.state.integrations.len() + 1),
                                                start_ppm: x1,
                                                end_ppm: click_ppm,
                                                y_start: y1,
                                                y_end: y_split,
                                            };
                                            let d2 = IntegrationItem {
                                                id: format!("intg-{}", self.project.state.integrations.len() + 2),
                                                start_ppm: click_ppm,
                                                end_ppm: x2,
                                                y_start: y_split,
                                                y_end: y2,
                                            };
                                            self.project.state.integrations.insert(idx, d2);
                                            self.project.state.integrations.insert(idx, d1);
                                            self.project.push_history();
                                            self.is_dirty = true;
                                            self.status_message = format!("Split integration at {:.3} ppm", click_ppm);
                                        }
                                    }
                                    IntegrateSubMode::Reference => {
                                        let mut target_idx = None;
                                        for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                            if item.contains_ppm(click_ppm) {
                                                target_idx = Some(idx);
                                                break;
                                            }
                                        }
                                        if let Some(idx) = target_idx {
                                            if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                let item = &self.project.state.integrations[idx];
                                                if let Some(res) = compute_integral(spec, ppm, item, 1.0, 1.0, 0.03) {
                                                    if res.total_area > 1e-12 {
                                                        let target_val = self.action_state.integration_ref_val;
                                                        self.project.state.integration_ref_area = res.total_area;
                                                        self.project.state.integration_ref_value = target_val;
                                                        self.project.push_history();
                                                        self.is_dirty = true;
                                                        self.status_message = format!("Set reference integral to {:.2}", target_val);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                // Multiview 選択中の Delete キー削除 (ezNMR lines 401-407準拠)
                if ctx.input(|i| i.key_pressed(Key::Delete))
                    && is_multiview_mode
                    && !self.selected_multiview_ids.is_empty()
                {
                    let count = self.selected_multiview_ids.len();
                    self.project.state.multiviews.retain(|m| !self.selected_multiview_ids.contains(&m.id));
                    self.selected_multiview_ids.clear();
                    self.project.push_history();
                    self.is_dirty = true;
                    self.status_message = format!("Deleted {} multiview inset(s) (Delete key)", count);
                }

                if let Some(pos) = pointer_pos {
                    let (cur_ppm, cur_y) = t.screen_to_data(pos);
                    self.status_message = format!("PPM {:.3}   Intensity {:.1}", cur_ppm, cur_y);

                    // Delete サブモード時のホバー判定
                    if is_integrate_delete_mode && plot_rect.contains(pos) && pos.y <= axis_y {
                        if let Some(item) = self.project.state.integrations.iter().find(|it| it.contains_ppm(cur_ppm)) {
                            ctx.set_cursor_icon(egui::CursorIcon::Default);
                            let p_min = item.start_ppm.min(item.end_ppm);
                            let p_max = item.start_ppm.max(item.end_ppm);
                            self.status_message = format!("Click to delete integration [{:.2} ~ {:.2} ppm]", p_min, p_max);
                        }
                    }

                    // Edit サブモード時のホバー判定
                    if is_integrate_edit_mode && !self.is_dragging_threshold && self.integrate_drag.is_none() && plot_rect.contains(pos) && pos.y <= axis_y {
                        let mut hit_handle = false;
                        for intg in &self.project.state.integrations {
                            let s_pos = t.data_to_screen(intg.start_ppm, intg.y_start);
                            let e_pos = t.data_to_screen(intg.end_ppm, intg.y_end);
                            let hit_s = (pos - s_pos).length() <= 18.0 || ((pos.x - s_pos.x).abs() <= 16.0 && (pos.y - s_pos.y).abs() <= 20.0);
                            let hit_e = (pos - e_pos).length() <= 18.0 || ((pos.x - e_pos.x).abs() <= 16.0 && (pos.y - e_pos.y).abs() <= 20.0);
                            if hit_s || hit_e {
                                hit_handle = true;
                                break;
                            }
                        }
                        if hit_handle {
                            ctx.set_cursor_icon(egui::CursorIcon::Grab);
                            self.status_message = "Drag to adjust baseline handle position".to_string();
                        } else if let (Some(spec), Some(ppm_arr)) = (&self.project.spectrum_real, &self.project.ppm) {
                            let ref_factor = self.project.state.integration_ref_factor();
                            let mut detected = None;
                            for intg in &self.project.state.integrations {
                                if let Some(target) = detect_integrate_edit_target(
                                    pos,
                                    cur_ppm,
                                    intg,
                                    t,
                                    spec,
                                    ppm_arr,
                                    self.project.state.integration_scale,
                                    self.project.state.integration_offset,
                                    ref_factor,
                                ) {
                                    detected = Some(target);
                                    break;
                                }
                            }
                            match detected {
                                Some(IntgEditTarget::Scale) => {
                                    ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                    self.status_message = "Drag up/down to adjust integration scale (height)".to_string();
                                }
                                Some(IntgEditTarget::Offset) => {
                                    ctx.set_cursor_icon(egui::CursorIcon::Move);
                                    self.status_message = "Drag up/down to adjust integration vertical offset".to_string();
                                }
                                None => {}
                            }
                        }
                    }
                }

                // 非ドラッグ時: Split サブモードではマウス位置に縦の青色ガイド線を表示
                if self.mode == Some(AppMode::Integrate) && self.action_state.integrate_submode == IntegrateSubMode::Split && self.drag_start.is_none() {
                    if let Some(pos) = pointer_pos {
                        if plot_rect.contains(pos) && pos.y <= axis_y {
                            let painter = ui.painter_at(plot_rect);
                            painter.line_segment(
                                [Pos2::new(pos.x, plot_rect.min.y), Pos2::new(pos.x, axis_y)],
                                Stroke::new(1.5_f32, Color32::from_rgb(13, 110, 253)),
                            );
                        }
                    }
                }

                // 非ドラッグ時: Delete サブモードではマウス位置に縦の赤色ガイド線を表示
                if is_integrate_delete_mode && self.drag_start.is_none() {
                    if let Some(pos) = pointer_pos {
                        if plot_rect.contains(pos) && pos.y <= axis_y {
                            let painter = ui.painter_at(plot_rect);
                            painter.line_segment(
                                [Pos2::new(pos.x, plot_rect.min.y), Pos2::new(pos.x, axis_y)],
                                Stroke::new(1.2_f32, Color32::from_rgb(239, 68, 68)),
                            );
                        }
                    }
                }

                // ドラッグ開始 (押下した瞬間の正確な原点座標を取得して遅れを解消)
                if response.drag_started_by(egui::PointerButton::Primary) {
                    if is_phase_pivot_mode {
                        self.is_dragging_pivot = true;
                    } else if near_threshold {
                        self.is_dragging_threshold = true;
                        if let Some(pos) = pointer_pos {
                            let new_thresh = t.screen_y_to_y(pos.y).abs();
                            self.action_state.peak_threshold = new_thresh;
                            self.project.state.peak_threshold = Some(new_thresh);
                        }
                    } else if is_multiview_mode && !is_multiview_delete_mode {
                        self.is_dragging_threshold = false;
                        let origin = ctx.input(|i| i.pointer.press_origin()).or(pointer_pos);
                        let mut hit_mv = None;
                        if let Some(pos) = origin {
                            for mv in self.project.state.multiviews.iter().rev() {
                                if let Some(mode) = detect_multiview_drag_mode(pos, &mv.geometry) {
                                    hit_mv = Some((mv.id.clone(), mode, mv.geometry));
                                    break;
                                }
                            }
                        }
                        if let (Some(pos), Some((m_id, mode, geom))) = (origin, hit_mv) {
                            let is_shift = ctx.input(|i| i.modifiers.shift);
                            if !self.selected_multiview_ids.contains(&m_id) {
                                if !is_shift {
                                    self.selected_multiview_ids.clear();
                                }
                                self.selected_multiview_ids.insert(m_id.clone());
                            }

                            // 移動モードの場合は選択中の全アイテムを初期位置とともに保持
                            let items = if mode == MultiviewDragMode::Move {
                                self.project
                                    .state
                                    .multiviews
                                    .iter()
                                    .filter(|m| self.selected_multiview_ids.contains(&m.id))
                                    .map(|m| MultiviewDragItemState {
                                        id: m.id.clone(),
                                        start_rect: m.geometry,
                                    })
                                    .collect()
                            } else {
                                vec![MultiviewDragItemState {
                                    id: m_id.clone(),
                                    start_rect: geom,
                                }]
                            };

                            self.multiview_drag = Some(MultiviewDragState {
                                item_id: m_id,
                                mode,
                                start_rect: geom,
                                start_pointer: pos,
                                items,
                            });
                        } else {
                            self.drag_start = origin;
                            self.drag_current = pointer_pos;
                        }
                    } else if is_integrate_edit_mode {
                        self.is_dragging_threshold = false;
                        if let Some(pos) = pointer_pos {
                            let mut hit_handle = None;
                            for (idx, intg) in self.project.state.integrations.iter().enumerate() {
                                let s_pos = t.data_to_screen(intg.start_ppm, intg.y_start);
                                let e_pos = t.data_to_screen(intg.end_ppm, intg.y_end);
                                let hit_s = (pos - s_pos).length() <= 18.0 || ((pos.x - s_pos.x).abs() <= 16.0 && (pos.y - s_pos.y).abs() <= 20.0);
                                let hit_e = (pos - e_pos).length() <= 18.0 || ((pos.x - e_pos.x).abs() <= 16.0 && (pos.y - e_pos.y).abs() <= 20.0);
                                if hit_s {
                                    hit_handle = Some(IntegrateDragTarget::StartHandle(idx));
                                    break;
                                } else if hit_e {
                                    hit_handle = Some(IntegrateDragTarget::EndHandle(idx));
                                    break;
                                }
                            }
                            if let Some(target) = hit_handle {
                                self.integrate_drag = Some(target);
                            } else if let (Some(spec), Some(ppm_arr)) = (&self.project.spectrum_real, &self.project.ppm) {
                                let (ppm, _) = t.screen_to_data(pos);
                                let ref_factor = self.project.state.integration_ref_factor();
                                let mut detected = None;
                                for intg in &self.project.state.integrations {
                                    if let Some(target) = detect_integrate_edit_target(
                                        pos,
                                        ppm,
                                        intg,
                                        t,
                                        spec,
                                        ppm_arr,
                                        self.project.state.integration_scale,
                                        self.project.state.integration_offset,
                                        ref_factor,
                                    ) {
                                        detected = Some(target);
                                        break;
                                    }
                                }
                                match detected {
                                    Some(IntgEditTarget::Scale) => {
                                        self.integrate_drag = Some(IntegrateDragTarget::Scale {
                                            start_scale: self.project.state.integration_scale,
                                            start_y: pos.y,
                                        });
                                    }
                                    Some(IntgEditTarget::Offset) => {
                                        self.integrate_drag = Some(IntegrateDragTarget::Offset {
                                            start_offset: self.project.state.integration_offset,
                                            start_y: pos.y,
                                        });
                                    }
                                    None => {
                                        self.integrate_drag = None;
                                    }
                                }
                            }
                        }
                    } else {
                        self.is_dragging_threshold = false;
                        self.is_dragging_pivot = false;
                        let origin = ctx.input(|i| i.pointer.press_origin()).or(pointer_pos);
                        self.drag_start = origin;
                        self.drag_current = pointer_pos;
                    }
                }

                // ドラッグ中
                if response.dragged_by(egui::PointerButton::Primary) {
                    if self.is_dragging_pivot {
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                        if let Some(pos) = pointer_pos {
                            let (cur_ppm, _) = t.screen_to_data(pos);
                            let shift_down = ctx.input(|i| i.modifiers.shift);
                            let preview_ppm = if shift_down {
                                cur_ppm
                            } else if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                let p1 = t.screen_to_data(Pos2::new(pos.x - 20.0, pos.y)).0;
                                let p2 = t.screen_to_data(Pos2::new(pos.x + 20.0, pos.y)).0;
                                crate::core::signal::phase::find_highest_peak_in_range(spec, ppm, p1, p2).unwrap_or(cur_ppm)
                            } else {
                                cur_ppm
                            };
                            self.status_message = format!("Pivot preview: {:.3} ppm (Release to set)", preview_ppm);
                        }
                    } else if self.is_dragging_threshold {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        if let Some(pos) = pointer_pos {
                            let new_thresh = t.screen_y_to_y(pos.y).abs();
                            self.action_state.peak_threshold = new_thresh;
                            self.project.state.peak_threshold = Some(new_thresh);
                        }
                    } else if let Some(ref drag) = self.multiview_drag {
                        if let Some(pos) = pointer_pos {
                            let delta = pos - drag.start_pointer;
                            let is_shift = ctx.input(|i| i.modifiers.shift);

                            if drag.mode == MultiviewDragMode::Move {
                                // 拡大図の直角移動 (Shift を押しながら)
                                let effective_delta = if is_shift {
                                    if delta.x.abs() >= delta.y.abs() {
                                        egui::vec2(delta.x, 0.0) // 水平移動
                                    } else {
                                        egui::vec2(0.0, delta.y) // 垂直移動
                                    }
                                } else {
                                    delta
                                };

                                // 複数選択されたすべての拡大図を同時に移動
                                for item in &drag.items {
                                    if let Some(mv) = self.project.state.multiviews.iter_mut().find(|m| m.id == item.id) {
                                        mv.geometry.x = item.start_rect.x + effective_delta.x;
                                        mv.geometry.y = item.start_rect.y + effective_delta.y;
                                    }
                                }
                            } else if let Some(mv) = self.project.state.multiviews.iter_mut().find(|m| m.id == drag.item_id) {
                                let mut r = drag.start_rect;
                                if is_shift {
                                    // アスペクト比維持の拡大縮小
                                    let aspect = (drag.start_rect.w / drag.start_rect.h.max(1e-3)) as f32;
                                    let orig_w = drag.start_rect.w;
                                    let orig_h = drag.start_rect.h;

                                    match drag.mode {
                                        MultiviewDragMode::Move => unreachable!(),
                                        MultiviewDragMode::Right => {
                                            let new_w = (orig_w + delta.x).max(60.0);
                                            let new_h = (new_w / aspect).max(60.0);
                                            let actual_w = new_h * aspect;
                                            r.x = drag.start_rect.x;
                                            r.y = drag.start_rect.y + (orig_h - new_h) * 0.5;
                                            r.w = actual_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::Left => {
                                            let new_w = (orig_w - delta.x).max(60.0);
                                            let new_h = (new_w / aspect).max(60.0);
                                            let actual_w = new_h * aspect;
                                            r.x = drag.start_rect.x + orig_w - actual_w;
                                            r.y = drag.start_rect.y + (orig_h - new_h) * 0.5;
                                            r.w = actual_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::Bottom => {
                                            let new_h = (orig_h + delta.y).max(60.0);
                                            let new_w = (new_h * aspect).max(60.0);
                                            let actual_h = new_w / aspect;
                                            r.y = drag.start_rect.y;
                                            r.x = drag.start_rect.x + (orig_w - new_w) * 0.5;
                                            r.w = new_w;
                                            r.h = actual_h;
                                        }
                                        MultiviewDragMode::Top => {
                                            let new_h = (orig_h - delta.y).max(60.0);
                                            let new_w = (new_h * aspect).max(60.0);
                                            let actual_h = new_w / aspect;
                                            r.y = drag.start_rect.y + orig_h - actual_h;
                                            r.x = drag.start_rect.x + (orig_w - new_w) * 0.5;
                                            r.w = new_w;
                                            r.h = actual_h;
                                        }
                                        MultiviewDragMode::TopLeft => {
                                            let dx = -delta.x;
                                            let dy = -delta.y;
                                            let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                            let mut new_w = (orig_w + d).max(60.0);
                                            let mut new_h = new_w / aspect;
                                            if new_h < 60.0 {
                                                new_h = 60.0;
                                                new_w = new_h * aspect;
                                            }
                                            r.x = drag.start_rect.x + orig_w - new_w;
                                            r.y = drag.start_rect.y + orig_h - new_h;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::TopRight => {
                                            let dx = delta.x;
                                            let dy = -delta.y;
                                            let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                            let mut new_w = (orig_w + d).max(60.0);
                                            let mut new_h = new_w / aspect;
                                            if new_h < 60.0 {
                                                new_h = 60.0;
                                                new_w = new_h * aspect;
                                            }
                                            r.x = drag.start_rect.x;
                                            r.y = drag.start_rect.y + orig_h - new_h;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::BottomLeft => {
                                            let dx = -delta.x;
                                            let dy = delta.y;
                                            let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                            let mut new_w = (orig_w + d).max(60.0);
                                            let mut new_h = new_w / aspect;
                                            if new_h < 60.0 {
                                                new_h = 60.0;
                                                new_w = new_h * aspect;
                                            }
                                            r.x = drag.start_rect.x + orig_w - new_w;
                                            r.y = drag.start_rect.y;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::BottomRight => {
                                            let dx = delta.x;
                                            let dy = delta.y;
                                            let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                            let mut new_w = (orig_w + d).max(60.0);
                                            let mut new_h = new_w / aspect;
                                            if new_h < 60.0 {
                                                new_h = 60.0;
                                                new_w = new_h * aspect;
                                            }
                                            r.x = drag.start_rect.x;
                                            r.y = drag.start_rect.y;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                    }
                                } else {
                                    match drag.mode {
                                        MultiviewDragMode::Move => unreachable!(),
                                        MultiviewDragMode::Left => {
                                            let new_w = (r.w - delta.x).max(60.0);
                                            r.x += r.w - new_w;
                                            r.w = new_w;
                                        }
                                        MultiviewDragMode::Right => {
                                            r.w = (r.w + delta.x).max(60.0);
                                        }
                                        MultiviewDragMode::Top => {
                                            let new_h = (r.h - delta.y).max(60.0);
                                            r.y += r.h - new_h;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::Bottom => {
                                            r.h = (r.h + delta.y).max(60.0);
                                        }
                                        MultiviewDragMode::TopLeft => {
                                            let new_w = (r.w - delta.x).max(60.0);
                                            let new_h = (r.h - delta.y).max(60.0);
                                            r.x += r.w - new_w;
                                            r.y += r.h - new_h;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::TopRight => {
                                            let new_w = (r.w + delta.x).max(60.0);
                                            let new_h = (r.h - delta.y).max(60.0);
                                            r.y += r.h - new_h;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::BottomLeft => {
                                            let new_w = (r.w - delta.x).max(60.0);
                                            let new_h = (r.h + delta.y).max(60.0);
                                            r.x += r.w - new_w;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::BottomRight => {
                                            r.w = (r.w + delta.x).max(60.0);
                                            r.h = (r.h + delta.y).max(60.0);
                                        }
                                    }
                                }
                                mv.geometry = r;
                            }
                        }
                    } else if let Some(target) = self.integrate_drag {
                        if let Some(pos) = pointer_pos {
                            match target {
                                IntegrateDragTarget::StartHandle(idx) => {
                                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                    if idx < self.project.state.integrations.len() {
                                        let (p, y) = t.screen_to_data(pos);
                                        self.project.state.integrations[idx].start_ppm = p;
                                        self.project.state.integrations[idx].y_start = y;
                                        self.status_message = format!("Adjusting start handle: {:.3} ppm", p);
                                    }
                                }
                                IntegrateDragTarget::EndHandle(idx) => {
                                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                    if idx < self.project.state.integrations.len() {
                                        let (p, y) = t.screen_to_data(pos);
                                        self.project.state.integrations[idx].end_ppm = p;
                                        self.project.state.integrations[idx].y_end = y;
                                        self.status_message = format!("Adjusting end handle: {:.3} ppm", p);
                                    }
                                }
                                IntegrateDragTarget::Scale { start_scale, start_y } => {
                                    ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                    let dy = start_y - pos.y;
                                    let factor = ((dy / (plot_rect.height() * 0.25)) as f64).exp();
                                    self.project.state.integration_scale = (start_scale * factor).max(1e-12);
                                    self.status_message = format!("Integration Scale: {:.2e}", self.project.state.integration_scale);
                                }
                                IntegrateDragTarget::Offset { start_offset, start_y } => {
                                    ctx.set_cursor_icon(egui::CursorIcon::Move);
                                    let dy = start_y - pos.y;
                                    let d_offset = (dy / plot_rect.height()) as f64 * 0.5;
                                    self.project.state.integration_offset = start_offset + d_offset;
                                    self.status_message = format!("Integration Offset: {:.3}", self.project.state.integration_offset);
                                }
                            }
                        }
                    } else {
                        if self.drag_start.is_none() {
                            self.drag_start = ctx.input(|i| i.pointer.press_origin()).or(pointer_pos);
                        }
                        self.drag_current = pointer_pos;
                    }
                }

                // ラバーバンド描画 (ドラッグ中、スレッショルドドラッグでない場合)
                if !self.is_dragging_threshold && !self.is_dragging_pivot && self.integrate_drag.is_none() && self.multiview_drag.is_none() {
                    if let (Some(start), Some(curr)) = (self.drag_start, self.drag_current) {
                        let painter = ui.painter_at(plot_rect);
                        let axis_y = t.axis_y();

                        // Zoom ツールがアクティブな場合は最優先で Zoom ラバーバンドを表示
                        if let Some(tool) = self.active_zoom {
                            let band_rect = match tool {
                                ZoomTool::Rect => Rect::from_two_pos(start, curr),
                                ZoomTool::X => Rect::from_min_max(
                                    Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                    Pos2::new(start.x.max(curr.x), axis_y),
                                ),
                                ZoomTool::Y => Rect::from_min_max(
                                    Pos2::new(plot_rect.min.x, start.y.min(curr.y)),
                                    Pos2::new(plot_rect.max.x, start.y.max(curr.y)),
                                ),
                            };
                            painter.rect_filled(band_rect, 0.0, self.plot_style.rubberband_color);
                            painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(13, 110, 253)));
                        } else if let Some(mode) = self.mode {
                            match mode {
                                AppMode::Reference => {
                                    // paint_spectrum の ref_drag_range で既に半透明矩形を描画
                                }
                                AppMode::Peak => {
                                    let band_rect = Rect::from_min_max(
                                        Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                        Pos2::new(start.x.max(curr.x), axis_y),
                                    );
                                    painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(0, 180, 80, 25));
                                }
                                AppMode::Integrate => {
                                    let min_x = start.x.min(curr.x);
                                    let max_x = start.x.max(curr.x);
                                    let band_rect = Rect::from_min_max(
                                        Pos2::new(min_x, plot_rect.min.y),
                                        Pos2::new(max_x, axis_y),
                                    );
                                    match self.action_state.integrate_submode {
                                        IntegrateSubMode::Add => {
                                            // 1. 半透明の濃いめハイライト (赤/ピンク)
                                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(225, 29, 72, 45));
                                            painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(225, 29, 72)));
                                            // 2. 開始位置と現在位置の両端に明瞭な縦線 (上からX軸まで届く赤線)
                                            let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(225, 29, 72));
                                            painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);

                                            // 3. 上端に選択範囲の PPM 情報ガイド
                                            let (p1, _) = t.screen_to_data(start);
                                            let (p2, _) = t.screen_to_data(curr);
                                            let p_s = p1.max(p2);
                                            let p_e = p1.min(p2);
                                            let label_txt = format!("{:.3} ~ {:.3} ppm (Δ={:.3})", p_s, p_e, p_s - p_e);
                                            painter.text(
                                                Pos2::new((min_x + max_x) * 0.5, plot_rect.min.y + 12.0),
                                                egui::Align2::CENTER_CENTER,
                                                label_txt,
                                                egui::FontId::proportional(11.5),
                                                Color32::from_rgb(225, 29, 72),
                                            );

                                            // 4. ドラッグ中のリアルタイム積分曲線描画
                                            if (start.x - curr.x).abs() > 4.0 {
                                                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                    let temp_item = IntegrationItem {
                                                        id: "preview".to_string(),
                                                        start_ppm: p_s,
                                                        end_ppm: p_e,
                                                        y_start: 0.0,
                                                        y_end: 0.0,
                                                    };
                                                    let scale = if self.project.state.integrations.is_empty() || self.project.state.integration_scale == 1.0 {
                                                        if let Some(r0) = compute_integral(spec, ppm, &temp_item, 1.0, 1.0, 0.03) {
                                                            let max_spec = self.project.max_intensity();
                                                            if r0.total_area > 1e-12 && max_spec > 0.0 {
                                                                (max_spec * 0.35) / r0.total_area
                                                            } else {
                                                                1.0
                                                            }
                                                        } else {
                                                            1.0
                                                        }
                                                    } else {
                                                        self.project.state.integration_scale
                                                    };
                                                    let ref_factor = self.project.state.integration_ref_factor();
                                                    if let Some(res) = compute_integral(spec, ppm, &temp_item, scale, ref_factor, 0.03) {
                                                        if res.ppm.len() > 1 && res.ppm.len() == res.curve_y.len() {
                                                            let mut pts: Vec<Pos2> = Vec::with_capacity(res.ppm.len());
                                                            for i in 0..res.ppm.len() {
                                                                let pos = t.data_to_screen(res.ppm[i], res.curve_y[i]);
                                                                let clamped_pos = Pos2::new(pos.x, pos.y.clamp(plot_rect.min.y, plot_rect.max.y));
                                                                pts.push(clamped_pos);
                                                            }
                                                            if pts.len() > 1 {
                                                                painter.add(egui::epaint::PathShape::line(
                                                                    pts.clone(),
                                                                    Stroke::new(2.0_f32, self.plot_style.integral_color),
                                                                ));

                                                                // 5. ドラッグ中のリアルタイム積分数値ラベル (縦書き90度回転)
                                                                let mid_x = (start.x + curr.x) * 0.5;
                                                                let val_text = format!("{:.1$}", res.normalized_value, self.plot_style.integral_decimals);
                                                                let font_intg = egui::FontId::proportional(11.0);
                                                                let galley = painter.layout_no_wrap(val_text, font_intg, self.plot_style.integral_color);
                                                                let text_len = galley.size().x;
                                                                let text_h = galley.size().y;
                                                                let min_screen_y = pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                                                                let start_y = (min_screen_y - 4.0 - text_len).max(plot_rect.min.y + 4.0);
                                                                let text_pos = Pos2::new(mid_x + text_h * 0.5, start_y);
                                                                let ts = egui::epaint::TextShape::new(text_pos, galley, self.plot_style.integral_color)
                                                                    .with_angle(std::f32::consts::FRAC_PI_2);
                                                                painter.add(ts);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        IntegrateSubMode::Delete => {
                                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(220, 38, 38, 50));
                                            let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(220, 38, 38));
                                            painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);
                                        }
                                        IntegrateSubMode::Split => {
                                            let stroke_v = Stroke::new(2.0_f32, Color32::from_rgb(13, 110, 253));
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);
                                        }
                                        IntegrateSubMode::Reference => {
                                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(13, 110, 253, 45));
                                            let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(13, 110, 253));
                                            painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);
                                        }
                                        _ => {}
                                    }
                                }
                                AppMode::Multiview => {
                                    if self.action_state.multiview_submode != MultiviewSubMode::Delete {
                                        let p1 = t.screen_to_data(start).0;
                                        let p2 = t.screen_to_data(curr).0;
                                        let p_high = p1.max(p2);
                                        let p_low = p1.min(p2);
                                        let delta_p = p_high - p_low;

                                        let min_x = start.x.min(curr.x);
                                        let max_x = start.x.max(curr.x);
                                        let band_rect = Rect::from_min_max(
                                            Pos2::new(min_x, plot_rect.min.y),
                                            Pos2::new(max_x, axis_y),
                                        );
                                        painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(147, 51, 234, 45));
                                        painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(147, 51, 234)));
                                        let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(147, 51, 234));
                                        painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                        painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);

                                        let label_txt = format!("{:.3} ~ {:.3} ppm (Δ={:.3})", p_high, p_low, delta_p);
                                        painter.text(
                                            Pos2::new((min_x + max_x) * 0.5, plot_rect.min.y + 12.0),
                                            egui::Align2::CENTER_CENTER,
                                            label_txt,
                                            egui::FontId::proportional(11.5),
                                            Color32::from_rgb(147, 51, 234),
                                        );
                                    }
                                }
                                AppMode::JCoupling => {
                                    if self.action_state.jcoupling_add_active {
                                        let band_rect = Rect::from_min_max(
                                            Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                            Pos2::new(start.x.max(curr.x), axis_y),
                                        );
                                        painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(13, 110, 253, 30));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // Phase モード時の Pivot 線の描画 (赤い縦線 + ラベルタグ)
                if self.mode == Some(AppMode::Phase) {
                    let painter = ui.painter_at(plot_rect);
                    let axis_y = t.axis_y();
                    let pivot_ppm = self.project.pivot_ppm();
                    let px = t.ppm_to_screen_x(pivot_ppm);

                    if px >= plot_rect.min.x - 2.0 && px <= plot_rect.max.x + 2.0 {
                        let line_color = Color32::from_rgb(220, 38, 38);

                        painter.line_segment(
                            [Pos2::new(px, plot_rect.min.y), Pos2::new(px, axis_y)],
                            Stroke::new(1.5_f32, line_color),
                        );

                        let tag_text = format!("Pivot: {:.3} ppm", pivot_ppm);
                        let font_id = egui::FontId::proportional(11.0);
                        let galley = painter.layout_no_wrap(tag_text, font_id, Color32::WHITE);
                        let tag_w = galley.size().x + 8.0;
                        let tag_h = galley.size().y + 4.0;
                        let tag_rect = Rect::from_center_size(
                            Pos2::new(px.clamp(plot_rect.min.x + tag_w * 0.5 + 4.0, plot_rect.max.x - tag_w * 0.5 - 4.0), plot_rect.min.y + 12.0),
                            egui::vec2(tag_w, tag_h),
                        );
                        painter.rect_filled(tag_rect, 3.0, line_color);
                        painter.galley(Pos2::new(tag_rect.min.x + 4.0, tag_rect.min.y + 2.0), galley, Color32::WHITE);
                    }

                    // Pivot モード中にマウスを押下している場合のプレビュー線
                    if is_phase_pivot_mode {
                        let is_pressing = ui.input(|i| i.pointer.primary_down()) || self.is_dragging_pivot;
                        if is_pressing {
                            if let Some(pos) = pointer_pos {
                                if plot_rect.contains(pos) && pos.y <= axis_y {
                                    let shift_down = ctx.input(|i| i.modifiers.shift);
                                    let cur_ppm = t.screen_to_data(pos).0;
                                    let (preview_ppm, is_snapped) = if shift_down {
                                        (cur_ppm, false)
                                    } else if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                        let p1 = t.screen_to_data(Pos2::new(pos.x - 20.0, pos.y)).0;
                                        let p2 = t.screen_to_data(Pos2::new(pos.x + 20.0, pos.y)).0;
                                        if let Some(snapped) = crate::core::signal::phase::find_highest_peak_in_range(spec, ppm, p1, p2) {
                                            (snapped, true)
                                        } else {
                                            (cur_ppm, false)
                                        }
                                    } else {
                                        (cur_ppm, false)
                                    };

                                    let prev_px = t.ppm_to_screen_x(preview_ppm);
                                    if prev_px >= plot_rect.min.x - 2.0 && prev_px <= plot_rect.max.x + 2.0 {
                                        let prev_color = Color32::from_rgb(239, 68, 68);
                                        painter.line_segment(
                                            [Pos2::new(prev_px, plot_rect.min.y), Pos2::new(prev_px, axis_y)],
                                            Stroke::new(2.0_f32, prev_color),
                                        );

                                        let tag_text = if is_snapped {
                                            format!("New Pivot: {:.3} ppm (Snapped)", preview_ppm)
                                        } else {
                                            format!("New Pivot: {:.3} ppm", preview_ppm)
                                        };
                                        let font_id = egui::FontId::proportional(11.0);
                                        let galley = painter.layout_no_wrap(tag_text, font_id, Color32::WHITE);
                                        let tag_w = galley.size().x + 8.0;
                                        let tag_h = galley.size().y + 4.0;
                                        let tag_rect = Rect::from_center_size(
                                            Pos2::new(prev_px.clamp(plot_rect.min.x + tag_w * 0.5 + 4.0, plot_rect.max.x - tag_w * 0.5 - 4.0), plot_rect.min.y + 32.0),
                                            egui::vec2(tag_w, tag_h),
                                        );
                                        painter.rect_filled(tag_rect, 3.0, prev_color);
                                        painter.galley(Pos2::new(tag_rect.min.x + 4.0, tag_rect.min.y + 2.0), galley, Color32::WHITE);
                                    }
                                }
                            }
                        }
                    }
                }

                // ドラッグ終了 (解放) 時の処理
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    if self.is_dragging_pivot {
                        self.is_dragging_pivot = false;
                        if let Some(pos) = pointer_pos {
                            let (cur_ppm, _) = t.screen_to_data(pos);
                            let shift_down = ctx.input(|i| i.modifiers.shift);
                            let target_ppm = if shift_down {
                                cur_ppm
                            } else if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                let p1 = t.screen_to_data(Pos2::new(pos.x - 20.0, pos.y)).0;
                                let p2 = t.screen_to_data(Pos2::new(pos.x + 20.0, pos.y)).0;
                                crate::core::signal::phase::find_highest_peak_in_range(spec, ppm, p1, p2).unwrap_or(cur_ppm)
                            } else {
                                cur_ppm
                            };
                            self.project.set_pivot_ppm(target_ppm);
                            self.push_history();
                            self.status_message = format!("Set pivot to {:.3} ppm", self.project.pivot_ppm());
                        }
                    } else if self.multiview_drag.is_some() {
                        self.multiview_drag = None;
                        self.push_history();
                    } else if self.integrate_drag.is_some() {
                        self.integrate_drag = None;
                        self.push_history();
                    } else if self.is_dragging_threshold {
                        self.is_dragging_threshold = false;
                        self.status_message = format!("Threshold set to {:.2}", self.action_state.peak_threshold);
                    } else if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
                        let (p_start, y_start) = t.screen_to_data(start);
                        let (p_end, y_end) = t.screen_to_data(end);

                        if let Some(tool) = self.active_zoom {
                            // ズーム前の範囲を履歴に保存
                            self.zoom_history.push((t.ppm_min, t.ppm_max, t.y_min, t.y_max));

                            let dx = (start.x - end.x).abs();
                            let dy = (start.y - end.y).abs();
                            if dx > 5.0 || dy > 5.0 {
                                match tool {
                                    ZoomTool::Rect => {
                                        t.ppm_min = p_start.min(p_end);
                                        t.ppm_max = p_start.max(p_end);
                                        t.y_min = y_start.min(y_end);
                                        t.y_max = y_start.max(y_end);
                                    }
                                    ZoomTool::X => {
                                        t.ppm_min = p_start.min(p_end);
                                        t.ppm_max = p_start.max(p_end);
                                    }
                                    ZoomTool::Y => {
                                        t.y_min = y_start.min(y_end);
                                        t.y_max = y_start.max(y_end);
                                    }
                                }
                            }
                        } else if let Some(mode) = self.mode {
                            match mode {
                                AppMode::Reference => {
                                    if self.action_state.ref_set_active && (start.x - end.x).abs() > 3.0 {
                                        if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                            let p_low = p_start.min(p_end);
                                            let p_high = p_start.max(p_end);
                                            let mut best_p = None;
                                            let mut max_abs = -1.0_f64;
                                            for i in 0..ppm.len().min(spec.len()) {
                                                let p = ppm[i];
                                                if p >= p_low && p <= p_high {
                                                    let abs_val = spec[i].abs();
                                                    if abs_val > max_abs {
                                                        max_abs = abs_val;
                                                        best_p = Some(p);
                                                    }
                                                }
                                            }
                                            if let Some(peak_ppm) = best_p {
                                                let target = self.action_state.ref_target_ppm;
                                                self.apply_shift_reference(peak_ppm, target);
                                                self.push_history();
                                                self.status_message = format!("Referenced peak at {:.3} ppm -> {:.3} ppm", peak_ppm, target);
                                            }
                                        }
                                    }
                                }
                                AppMode::Peak => {
                                    if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                        let p_low = p_start.min(p_end);
                                        let p_high = p_start.max(p_end);
                                        let dx = (start.x - end.x).abs();
                                        if dx > 3.0 {
                                            if self.action_state.peak_submode == PeakSubMode::Add {
                                                let before_count = self.project.state.peaks.len();
                                                self.project.state.peaks = add_peak_in_range(spec, ppm, p_low, p_high, &self.project.state.peaks);
                                                if self.project.state.peaks.len() > before_count {
                                                    self.push_history();
                                                    self.status_message = format!("Added peak in range [{:.3}, {:.3}] ppm", p_low, p_high);
                                                } else {
                                                    self.status_message = "Peak already exists in selected range".to_string();
                                                }
                                            } else if self.action_state.peak_submode == PeakSubMode::Delete {
                                                let before_count = self.project.state.peaks.len();
                                                self.project.state.peaks.retain(|pk| pk.ppm < p_low || pk.ppm > p_high);
                                                if self.project.state.peaks.len() < before_count {
                                                    self.push_history();
                                                    self.status_message = "Deleted peaks in selected range".to_string();
                                                }
                                            }
                                        }
                                    }
                                }
                                AppMode::Integrate => {
                                    let p_low = p_start.min(p_end);
                                    let p_high = p_start.max(p_end);
                                    let dx = (start.x - end.x).abs();
                                    match self.action_state.integrate_submode {
                                        IntegrateSubMode::Add => {
                                            if dx > 5.0 {
                                                let s_ppm = p_start.max(p_end);
                                                let e_ppm = p_start.min(p_end);
                                                let med = self.project.spectrum_real.as_ref().map(|s| {
                                                    let mut sorted = s.to_vec();
                                                    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                                                    sorted[sorted.len() / 2]
                                                }).unwrap_or(0.0);

                                                let new_item = IntegrationItem {
                                                    id: format!("intg-{}", self.project.state.integrations.len() + 1),
                                                    start_ppm: s_ppm,
                                                    end_ppm: e_ppm,
                                                    y_start: med,
                                                    y_end: med,
                                                };

                                                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                    if self.project.state.integrations.is_empty() || self.project.state.integration_scale == 1.0 {
                                                        if let Some(res) = compute_integral(spec, ppm, &new_item, 1.0, 1.0, 0.03) {
                                                            if res.total_area > 1e-12 {
                                                                self.project.state.integration_ref_area = res.total_area;
                                                                self.project.state.integration_ref_value = 1.0;
                                                                let max_spec = self.project.max_intensity();
                                                                if max_spec > 0.0 {
                                                                    self.project.state.integration_scale = (max_spec * 0.35) / res.total_area;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                self.project.state.integrations.push(new_item);
                                                self.push_history();
                                                self.status_message = format!("Added integration {:.3} ~ {:.3} ppm", s_ppm, e_ppm);
                                            }
                                        }
                                        IntegrateSubMode::Delete => {
                                            if dx > 3.0 {
                                                let before_len = self.project.state.integrations.len();
                                                self.project.state.integrations.retain(|item| {
                                                    let item_low = item.min_ppm();
                                                    let item_high = item.max_ppm();
                                                    !(p_low.max(item_low) <= p_high.min(item_high))
                                                });
                                                if self.project.state.integrations.len() != before_len {
                                                    self.push_history();
                                                    self.status_message = "Deleted integrations in range".to_string();
                                                }
                                            }
                                        }
                                        IntegrateSubMode::Split => {
                                            let split_ppm = p_end;
                                            let mut split_idx = None;
                                            for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                                if item.contains_ppm(split_ppm) {
                                                    split_idx = Some(idx);
                                                    break;
                                                }
                                            }
                                            if let Some(idx) = split_idx {
                                                let item = self.project.state.integrations.remove(idx);
                                                let (x1, x2) = (item.start_ppm, item.end_ppm);
                                                let (y1, y2) = (item.y_start, item.y_end);
                                                let y_split = item.baseline_y_at(split_ppm);
                                                let d1 = IntegrationItem {
                                                    id: format!("intg-{}", self.project.state.integrations.len() + 1),
                                                    start_ppm: x1,
                                                    end_ppm: split_ppm,
                                                    y_start: y1,
                                                    y_end: y_split,
                                                };
                                                let d2 = IntegrationItem {
                                                    id: format!("intg-{}", self.project.state.integrations.len() + 2),
                                                    start_ppm: split_ppm,
                                                    end_ppm: x2,
                                                    y_start: y_split,
                                                    y_end: y2,
                                                };
                                                self.project.state.integrations.insert(idx, d2);
                                                self.project.state.integrations.insert(idx, d1);
                                                self.push_history();
                                                self.status_message = format!("Split integration at {:.3} ppm", split_ppm);
                                            }
                                        }
                                        IntegrateSubMode::Reference => {
                                            let ref_ppm = p_end;
                                            let mut target_idx = None;
                                            for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                                if item.contains_ppm(ref_ppm) || (p_low.max(item.min_ppm()) <= p_high.min(item.max_ppm())) {
                                                    target_idx = Some(idx);
                                                    break;
                                                }
                                            }
                                            if let Some(idx) = target_idx {
                                                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                    let item = &self.project.state.integrations[idx];
                                                    if let Some(res) = compute_integral(spec, ppm, item, 1.0, 1.0, 0.03) {
                                                        if res.total_area > 1e-12 {
                                                            let target_val = self.action_state.integration_ref_val;
                                                            self.project.state.integration_ref_area = res.total_area;
                                                            self.project.state.integration_ref_value = target_val;
                                                            self.project.push_history();
                                                            self.status_message = format!("Set reference integral to {:.2}", target_val);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                AppMode::Multiview => {
                                    let p_low = p_start.min(p_end);
                                    let p_high = p_start.max(p_end);
                                    let dx = (start.x - end.x).abs();
                                    let ratio = self.action_state.multiview_ratio;

                                    if self.action_state.multiview_submode != MultiviewSubMode::Delete {
                                        if dx > 8.0 {
                                            let view_ppm_span = (t.ppm_max - t.ppm_min).abs().max(1e-6);
                                            let px_w = plot_rect.width();
                                            let w_main = ((p_high - p_low) / view_ppm_span) * (px_w as f64);
                                            let w = (240.0 + (w_main * ratio) as f32 * 0.5).clamp(240.0, 600.0);
                                            let h = (w * 0.70).clamp(160.0, 420.0);
                                            // 既存の拡大図に被らないように左上から順に空き位置を探索
                                            let pos = find_non_overlapping_multiview_pos(&self.project.state.multiviews, plot_rect, w, h);
                                            let geom = RectF {
                                                x: pos.x,
                                                y: pos.y,
                                                w,
                                                h,
                                            };
                                            let mv_id = format!("mv-{}", self.project.state.multiviews.len() + 1);
                                            self.project.state.multiviews.push(MultiviewItem {
                                                id: mv_id.clone(),
                                                src_x_min: p_low,
                                                src_x_max: p_high,
                                                src_y_min: None,
                                                src_y_max: None,
                                                ratio,
                                                geometry: geom,
                                            });
                                            self.selected_multiview_ids.clear();
                                            self.selected_multiview_ids.insert(mv_id);
                                            if self.action_state.multiview_auto_align {
                                                self.adjust_y_multiviews_internal();
                                                self.align_multiviews_internal(plot_rect);
                                            }
                                            self.push_history();
                                            self.status_message = format!("Added multiview inset {:.3} ~ {:.3} ppm", p_high, p_low);
                                        }
                                    }
                                }
                                AppMode::JCoupling => {
                                    if self.action_state.jcoupling_add_active && (start.x - end.x).abs() > 5.0 {
                                        let p_low = p_start.min(p_end);
                                        let p_high = p_start.max(p_end);
                                        let freq_mhz = self.project.metadata.obs_freq_mhz;

                                        let mut peaks_in_range = Vec::new();
                                        let mut peaks_hz = Vec::new();
                                        let mut intensities = Vec::new();
                                        for pk in &self.project.state.peaks {
                                            if pk.ppm >= p_low && pk.ppm <= p_high {
                                                peaks_in_range.push(pk);
                                                peaks_hz.push(pk.ppm * freq_mhz);
                                                intensities.push(pk.intensity);
                                            }
                                        }

                                        if !peaks_hz.is_empty() {
                                            // 1. ピーク強度加重平均による化学シフト重心の計算
                                            let total_int: f64 = intensities.iter().sum();
                                            let center_ppm = if total_int > 0.0 {
                                                peaks_in_range.iter().map(|p| p.ppm * p.intensity).sum::<f64>() / total_int
                                            } else {
                                                (p_low + p_high) * 0.5
                                            };

                                            let shift_str = format!("{:.2}", center_ppm);
                                            let shift_str_m = format!("{:.2}-{:.2}", p_high, p_low);

                                            // 2. プロトン数 (積分値) の算出
                                            let mut raw_protons = 1.0_f64;
                                            if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                let ref_factor = self.project.state.integration_ref_factor();

                                                // 重なる既存の積分区間を探索
                                                let mut matched_intg = None;
                                                for intg in &self.project.state.integrations {
                                                    let i_min = intg.min_ppm();
                                                    let i_max = intg.max_ppm();
                                                    let overlap_min = p_low.max(i_min);
                                                    let overlap_max = p_high.min(i_max);
                                                    if overlap_max > overlap_min {
                                                        let overlap_len = overlap_max - overlap_min;
                                                        if overlap_len > 0.4 * (i_max - i_min) || overlap_len > 0.4 * (p_high - p_low) {
                                                            if let Some(res) = compute_integral(spec, ppm, intg, 1.0, ref_factor, 0.0) {
                                                                matched_intg = Some(res.normalized_value);
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }

                                                if let Some(val) = matched_intg {
                                                    raw_protons = val;
                                                } else if !self.project.state.integrations.is_empty() || self.project.state.integration_ref_area != 1.0 {
                                                    // 選択範囲の直接台形積分
                                                    let n_pts = ppm.len().min(spec.len());
                                                    let mut in_range_pts: Vec<(f64, f64)> = Vec::new();
                                                    for i in 0..n_pts {
                                                        let p = ppm[i];
                                                        if p >= p_low && p <= p_high {
                                                            in_range_pts.push((p, spec[i]));
                                                        }
                                                    }
                                                    if in_range_pts.len() >= 2 {
                                                        in_range_pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                                                        let mut area = 0.0_f64;
                                                        for i in 0..in_range_pts.len() - 1 {
                                                            let dx = (in_range_pts[i + 1].0 - in_range_pts[i].0).abs();
                                                            let avg_y = (in_range_pts[i].1 + in_range_pts[i + 1].1) * 0.5;
                                                            area += avg_y * dx;
                                                        }
                                                        raw_protons = (area.abs() * ref_factor).max(0.01);
                                                    }
                                                }
                                            }

                                            // 3. Allow Non-Integer の適用
                                            let proton_str = if self.action_state.non_integer_protons {
                                                format!("{:.2}H", raw_protons)
                                            } else {
                                                format!("{}H", raw_protons.round().max(1.0) as i64)
                                            };

                                            let candidates = analyze_multiplet(
                                                peaks_hz,
                                                intensities,
                                                &shift_str,
                                                &shift_str_m,
                                                &proton_str,
                                                1.0,
                                            );

                                            if !candidates.is_empty() {
                                                self.jcoupling_dialog_state.candidates = candidates;
                                                self.jcoupling_dialog_state.selected_idx = 0;
                                                self.jcoupling_dialog_state.edited_text = self.jcoupling_dialog_state.candidates[0].text.clone();
                                                self.jcoupling_dialog_state.center_ppm = center_ppm;
                                                self.jcoupling_dialog_state.open = true;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    self.drag_start = None;
                    self.drag_current = None;
                    self.is_dragging_threshold = false;
                    self.is_dragging_pivot = false;

                    // 一時 Zoom 中にキーが既に離れていた場合、ドラッグ完了のこの瞬間に元のモードへ復帰
                    if self.temp_zoom_saved.is_some() {
                        let z_down = ctx.input(|i| i.key_down(Key::Z));
                        let x_down = ctx.input(|i| i.key_down(Key::X));
                        let s_down = ctx.input(|i| i.key_down(Key::S));
                        if !z_down && !x_down && !s_down {
                            if let Some(saved) = self.temp_zoom_saved.take() {
                                self.active_zoom = saved;
                            }
                        }
                    }
                }
            }
        }

        if do_reset_zoom {
            self.reset_zoom();
        }
    }
}
