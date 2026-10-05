use std::fs;
use std::sync::{Arc, Mutex};
use egui::{
    vec2, Align2, Button, Checkbox, Color32, FontFamily, FontId, Frame, Pos2, Rect,
    RichText, Stroke, Ui, Window,
};
use egui::epaint::{PathShape, TextShape};
use std::f32::consts::FRAC_PI_2;

use crate::core::compute_integral;
use crate::gui::dialogs::print_dialog::{
    fetch_system_printers, open_printer_preferences, PrintOrientation, SystemPrinter,
};
use crate::gui::dialogs::print_style_dialog::{
    PrintStyleDialogState, PrintStyleSettings,
};
use crate::multispec::export::export_multispec_svg;
use crate::multispec::settings::MultiSpecPrintSettings;
use crate::multispec::state::MultiSpecState;

/// MultiSpec 印刷ダイアログの状態管理
pub struct MultiSpecPrintDialogState {
    pub is_open: bool,
    pub available_printers: Arc<Mutex<Option<Vec<SystemPrinter>>>>,
    pub is_loading_printers: bool,
    pub status_message: Option<(String, bool)>, // (message, is_error)
    pub open_counter: usize,
    pub style_dialog_state: PrintStyleDialogState,
}

impl Default for MultiSpecPrintDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            available_printers: Arc::new(Mutex::new(None)),
            is_loading_printers: false,
            status_message: None,
            open_counter: 0,
            style_dialog_state: PrintStyleDialogState::default(),
        }
    }
}

impl std::fmt::Debug for MultiSpecPrintDialogState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiSpecPrintDialogState")
            .field("is_open", &self.is_open)
            .field("is_loading_printers", &self.is_loading_printers)
            .field("status_message", &self.status_message)
            .field("open_counter", &self.open_counter)
            .finish()
    }
}

impl MultiSpecPrintDialogState {
    /// ダイアログを開き、非同期でシステムプリンター一覧を検出する
    pub fn open(&mut self) {
        self.is_open = true;
        self.open_counter += 1;
        self.status_message = None;

        let printers_arc = Arc::clone(&self.available_printers);
        self.is_loading_printers = true;

        std::thread::spawn(move || {
            let printers = fetch_system_printers();
            if let Ok(mut lock) = printers_arc.lock() {
                *lock = Some(printers);
            }
        });
    }

    /// プリンター一覧の再取得
    pub fn refresh_printers(&mut self) {
        let printers_arc = Arc::clone(&self.available_printers);
        self.is_loading_printers = true;
        self.status_message = None;

        std::thread::spawn(move || {
            let printers = fetch_system_printers();
            if let Ok(mut lock) = printers_arc.lock() {
                *lock = Some(printers);
            }
        });
    }
}

pub fn show_multispec_print_dialog(
    ctx: &egui::Context,
    dialog_state: &mut MultiSpecPrintDialogState,
    state: &MultiSpecState,
    print_settings: &mut MultiSpecPrintSettings,
    style_settings: &mut PrintStyleSettings,
) {
    if !dialog_state.is_open {
        return;
    }

    // 非同期ロードされたプリンター一覧の反映
    if let Ok(lock) = dialog_state.available_printers.lock() {
        if let Some(ref printers) = *lock {
            dialog_state.is_loading_printers = false;
            let current_exists = !print_settings.printer_name.is_empty()
                && printers.iter().any(|p| p.name == print_settings.printer_name);
            if !current_exists {
                if let Some(def) = printers.iter().find(|p| p.is_default) {
                    print_settings.printer_name = def.name.clone();
                } else if let Some(first) = printers.first() {
                    print_settings.printer_name = first.name.clone();
                }
            }
        }
    }

    let mut is_open = dialog_state.is_open;
    let mut should_close = false;
    let is_style_open = dialog_state.style_dialog_state.is_open;

    Window::new(RichText::new("MultiSpec Print Preview").strong().size(14.0))
        .id(egui::Id::new("multispec_print_preview_dialog").with(dialog_state.open_counter))
        .open(&mut is_open)
        .resizable(true)
        .default_width(820.0)
        .default_height(640.0)
        .min_width(650.0)
        .min_height(500.0)
        .show(ctx, |ui| {
            if is_style_open {
                ui.disable();
            }
            ui.spacing_mut().item_spacing.y = 8.0;

            // 1. 上部コントロール (プリンター選択 & ページ設定)
            ui.horizontal(|ui| {
                ui.label(RichText::new("Printer").strong().size(12.0));

                let current_selection = if print_settings.printer_name.is_empty() {
                    if dialog_state.is_loading_printers {
                        "Detecting printers..."
                    } else {
                        "Select Printer"
                    }
                } else {
                    &print_settings.printer_name
                };

                egui::ComboBox::from_id_salt("multispec_printer_select")
                    .selected_text(current_selection)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        if let Ok(lock) = dialog_state.available_printers.lock() {
                            if let Some(ref printers) = *lock {
                                for p in printers {
                                    let label = if p.is_default {
                                        format!("{} (Default)", p.name)
                                    } else {
                                        p.name.clone()
                                    };
                                    ui.selectable_value(
                                        &mut print_settings.printer_name,
                                        p.name.clone(),
                                        label,
                                    );
                                }
                            }
                        }
                    });

                let p_name = print_settings.printer_name.clone();
                if ui.button("Detail...").clicked() && !p_name.is_empty() {
                    std::thread::spawn(move || {
                        open_printer_preferences(&p_name);
                    });
                }

                if ui.button("Refresh").clicked() {
                    dialog_state.refresh_printers();
                }

                ui.separator();

                ui.label(RichText::new("Orientation").strong().size(12.0));
                ui.selectable_value(
                    &mut print_settings.orientation,
                    PrintOrientation::Landscape,
                    "Landscape",
                );
                ui.selectable_value(
                    &mut print_settings.orientation,
                    PrintOrientation::Portrait,
                    "Portrait",
                );
            });

            // 2. 印刷項目 & アクションボタン
            ui.horizontal(|ui| {
                // 左側: 印刷項目
                Frame::group(ui.style()).show(ui, |ui| {
                    egui::Grid::new("multispec_print_items_grid")
                        .spacing(vec2(16.0, 4.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Print Items").strong().size(12.0));
                            ui.add(Checkbox::new(&mut print_settings.show_spectrum_names, "Spectrum Names"));
                            ui.add(Checkbox::new(&mut print_settings.show_integrals, "Integrals"));
                            ui.add(Checkbox::new(&mut print_settings.show_axis, "X-Axis"));
                            ui.add(Checkbox::new(&mut print_settings.show_title, "Title"));
                            ui.end_row();
                        });
                });

                ui.add_space(8.0);

                // 右側: アクションボタン
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        // 印刷ボタン (プライマリ: 青背景・白文字)
                        let btn_print = Button::new(RichText::new("Print").strong().size(13.0).color(Color32::WHITE))
                            .min_size(vec2(80.0, 26.0))
                            .fill(Color32::from_rgb(13, 110, 253))
                            .rounding(3.0_f32);
                        if ui.add(btn_print).clicked() {
                            match execute_multispec_native_print(state, print_settings, style_settings) {
                                Ok(()) => {
                                    should_close = true;
                                }
                                Err(e) => {
                                    dialog_state.status_message = Some((format!("Print failed: {}", e), true));
                                }
                            }
                        }

                        // 完全ベクター SVG ファイル保存ボタン
                        let btn_svg = Button::new(RichText::new("Export SVG...").size(12.5))
                            .min_size(vec2(95.0, 26.0))
                            .rounding(3.0_f32);
                        if ui.add(btn_svg).clicked() {
                            let default_svg_name = if let Some(first) = state.items.first() {
                                format!("{}_multispec.svg", first.name.replace(' ', "_"))
                            } else {
                                "multispec_comparison.svg".to_string()
                            };

                            if let Some(target) = rfd::FileDialog::new()
                                .set_title("Export MultiSpec as Vector SVG")
                                .add_filter("Scalable Vector Graphics", &["svg"])
                                .set_file_name(&default_svg_name)
                                .save_file()
                            {
                                let svg = export_multispec_svg(
                                    state,
                                    print_settings,
                                    style_settings,
                                    (state.common_ppm_min, state.common_ppm_max),
                                );
                                match fs::write(&target, svg) {
                                    Ok(()) => {
                                        dialog_state.status_message = Some((format!("Exported to {}", target.display()), false));
                                    }
                                    Err(e) => {
                                        dialog_state.status_message = Some((format!("Export failed: {}", e), true));
                                    }
                                }
                            }
                        }

                        if ui.button("Print Settings...").clicked() {
                            dialog_state.style_dialog_state.is_open = true;
                        }

                        if ui.button("Close").clicked() {
                            should_close = true;
                        }
                    });

                    // ステータスメッセージ
                    if let Some((msg, is_err)) = dialog_state.status_message.as_ref() {
                        let col = if *is_err {
                            Color32::from_rgb(220, 53, 69)
                        } else {
                            Color32::from_rgb(25, 135, 84)
                        };
                        ui.label(RichText::new(msg).size(11.5).color(col));
                    }
                });
            });

            ui.separator();

            // 3. リアルタイム用紙比率プレビュー (A4用紙, 影, 白背景, 波形, 90度回転積分, 軸, 名タグ)
            let avail = ui.available_size();
            render_multispec_realtime_preview(ui, avail, state, print_settings, style_settings);
        });

    // MultiSpec 専用スタイル設定ダイアログ表示
    show_multispec_print_settings_dialog(ctx, &mut dialog_state.style_dialog_state, style_settings);

    if should_close {
        is_open = false;
    }
    dialog_state.is_open = is_open;
}

/// リアルタイム用紙プレビューの描画
fn render_multispec_realtime_preview(
    ui: &mut Ui,
    avail_size: egui::Vec2,
    state: &MultiSpecState,
    settings: &MultiSpecPrintSettings,
    style: &PrintStyleSettings,
) {
    let page_aspect = match settings.orientation {
        PrintOrientation::Landscape => 297.0 / 210.0,
        PrintOrientation::Portrait => 210.0 / 297.0,
    };

    let mut preview_w = avail_size.x.max(200.0);
    let mut preview_h = preview_w / page_aspect;
    if preview_h > avail_size.y {
        preview_h = avail_size.y.max(150.0);
        preview_w = preview_h * page_aspect;
    }

    let (response, painter) = ui.allocate_painter(vec2(preview_w, preview_h), egui::Sense::hover());
    let page_rect = response.rect;

    // 用紙描画 (白背景 + 影 + 外枠)
    let shadow_rect = page_rect.translate(vec2(2.0, 2.0));
    painter.rect_filled(shadow_rect, 2.0, Color32::from_rgba_unmultiplied(0, 0, 0, 18));
    painter.rect_filled(page_rect, 2.0, Color32::WHITE);
    painter.rect_stroke(page_rect, 2.0, Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)));

    let mut cur_y = page_rect.min.y + 8.0;

    // 1. タイトル
    let display_title = if !state.title.is_empty() {
        &state.title
    } else {
        &settings.title
    };
    if settings.show_title && !display_title.is_empty() {
        painter.text(
            Pos2::new(page_rect.min.x + 12.0, cur_y),
            Align2::LEFT_TOP,
            display_title,
            FontId::new(12.0, FontFamily::Proportional),
            Color32::from_rgb(33, 37, 41),
        );
        cur_y += 18.0;
        painter.line_segment(
            [
                Pos2::new(page_rect.min.x + 12.0, cur_y),
                Pos2::new(page_rect.max.x - 12.0, cur_y),
            ],
            Stroke::new(0.8_f32, Color32::from_rgb(222, 226, 230)),
        );
        cur_y += 6.0;
    }

    let margin_side = 10.0_f32;
    let axis_h = if settings.show_axis { 24.0 } else { 8.0 };

    let plot_rect = Rect::from_min_max(
        Pos2::new(page_rect.min.x + margin_side, cur_y),
        Pos2::new(page_rect.max.x - margin_side, page_rect.max.y - margin_side),
    );

    let axis_y = plot_rect.max.y - axis_h;
    let plot_h = (axis_y - plot_rect.min.y).max(20.0);

    let p_min = state.common_ppm_min;
    let p_max = state.common_ppm_max;
    let p_span = (p_max - p_min).abs().max(1e-4);

    let ppm_to_screen_x = |ppm: f64| -> f32 {
        let ratio = ((p_max - ppm) / p_span) as f32;
        plot_rect.min.x + ratio * plot_rect.width()
    };

    let visible_items: Vec<usize> = state
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.visible && it.project.is_some())
        .map(|(idx, _)| idx)
        .collect();

    let num_vis = visible_items.len();
    let slot_h = if num_vis > 0 { plot_h / num_vis as f32 } else { plot_h };

    // 2. 各スペクトルの描画 (Auto Stack または Overlay)
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
            let b_y = axis_y - 15.0 - it.y_offset as f32 * (plot_h / 600.0);
            (b_y, u_h, plot_rect.min.y)
        } else {
            let s_top = plot_rect.min.y + step_idx as f32 * slot_h;
            let u_h = (slot_h * 0.72).max(8.0);
            let b_y = s_top + slot_h - 6.0 - it.y_offset as f32 * (slot_h / 200.0);
            (b_y, u_h, s_top)
        };

        let max_val = proj.max_intensity().max(1e-6);
        let top_val = max_val * (100.0 / it.y_scale_max.max(1.0));
        let min_val = -max_val * (it.y_scale_min / 100.0);
        let val_span = (top_val - min_val).max(1e-6);

        let stroke_color = Color32::from_rgb(it.color[0], it.color[1], it.color[2]);
        let stroke_width = (style.main_spectrum.line_width * 0.8_f32).clamp(0.6_f32, 2.0_f32);

        // 2.1 スペクトル名タグ (波形左上に配置)
        if settings.show_spectrum_names && !it.name.is_empty() {
            let tag_x = plot_rect.min.x + 4.0;
            let tag_y = if state.is_overlay {
                plot_rect.min.y + 4.0 + (step_idx as f32 * 14.0)
            } else {
                slot_top + 3.0
            };

            let name_font = FontId::new(8.0, FontFamily::Proportional);
            painter.text(
                Pos2::new(tag_x, tag_y),
                Align2::LEFT_TOP,
                &it.name,
                name_font,
                stroke_color,
            );
        }

        // 2.2 波形描画
        let n_pts = ppm.len().min(spec.len());
        if n_pts > 1 {
            let mut line_points = Vec::with_capacity(preview_w as usize);
            let mut prev_x = -9999.0_f32;
            let mut min_y = f32::INFINITY;
            let mut max_y = f32::NEG_INFINITY;

            for k in 0..n_pts {
                let p = ppm[k];
                if (p < p_min && p < p_max) || (p > p_min && p > p_max) {
                    continue;
                }
                let v = spec[k];
                let sx = ppm_to_screen_x(p);
                let y_norm = ((v - min_val) / val_span) as f32;
                let sy = base_y - y_norm * unit_h;

                let px = sx.round();
                if (px - prev_x).abs() >= 1.0 {
                    if prev_x >= plot_rect.min.x - 2.0 {
                        line_points.push(Pos2::new(prev_x, min_y));
                        if (max_y - min_y).abs() > 0.5 {
                            line_points.push(Pos2::new(prev_x, max_y));
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
            if prev_x >= plot_rect.min.x - 2.0 && min_y <= max_y {
                line_points.push(Pos2::new(prev_x, min_y));
                if (max_y - min_y).abs() > 0.5 {
                    line_points.push(Pos2::new(prev_x, max_y));
                }
            }

            if line_points.len() > 1 {
                painter.add(PathShape::line(
                    line_points,
                    Stroke::new(stroke_width, stroke_color),
                ));
            }
        }

        // 2.3 積分曲線 & 90度回転縦書き数値ラベル
        if settings.show_integrals && it.show_integral && !proj.state.integrations.is_empty() {
            let ref_factor = if proj.state.integration_ref_area > 1e-12 {
                proj.state.integration_ref_value / proj.state.integration_ref_area
            } else {
                1.0
            };

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
                        let mut max_curve_y = f64::NEG_INFINITY;
                        for ki in 0..res.ppm.len() {
                            let p = res.ppm[ki];
                            let sx = ppm_to_screen_x(p);
                            let y_norm = ((res.curve_y[ki] - min_val) / val_span) as f32;
                            let sy = base_y - y_norm * unit_h;
                            intg_pts.push(Pos2::new(sx, sy));
                            if res.curve_y[ki] > max_curve_y {
                                max_curve_y = res.curve_y[ki];
                            }
                        }

                        let intg_color = Color32::from_rgb(
                            (it.color[0] as f32 * 0.85) as u8,
                            (it.color[1] as f32 * 0.85) as u8,
                            (it.color[2] as f32 * 0.85) as u8,
                        );

                        painter.add(PathShape::line(
                            intg_pts,
                            Stroke::new(stroke_width * 0.85, intg_color),
                        ));

                        // 積分値ラベル (時計回り90度回転で配置)
                        let mid_p = (intg.start_ppm + intg.end_ppm) * 0.5;
                        let sx_mid = ppm_to_screen_x(mid_p);
                        let y_norm_top = ((max_curve_y - min_val) / val_span) as f32;
                        let sy_top = base_y - y_norm_top * unit_h;
                        let text_y = sy_top - 4.0;
                        let val_str = format!("{:.2}", res.normalized_value);

                        let font_id = FontId::new(7.5, FontFamily::Proportional);
                        let text_shape = TextShape::new(
                            Pos2::new(sx_mid, text_y),
                            painter.layout_no_wrap(val_str, font_id, stroke_color),
                            stroke_color,
                        ).with_angle(FRAC_PI_2);
                        painter.add(text_shape);
                    }
                }
            }
        }
    }

    // 3. 最下部共通 X 軸
    if settings.show_axis {
        painter.line_segment(
            [
                Pos2::new(plot_rect.min.x, axis_y),
                Pos2::new(plot_rect.max.x, axis_y),
            ],
            Stroke::new(1.0_f32, Color32::from_rgb(33, 37, 41)),
        );

        let major_step = if p_span > 50.0 {
            10.0
        } else if p_span > 20.0 {
            5.0
        } else if p_span > 8.0 {
            1.0
        } else if p_span > 3.0 {
            0.5
        } else if p_span > 1.0 {
            0.2
        } else {
            0.1
        };

        let first_major = (p_min.min(p_max) / major_step).floor() * major_step;
        let last_major = (p_min.max(p_max) / major_step).ceil() * major_step;

        let font_id = FontId::new(8.0, FontFamily::Proportional);
        let mut cur_major = first_major;
        while cur_major <= last_major + major_step * 0.1 {
            if (p_min..=p_max).contains(&cur_major) || (p_max..=p_min).contains(&cur_major) {
                let x = ppm_to_screen_x(cur_major);
                if x >= plot_rect.min.x - 1.0 && x <= plot_rect.max.x + 1.0 {
                    painter.line_segment(
                        [Pos2::new(x, axis_y), Pos2::new(x, axis_y + 4.0)],
                        Stroke::new(0.8_f32, Color32::from_rgb(33, 37, 41)),
                    );

                    let label = if major_step < 0.1 {
                        format!("{:.2}", cur_major)
                    } else if major_step < 1.0 {
                        format!("{:.1}", cur_major)
                    } else {
                        format!("{:.0}", cur_major)
                    };

                    painter.text(
                        Pos2::new(x, axis_y + 11.0),
                        Align2::CENTER_CENTER,
                        &label,
                        font_id.clone(),
                        Color32::from_rgb(33, 37, 41),
                    );
                }

                // 小目盛り
                let sub_step = major_step / 10.0;
                for s in 1..10 {
                    let sub_ppm = cur_major + sub_step * (s as f64);
                    if (p_min..=p_max).contains(&sub_ppm) || (p_max..=p_min).contains(&sub_ppm) {
                        let sx = ppm_to_screen_x(sub_ppm);
                        if sx >= plot_rect.min.x && sx <= plot_rect.max.x {
                            let tick_len = if s == 5 { 3.0 } else { 2.0 };
                            painter.line_segment(
                                [Pos2::new(sx, axis_y), Pos2::new(sx, axis_y + tick_len)],
                                Stroke::new(0.5_f32, Color32::from_rgb(108, 117, 125)),
                            );
                        }
                    }
                }
            }
            cur_major += major_step;
        }

        // "(ppm)" 単位
        painter.text(
            Pos2::new(plot_rect.max.x, axis_y + 18.0),
            Align2::RIGHT_CENTER,
            "(ppm)",
            font_id,
            Color32::from_rgb(73, 80, 87),
        );
    }
}

/// OSネイティブ直接印刷の実行
pub fn execute_multispec_native_print(
    state: &MultiSpecState,
    settings: &MultiSpecPrintSettings,
    style: &PrintStyleSettings,
) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        print_multispec_windows_native(state, settings, style)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (state, settings, style);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Direct native printing is currently supported on Windows. Please use 'Export SVG' for printing on other platforms.",
        ))
    }
}

#[cfg(target_os = "windows")]
fn print_multispec_windows_native(
    state: &MultiSpecState,
    settings: &MultiSpecPrintSettings,
    style: &PrintStyleSettings,
) -> std::io::Result<()> {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct DOCINFOW {
        cbSize: i32,
        lpszDocName: *const u16,
        lpszOutput: *const u16,
        lpszDatatype: *const u16,
        fwType: u32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct LOGFONTW {
        lfHeight: i32,
        lfWidth: i32,
        lfEscapement: i32,
        lfOrientation: i32,
        lfWeight: i32,
        lfItalic: u8,
        lfUnderline: u8,
        lfStrikeOut: u8,
        lfCharSet: u8,
        lfOutPrecision: u8,
        lfClipPrecision: u8,
        lfQuality: u8,
        lfPitchAndFamily: u8,
        lfFaceName: [u16; 32],
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateDCW(
            pDriver: *const u16,
            pDevice: *const u16,
            pPort: *const u16,
            pDevMode: *const std::ffi::c_void,
        ) -> isize;
        fn DeleteDC(hdc: isize) -> i32;
        fn StartDocW(hdc: isize, lpdi: *const DOCINFOW) -> i32;
        fn EndDoc(hdc: isize) -> i32;
        fn StartPage(hdc: isize) -> i32;
        fn EndPage(hdc: isize) -> i32;
        fn GetDeviceCaps(hdc: isize, nIndex: i32) -> i32;
        fn CreatePen(iStyle: i32, cWidth: i32, color: u32) -> isize;
        fn SelectObject(hdc: isize, hgdiobj: isize) -> isize;
        fn DeleteObject(ho: isize) -> i32;
        fn Polyline(hdc: isize, apt: *const POINT, cpt: i32) -> i32;
        fn SetTextColor(hdc: isize, color: u32) -> u32;
        fn SetBkMode(hdc: isize, mode: i32) -> i32;
        fn SetTextAlign(hdc: isize, fMode: u32) -> u32;
        fn CreateFontIndirectW(lplf: *const LOGFONTW) -> isize;
        fn TextOutW(hdc: isize, x: i32, y: i32, lpString: *const u16, c: i32) -> i32;
    }

    const HORZRES: i32 = 8;
    const VERTRES: i32 = 10;
    const LOGPIXELSX: i32 = 88;
    const LOGPIXELSY: i32 = 90;
    const PS_SOLID: i32 = 0;
    const TRANSPARENT: i32 = 1;
    const TA_TOP: u32 = 0;
    const TA_LEFT: u32 = 0;
    const TA_CENTER: u32 = 6;
    const TA_RIGHT: u32 = 2;
    const FW_NORMAL: i32 = 400;
    const FW_BOLD: i32 = 700;

    fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let driver_name = to_wide("WINSPOOL");
    let printer_name_wide = if !settings.printer_name.is_empty() {
        to_wide(&settings.printer_name)
    } else {
        #[link(name = "winspool")]
        unsafe extern "system" {
            fn GetDefaultPrinterW(pszBuffer: *mut u16, pcchBuffer: *mut u32) -> i32;
        }
        let mut buf = vec![0u16; 512];
        let mut len = 512u32;
        unsafe {
            if GetDefaultPrinterW(buf.as_mut_ptr(), &mut len) != 0 {
                buf.truncate(len as usize);
                buf
            } else {
                to_wide("Microsoft Print to PDF")
            }
        }
    };

    let hdc = unsafe {
        CreateDCW(
            driver_name.as_ptr(),
            printer_name_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };

    if hdc == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Failed to create printer DC for {}", settings.printer_name),
        ));
    }

    let (dev_w, dev_h, dpi_x, dpi_y) = unsafe {
        (
            GetDeviceCaps(hdc, HORZRES),
            GetDeviceCaps(hdc, VERTRES),
            GetDeviceCaps(hdc, LOGPIXELSX).max(72),
            GetDeviceCaps(hdc, LOGPIXELSY).max(72),
        )
    };

    let doc_name = to_wide("Resona MultiSpec Report");
    let doc_info = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: doc_name.as_ptr(),
        lpszOutput: std::ptr::null(),
        lpszDatatype: std::ptr::null(),
        fwType: 0,
    };

    unsafe {
        if StartDocW(hdc, &doc_info) <= 0 {
            DeleteDC(hdc);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "StartDocW failed",
            ));
        }
        StartPage(hdc);
        SetBkMode(hdc, TRANSPARENT);
    }

    let margin_px_x = (dpi_x as f64 * (settings.margin_mm / 25.4)) as i32;
    let margin_px_y = (dpi_y as f64 * (settings.margin_mm / 25.4)) as i32;

    let mut cur_y = margin_px_y;

    // フォント作成ヘルパー
    let make_font = |size_pt: f32, bold: bool, escapement: i32| -> isize {
        let h = -((size_pt * dpi_y as f32 / 72.0).round() as i32);
        let mut lf = LOGFONTW {
            lfHeight: h,
            lfWidth: 0,
            lfEscapement: escapement,
            lfOrientation: escapement,
            lfWeight: if bold { FW_BOLD } else { FW_NORMAL },
            lfItalic: 0,
            lfUnderline: 0,
            lfStrikeOut: 0,
            lfCharSet: 1, // DEFAULT_CHARSET
            lfOutPrecision: 0,
            lfClipPrecision: 0,
            lfQuality: 5, // CLEARTYPE_QUALITY
            lfPitchAndFamily: 0,
            lfFaceName: [0; 32],
        };
        let face = to_wide("Arial");
        for (i, &c) in face.iter().take(31).enumerate() {
            lf.lfFaceName[i] = c;
        }
        unsafe { CreateFontIndirectW(&lf) }
    };

    // 1. タイトル
    let display_title = if !state.title.is_empty() {
        &state.title
    } else {
        &settings.title
    };
    if settings.show_title && !display_title.is_empty() {
        let font = make_font(14.0, true, 0);
        let old_font = unsafe { SelectObject(hdc, font) };
        unsafe {
            SetTextColor(hdc, rgb(33, 37, 41));
            SetTextAlign(hdc, TA_LEFT | TA_TOP);
            let wide = to_wide(display_title);
            TextOutW(hdc, margin_px_x, cur_y, wide.as_ptr(), (wide.len() - 1) as i32);
            SelectObject(hdc, old_font);
            DeleteObject(font);
        }
        cur_y += (dpi_y as f32 * 0.28) as i32;
    }

    let footer_h = 0;
    let axis_h = if settings.show_axis { (dpi_y as f32 * 0.35) as i32 } else { (dpi_y as f32 * 0.10) as i32 };

    let plot_x = margin_px_x;
    let plot_w = dev_w - margin_px_x * 2;
    let axis_y = dev_h - margin_px_y - footer_h - axis_h;
    let plot_h = (axis_y - cur_y).max(100);

    let p_min = state.common_ppm_min;
    let p_max = state.common_ppm_max;
    let p_span = (p_max - p_min).abs().max(1e-4);

    let ppm_to_x = |ppm: f64| -> i32 {
        let ratio = (p_max - ppm) / p_span;
        plot_x + (ratio * plot_w as f64).round() as i32
    };

    let visible_items: Vec<usize> = state
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.visible && it.project.is_some())
        .map(|(idx, _)| idx)
        .collect();

    let num_vis = visible_items.len();
    let slot_h = if num_vis > 0 { plot_h / num_vis as i32 } else { plot_h };

    // 2. 各スペクトルの描画 (Auto Stack または Overlay)
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
            let u_h = (plot_h as f64 * 0.72).max(20.0);
            let b_y = axis_y - (dpi_y as f64 * 0.25) as i32 - (it.y_offset * dpi_y as f64 / 96.0) as i32;
            (b_y, u_h, cur_y)
        } else {
            let s_top = cur_y + step_idx as i32 * slot_h;
            let u_h = (slot_h as f64 * 0.72).max(20.0);
            let b_y = s_top + slot_h - (dpi_y as f64 * 0.10) as i32 - (it.y_offset * dpi_y as f64 / 96.0) as i32;
            (b_y, u_h, s_top)
        };

        let max_val = proj.max_intensity().max(1e-6);
        let top_val = max_val * (100.0 / it.y_scale_max.max(1.0));
        let min_val = -max_val * (it.y_scale_min / 100.0);
        let val_span = (top_val - min_val).max(1e-6);

        let col = rgb(it.color[0], it.color[1], it.color[2]);
        let pen_w = ((style.main_spectrum.line_width as f64 * dpi_x as f64 / 96.0).round() as i32).max(1);

        // 2.1 スペクトル名タグ (波形左上に配置)
        if settings.show_spectrum_names && !it.name.is_empty() {
            let tag_x = plot_x + (dpi_x as f32 * 0.08) as i32;
            let tag_y = if state.is_overlay {
                cur_y + (step_idx as i32 * (dpi_y as f32 * 0.22) as i32)
            } else {
                slot_top + (dpi_y as f32 * 0.05) as i32
            };

            let font = make_font(10.0, true, 0);
            let old_font = unsafe { SelectObject(hdc, font) };
            unsafe {
                SetTextColor(hdc, col);
                SetTextAlign(hdc, TA_LEFT | TA_TOP);
                let wide = to_wide(&it.name);
                TextOutW(hdc, tag_x, tag_y, wide.as_ptr(), (wide.len() - 1) as i32);
                SelectObject(hdc, old_font);
                DeleteObject(font);
            }
        }

        // 2.2 波形描画
        let n_pts = ppm.len().min(spec.len());
        if n_pts > 1 {
            let mut gdi_pts: Vec<POINT> = Vec::with_capacity(plot_w as usize);
            let mut prev_x = -99999;
            let mut min_y = i32::MAX;
            let mut max_y = i32::MIN;

            for k in 0..n_pts {
                let p = ppm[k];
                if (p < p_min && p < p_max) || (p > p_min && p > p_max) {
                    continue;
                }
                let v = spec[k];
                let sx = ppm_to_x(p);
                let y_norm = (v - min_val) / val_span;
                let sy = base_y - (y_norm * unit_h).round() as i32;

                if sx != prev_x {
                    if prev_x >= plot_x - 5 {
                        gdi_pts.push(POINT { x: prev_x, y: min_y });
                        if (max_y - min_y).abs() > 1 {
                            gdi_pts.push(POINT { x: prev_x, y: max_y });
                        }
                    }
                    prev_x = sx;
                    min_y = sy;
                    max_y = sy;
                } else {
                    if sy < min_y { min_y = sy; }
                    if sy > max_y { max_y = sy; }
                }
            }
            if prev_x >= plot_x - 5 && min_y <= max_y {
                gdi_pts.push(POINT { x: prev_x, y: min_y });
                if (max_y - min_y).abs() > 1 {
                    gdi_pts.push(POINT { x: prev_x, y: max_y });
                }
            }

            if gdi_pts.len() > 1 {
                unsafe {
                    let pen = CreatePen(PS_SOLID, pen_w, col);
                    let old_pen = SelectObject(hdc, pen);
                    Polyline(hdc, gdi_pts.as_ptr(), gdi_pts.len() as i32);
                    SelectObject(hdc, old_pen);
                    DeleteObject(pen);
                }
            }
        }

        // 2.3 積分曲線 & 90度回転縦書き数値ラベル
        if settings.show_integrals && it.show_integral && !proj.state.integrations.is_empty() {
            let ref_factor = if proj.state.integration_ref_area > 1e-12 {
                proj.state.integration_ref_value / proj.state.integration_ref_area
            } else {
                1.0
            };

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
                        let mut intg_pts: Vec<POINT> = Vec::with_capacity(res.ppm.len());
                        let mut max_curve_y = f64::NEG_INFINITY;
                        for ki in 0..res.ppm.len() {
                            let p = res.ppm[ki];
                            let sx = ppm_to_x(p);
                            let y_norm = (res.curve_y[ki] - min_val) / val_span;
                            let sy = base_y - (y_norm * unit_h).round() as i32;
                            intg_pts.push(POINT { x: sx, y: sy });
                            if res.curve_y[ki] > max_curve_y {
                                max_curve_y = res.curve_y[ki];
                            }
                        }

                        let intg_col = rgb(
                            (it.color[0] as f64 * 0.85) as u8,
                            (it.color[1] as f64 * 0.85) as u8,
                            (it.color[2] as f64 * 0.85) as u8,
                        );

                        unsafe {
                            let pen = CreatePen(PS_SOLID, (pen_w - 1).max(1), intg_col);
                            let old_pen = SelectObject(hdc, pen);
                            Polyline(hdc, intg_pts.as_ptr(), intg_pts.len() as i32);
                            SelectObject(hdc, old_pen);
                            DeleteObject(pen);
                        }

                        // 積分値ラベル (時計回り90度回転 = 2700 / 270度)
                        let mid_p = (intg.start_ppm + intg.end_ppm) * 0.5;
                        let sx_mid = ppm_to_x(mid_p);
                        let y_norm_top = (max_curve_y - min_val) / val_span;
                        let sy_top = base_y - (y_norm_top * unit_h).round() as i32;
                        let text_y = sy_top - (dpi_y as f32 * 0.05) as i32;
                        let val_str = format!("{:.2}", res.normalized_value);

                        let font = make_font(8.5, false, 2700);
                        let old_font = unsafe { SelectObject(hdc, font) };
                        unsafe {
                            SetTextColor(hdc, col);
                            SetTextAlign(hdc, TA_LEFT | TA_TOP);
                            let wide = to_wide(&val_str);
                            TextOutW(hdc, sx_mid, text_y, wide.as_ptr(), (wide.len() - 1) as i32);
                            SelectObject(hdc, old_font);
                            DeleteObject(font);
                        }
                    }
                }
            }
        }
    }

    // 3. 最下部共通 X 軸
    if settings.show_axis {
        let axis_pen = unsafe { CreatePen(PS_SOLID, ((dpi_x as f32 / 96.0).round() as i32).max(1), rgb(33, 37, 41)) };
        let old_pen = unsafe { SelectObject(hdc, axis_pen) };
        let axis_pts = [
            POINT { x: plot_x, y: axis_y },
            POINT { x: plot_x + plot_w, y: axis_y },
        ];
        unsafe {
            Polyline(hdc, axis_pts.as_ptr(), 2);
        }

        let major_step = if p_span > 50.0 {
            10.0
        } else if p_span > 20.0 {
            5.0
        } else if p_span > 8.0 {
            1.0
        } else if p_span > 3.0 {
            0.5
        } else if p_span > 1.0 {
            0.2
        } else {
            0.1
        };

        let first_major = (p_min.min(p_max) / major_step).floor() * major_step;
        let last_major = (p_min.max(p_max) / major_step).ceil() * major_step;

        let font = make_font(9.0, false, 0);
        let old_font = unsafe { SelectObject(hdc, font) };
        unsafe {
            SetTextColor(hdc, rgb(33, 37, 41));
            SetTextAlign(hdc, TA_CENTER | TA_TOP);
        }

        let mut cur_major = first_major;
        while cur_major <= last_major + major_step * 0.1 {
            if (p_min..=p_max).contains(&cur_major) || (p_max..=p_min).contains(&cur_major) {
                let x = ppm_to_x(cur_major);
                if x >= plot_x - 1 && x <= plot_x + plot_w + 1 {
                    let tick_pts = [
                        POINT { x, y: axis_y },
                        POINT { x, y: axis_y + (dpi_y as f32 * 0.05) as i32 },
                    ];
                    unsafe {
                        Polyline(hdc, tick_pts.as_ptr(), 2);
                    }

                    let label = if major_step < 0.1 {
                        format!("{:.2}", cur_major)
                    } else if major_step < 1.0 {
                        format!("{:.1}", cur_major)
                    } else {
                        format!("{:.0}", cur_major)
                    };
                    let wide = to_wide(&label);
                    unsafe {
                        TextOutW(hdc, x, axis_y + (dpi_y as f32 * 0.08) as i32, wide.as_ptr(), (wide.len() - 1) as i32);
                    }
                }
            }
            cur_major += major_step;
        }

        // "(ppm)" 単位
        unsafe {
            SetTextAlign(hdc, TA_RIGHT | TA_TOP);
            let wide = to_wide("(ppm)");
            TextOutW(hdc, plot_x + plot_w, axis_y + (dpi_y as f32 * 0.18) as i32, wide.as_ptr(), (wide.len() - 1) as i32);
            SelectObject(hdc, old_font);
            DeleteObject(font);
            SelectObject(hdc, old_pen);
            DeleteObject(axis_pen);
        }
    }

    unsafe {
        EndPage(hdc);
        EndDoc(hdc);
        DeleteDC(hdc);
    }

    Ok(())
}

/// MultiSpec 専用の印刷スタイル設定ダイアログ
/// (Peak Leader line は削除、Multiview は削除、ラベルのコロンなし)
pub fn show_multispec_print_settings_dialog(
    ctx: &egui::Context,
    state: &mut PrintStyleDialogState,
    settings: &mut PrintStyleSettings,
) {
    if !state.is_open {
        return;
    }

    Window::new(RichText::new("Print Settings").strong().size(13.5))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(320.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            ui.group(|ui| {
                ui.label(RichText::new("Spectrum Style").strong().size(12.5));
                ui.add_space(2.0);

                egui::Grid::new("multispec_print_style_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Spectrum line");
                        ui.add(
                            egui::DragValue::new(&mut settings.main_spectrum.line_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        ui.end_row();

                        ui.label("Integral curve");
                        ui.add(
                            egui::DragValue::new(&mut settings.main_spectrum.integral_width)
                                .speed(0.05)
                                .range(0.1..=10.0)
                                .suffix(" pt"),
                        );
                        ui.end_row();
                    });
            });

            ui.horizontal(|ui| {
                if ui.button("Close").clicked() {
                    state.is_open = false;
                }
            });
        });
}
