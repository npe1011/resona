use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use egui::{
    vec2, Align2, Button, Checkbox, Color32, FontFamily, FontId, Frame, Pos2, Rect,
    RichText, Stroke, Ui, Window,
};
use ndarray::Array1;
use serde::{Deserialize, Serialize};

use crate::core::{
    calc_ppm_ticks, compute_integral, AcquisitionMetadata, FtSettings, IntegrationItem, JCouplingResultItem,
    MultiviewItem, PeakItem,
};
use crate::gui::dialogs::print_style_dialog::{
    show_print_style_dialog, PrintStyleDialogState, PrintStyleSettings,
};
use crate::gui::plot::transform::PlotTransform;

/// 印刷の向き設定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PrintOrientation {
    #[default]
    Landscape,
    Portrait,
}

/// 印刷項目および設定 (Python版 ezNMR 完全準拠)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintSettings {
    pub printer_name: String,
    pub orientation: PrintOrientation,
    pub spectrum: bool,
    pub peak: bool,
    pub integrate: bool,
    pub multiview: bool,
    pub info: bool,
    pub jcoupling: bool,
    pub filename: bool,
    #[serde(default = "default_ppm_decimals")]
    pub ppm_decimals: usize,
    #[serde(default = "default_integral_decimals")]
    pub integral_decimals: usize,
    #[serde(default = "default_auto_ticks")]
    pub auto_ticks: bool,
    #[serde(default = "default_tick_major")]
    pub tick_major: f64,
    #[serde(default = "default_tick_minor")]
    pub tick_minor: usize,
}

fn default_ppm_decimals() -> usize { 3 }
fn default_integral_decimals() -> usize { 3 }
fn default_auto_ticks() -> bool { false }
fn default_tick_major() -> f64 { 1.0 }
fn default_tick_minor() -> usize { 10 }

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            printer_name: String::new(),
            orientation: PrintOrientation::Landscape,
            spectrum: true,
            peak: true,
            integrate: true,
            multiview: true,
            info: true,
            jcoupling: true,
            filename: true,
            ppm_decimals: 3,
            integral_decimals: 3,
            auto_ticks: false,
            tick_major: 1.0,
            tick_minor: 10,
        }
    }
}
// SystemPrinter は crate::gui::print より re-export
pub use crate::gui::print::SystemPrinter;

/// 印刷ダイアログの状態管理
pub struct PrintDialogState {
    pub is_open: bool,
    pub settings: PrintSettings,
    pub available_printers: Arc<Mutex<Option<Vec<SystemPrinter>>>>,
    pub is_loading_printers: bool,
    pub status_message: Option<(String, bool)>, // (メッセージ, is_error)
    pub open_counter: usize,
    pub style_settings: PrintStyleSettings,
    pub style_dialog_state: PrintStyleDialogState,
    pub devmode: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Default for PrintDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            settings: PrintSettings::default(),
            available_printers: Arc::new(Mutex::new(None)),
            is_loading_printers: false,
            status_message: None,
            open_counter: 0,
            style_settings: PrintStyleSettings::load(),
            style_dialog_state: PrintStyleDialogState::default(),
            devmode: Arc::new(Mutex::new(None)),
        }
    }
}

impl PrintDialogState {
    /// ダイアログを開き、非同期でプリンター一覧を検出する
    pub fn open(&mut self) {
        self.is_open = true;
        self.open_counter += 1;
        self.status_message = None;
        self.style_dialog_state.is_open = false;
        if let Ok(mut lock) = self.devmode.lock() {
            *lock = None;
        }

        let printers_arc = Arc::clone(&self.available_printers);
        self.is_loading_printers = true;

        std::thread::spawn(move || {
            let printers = fetch_system_printers();
            if let Ok(mut lock) = printers_arc.lock() {
                *lock = Some(printers);
            }
        });
    }

    /// プリンター一覧のリロード
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
pub use crate::gui::print::{fetch_system_printers, open_printer_preferences};
#[cfg(target_os = "windows")]
pub use crate::gui::print::{DEVMODEW, get_or_create_devmode};


/// 印刷ダイアログの表示
pub fn show_print_dialog(
    ctx: &egui::Context,
    state: &mut PrintDialogState,
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
) {
    if !state.is_open {
        return;
    }

    // 非同期ロードされたプリンター一覧の反映
    if let Ok(lock) = state.available_printers.lock() {
        if let Some(ref printers) = *lock {
            state.is_loading_printers = false;
            let current_exists = !state.settings.printer_name.is_empty()
                && printers.iter().any(|p| p.name == state.settings.printer_name);
            if !current_exists {
                if let Some(def) = printers.iter().find(|p| p.is_default) {
                    state.settings.printer_name = def.name.clone();
                } else if let Some(first) = printers.first() {
                    state.settings.printer_name = first.name.clone();
                }
            }
        }
    }

    let mut is_open = state.is_open;
    let mut should_close = false;
    let is_style_open = state.style_dialog_state.is_open;

    Window::new(RichText::new("Print Preview").strong())
        .id(egui::Id::new("print_preview_dialog_window").with(state.open_counter))
        .open(&mut is_open)
        .resizable(true)
        .default_width(740.0)
        .default_height(600.0)
        .min_width(620.0)
        .min_height(500.0)
        .show(ctx, |ui| {
            if is_style_open {
                ui.disable();
            }
            ui.spacing_mut().item_spacing.y = 8.0;

            // 1. 上部コントロール (プリンター選択 & ページ設定)
            ui.horizontal(|ui| {
                ui.label(RichText::new("Printer").strong().size(12.0));

                let current_selection = if state.settings.printer_name.is_empty() {
                    if state.is_loading_printers {
                        "Detecting printers..."
                    } else {
                        "Select Printer"
                    }
                } else {
                    &state.settings.printer_name
                };

                egui::ComboBox::from_id_salt("print_printer_select")
                    .selected_text(current_selection)
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        if let Ok(lock) = state.available_printers.lock() {
                            if let Some(ref printers) = *lock {
                                for p in printers {
                                    let label = if p.is_default {
                                        format!("{} (Default)", p.name)
                                    } else {
                                        p.name.clone()
                                    };
                                    if ui.selectable_value(
                                        &mut state.settings.printer_name,
                                        p.name.clone(),
                                        label,
                                    ).changed() {
                                        if let Ok(mut lock) = state.devmode.lock() {
                                            *lock = None;
                                        }
                                    }
                                }
                            }
                        }
                    });

                let p_name = state.settings.printer_name.clone();
                let orient = state.settings.orientation;
                let dev_arc = Arc::clone(&state.devmode);
                if ui.button("Detail...").clicked() && !p_name.is_empty() {
                    std::thread::spawn(move || {
                        open_printer_preferences(&p_name, orient, dev_arc);
                    });
                }

                if ui.button("Refresh").clicked() {
                    state.refresh_printers();
                }

                ui.separator();

                ui.label(RichText::new("Orientation").strong().size(12.0));
                if ui.selectable_value(
                    &mut state.settings.orientation,
                    PrintOrientation::Landscape,
                    "Landscape",
                ).changed() {
                    if let Ok(mut lock) = state.devmode.lock() {
                        *lock = None;
                    }
                }
                if ui.selectable_value(
                    &mut state.settings.orientation,
                    PrintOrientation::Portrait,
                    "Portrait",
                ).changed() {
                    if let Ok(mut lock) = state.devmode.lock() {
                        *lock = None;
                    }
                }
            });

            // 2. 印刷項目チェックボックス & アクションボタン (横並びでマウス動線を改善)
            ui.horizontal(|ui| {
                // 左側: 印刷項目
                Frame::group(ui.style()).show(ui, |ui| {
                    egui::Grid::new("print_items_grid")
                        .spacing(vec2(16.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Print Items").strong().size(12.0));
                            ui.add(Checkbox::new(&mut state.settings.spectrum, "Spectrum"));
                            ui.add(Checkbox::new(&mut state.settings.peak, "Peak Pick"));
                            ui.add(Checkbox::new(&mut state.settings.integrate, "Integrals"));
                            ui.add(Checkbox::new(&mut state.settings.multiview, "Multiview"));
                            ui.end_row();

                            ui.label("");
                            ui.add(Checkbox::new(&mut state.settings.info, "Parameters"));
                            ui.add(Checkbox::new(&mut state.settings.jcoupling, "J Coupling Table"));
                            ui.add(Checkbox::new(&mut state.settings.filename, "File Name"));
                            ui.label("");
                            ui.end_row();
                        });
                });

                ui.add_space(8.0);

                // 右側: アクションボタン (Print, Export SVG, Cancel)
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        // 印刷ボタン (プライマリ: 青背景・白文字)
                        let btn_print = Button::new(RichText::new("Print").strong().size(13.0).color(Color32::WHITE))
                            .min_size(vec2(85.0, 26.0))
                            .fill(Color32::from_rgb(13, 110, 253))
                            .rounding(3.0_f32);
                        if ui.add(btn_print).clicked() {
                            let devmode_copy = state.devmode.lock().ok().and_then(|g| g.clone());
                            match execute_native_print(
                                &state.settings,
                                &state.style_settings,
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
                                devmode_copy.as_deref(),
                            ) {
                                Ok(Some(info_msg)) => {
                                    state.status_message = Some((info_msg, false));
                                }
                                Ok(None) => {
                                    should_close = true;
                                }
                                Err(e) => {
                                    state.status_message = Some((format!("Print failed: {}", e), true));
                                }
                            }
                        }

                        // 完全ベクター SVG ファイル保存ボタン
                        let btn_svg = Button::new(RichText::new("Export SVG...").size(12.5))
                            .min_size(vec2(95.0, 26.0))
                            .rounding(3.0_f32);
                        if ui.add(btn_svg).clicked() {
                            let default_svg_name = current_filepath
                                .and_then(|p| p.file_stem())
                                .and_then(|s| s.to_str())
                                .map(|s| format!("{}.svg", s))
                                .unwrap_or_else(|| "resona_report.svg".to_string());

                            if let Some(target) = rfd::FileDialog::new()
                                .set_title("Export Complete Report as Vector SVG")
                                .add_filter("Scalable Vector Graphics", &["svg"])
                                .set_file_name(&default_svg_name)
                                .save_file()
                            {
                                let svg = generate_complete_page_svg_with_style(
                                    &state.settings,
                                    &state.style_settings,
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
                                );
                                if let Err(e) = fs::write(&target, svg) {
                                    state.status_message = Some((format!("Export failed: {}", e), true));
                                }
                            }
                        }

                        // Cancel ボタン
                        let btn_cancel = Button::new(RichText::new("Cancel").size(12.5))
                            .min_size(vec2(65.0, 26.0))
                            .rounding(3.0_f32);
                        if ui.add(btn_cancel).clicked() {
                            should_close = true;
                        }
                    });

                    // 右詰めで Print Settings ボタン
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_settings = Button::new(RichText::new("Print Settings...").size(12.0))
                            .min_size(vec2(105.0, 24.0))
                            .rounding(3.0_f32);
                        if ui.add(btn_settings).clicked() {
                            state.style_dialog_state.is_open = true;
                        }
                    });

                    // ステータス / エラーメッセージ表示
                    if let Some((ref msg, is_err)) = state.status_message {
                        if is_err {
                            ui.label(RichText::new(msg).size(11.0).color(Color32::from_rgb(220, 38, 38)));
                        }
                    }
                });
            });

            // 3. リアルタイムプレビュー領域
            ui.separator();

            let avail_size = ui.available_size();
            render_realtime_preview(
                ui,
                avail_size,
                &state.settings,
                &state.style_settings,
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
            );
        });

    if should_close {
        is_open = false;
    }
    state.is_open = is_open;

    // 親ダイアログが閉じている場合、子ダイアログ (Print Settings) も必ず閉じる
    if !state.is_open {
        state.style_dialog_state.is_open = false;
    } else {
        show_print_style_dialog(ctx, &mut state.style_dialog_state, &mut state.style_settings);
    }
}

/// ダイアログ内のリアルタイムプレビュー描画 (画面見た目通りに忠実再現)
fn render_realtime_preview(
    ui: &mut Ui,
    avail_size: egui::Vec2,
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

    let mut cur_y = page_rect.min.y + 6.0;

    // 1. ファイル名 (ヘッダー: 高さを固定して File Name の有無でプロットが動かないようにする)
    let header_h = 16.0_f32;
    if settings.filename {
        let name_str = current_filepath
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "No file loaded".to_string());
        painter.text(
            Pos2::new(page_rect.min.x + 10.0, cur_y),
            Align2::LEFT_TOP,
            &name_str,
            FontId::new(9.0, FontFamily::Proportional),
            Color32::from_rgb(108, 117, 125),
        );
    }
    cur_y += header_h;

    // 2. カラム分割 (プロット領域 vs パラメータ領域)
    let has_side_info = settings.info || (settings.jcoupling && !j_couplings.is_empty());
    let side_w = if has_side_info {
        (page_rect.width() * 0.14).clamp(65.0, 95.0)
    } else {
        0.0
    };

    let margin_side = 4.0_f32;
    let gap = if has_side_info { 6.0_f32 } else { 0.0_f32 };
    let plot_rect = Rect::from_min_max(
        Pos2::new(page_rect.min.x + margin_side, cur_y),
        Pos2::new(page_rect.max.x - margin_side - side_w - gap, page_rect.max.y - margin_side),
    );

    if let (Some(ppm_arr), Some(spec_arr)) = (ppm, spectrum) {
        if !ppm_arr.is_empty() && !spec_arr.is_empty() {
            let (p_min, p_max, y_min, y_max, main_screen_rect) = if let Some(t) = main_transform {
                (t.ppm_min, t.ppm_max, t.y_min, t.y_max, t.screen_rect)
            } else {
                let p_min_data = ppm_arr.iter().cloned().fold(f64::INFINITY, f64::min);
                let p_max_data = ppm_arr.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let y_max_data = spec_arr.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(1.0);
                (p_min_data, p_max_data, -0.05 * y_max_data, y_max_data * 1.1, plot_rect)
            };

            let preview_bottom_margin = 44.0_f32;
            let t = PlotTransform::new(plot_rect, p_min, p_max, y_min, y_max)
                .with_bottom_margin(preview_bottom_margin);
            let axis_y = t.axis_y();

            // X軸ベースライン
            painter.line_segment(
                [Pos2::new(plot_rect.min.x, axis_y), Pos2::new(plot_rect.max.x, axis_y)],
                Stroke::new(1.0_f32, Color32::from_rgb(40, 40, 40)),
            );

            // スペクトル曲線
            if settings.spectrum {
                let n_pts = ppm_arr.len().min(spec_arr.len());
                let p_low = p_min.min(p_max);
                let p_high = p_min.max(p_max);
                let mut points = Vec::new();

                for i in 0..n_pts {
                    let p = ppm_arr[i];
                    if p >= p_low && p <= p_high {
                        let pt = t.data_to_screen(p, spec_arr[i]);
                        points.push(Pos2::new(pt.x, pt.y.min(axis_y)));
                    }
                }
                if points.len() > 1 {
                    painter.add(egui::epaint::PathShape::line(
                        points,
                        Stroke::new(style.main_spectrum.line_width, style.main_spectrum.line_color.to_color32()),
                    ));
                }
            }

            // X軸目盛り & PPM ラベル
            let span = (p_max - p_min).abs();
            let (tick_interval, tick_dec) = calc_ppm_ticks(span, settings.auto_ticks, settings.tick_major);

            let p_low = p_min.min(p_max);
            let p_high = p_min.max(p_max);
            let minor_n = settings.tick_minor.max(1);
            let minor_step = tick_interval / (minor_n as f64);

            let first_tick = (p_low / tick_interval).ceil() * tick_interval;
            let mut cur_tick = first_tick;
            while cur_tick <= p_high {
                let tx = t.ppm_to_screen_x(cur_tick);
                if tx >= plot_rect.min.x && tx <= plot_rect.max.x {
                    // 主目盛り線
                    painter.line_segment(
                        [Pos2::new(tx, axis_y), Pos2::new(tx, axis_y + 3.0)],
                        Stroke::new(1.0_f32, Color32::from_rgb(40, 40, 40)),
                    );
                    painter.text(
                        Pos2::new(tx, axis_y + 4.0),
                        Align2::CENTER_TOP,
                        format!("{:.1$}", cur_tick, tick_dec),
                        FontId::new(7.5, FontFamily::Proportional),
                        Color32::from_rgb(50, 50, 50),
                    );
                }

                // サブ目盛り (StepをN分割した目盛りマーク、数字なし)
                if minor_n > 1 {
                    for m in 1..minor_n {
                        let sub_tick = cur_tick + (m as f64) * minor_step;
                        if sub_tick <= p_high {
                            let stx = t.ppm_to_screen_x(sub_tick);
                            if stx >= plot_rect.min.x && stx <= plot_rect.max.x {
                                painter.line_segment(
                                    [Pos2::new(stx, axis_y), Pos2::new(stx, axis_y + 1.5)],
                                    Stroke::new(0.8_f32, Color32::from_rgb(100, 100, 100)),
                                );
                            }
                        }
                    }
                }

                cur_tick += tick_interval;
            }

            // 積分
            if settings.integrate {
                for it in integrations {
                    let p_start = it.start_ppm.max(it.end_ppm);
                    let p_end = it.start_ppm.min(it.end_ppm);
                    let sx_start = t.ppm_to_screen_x(p_start);
                    let sx_end = t.ppm_to_screen_x(p_end);

                    if sx_end > plot_rect.min.x && sx_start < plot_rect.max.x {
                        let bl_start = t.data_to_screen(it.start_ppm, it.y_start);
                        let bl_end = t.data_to_screen(it.end_ppm, it.y_end);
                        painter.line_segment(
                            [bl_start, bl_end],
                            Stroke::new(1.0_f32, Color32::from_rgb(59, 130, 246)),
                        );

                        if let Some(res) = compute_integral(spec_arr, ppm_arr, it, integration_scale, integration_ref_factor, integration_offset) {
                            if res.ppm.len() > 1 && res.ppm.len() == res.curve_y.len() {
                                let mut pts = Vec::with_capacity(res.ppm.len());
                                let mut min_y = f32::MAX;
                                for idx in 0..res.ppm.len() {
                                    let pt = t.data_to_screen(res.ppm[idx], res.curve_y[idx]);
                                    pts.push(Pos2::new(pt.x, pt.y.min(axis_y)));
                                    min_y = min_y.min(pt.y);
                                }
                                painter.add(egui::epaint::PathShape::line(
                                    pts,
                                    Stroke::new(style.main_spectrum.integral_width, style.main_spectrum.integral_color.to_color32()),
                                ));

                                let mid_x = (sx_start + sx_end) * 0.5;
                                let val_text = format!("{:.1$}", res.normalized_value, settings.integral_decimals);
                                let font_intg = FontId::new(7.0, FontFamily::Proportional);
                                let color_intg = style.main_spectrum.integral_color.to_color32();
                                let galley = painter.layout_no_wrap(val_text, font_intg, color_intg);
                                let text_len = galley.size().x;
                                let text_h = galley.size().y;
                                let start_y = (min_y - 2.0 - text_len).max(plot_rect.min.y + 2.0);
                                let text_pos = Pos2::new(mid_x + text_h * 0.5, start_y);
                                let ts = egui::epaint::TextShape::new(text_pos, galley, color_intg)
                                    .with_angle(std::f32::consts::FRAC_PI_2);
                                painter.add(ts);
                            }
                        }
                    }
                }
            }

            // ピークピック (アンチコリジョン引き出し線)
            if settings.peak {
                let mut visible_peaks: Vec<&PeakItem> = peaks
                    .iter()
                    .filter(|p| p.ppm >= p_low && p.ppm <= p_high)
                    .collect();
                visible_peaks.sort_by(|a, b| b.ppm.partial_cmp(&a.ppm).unwrap_or(std::cmp::Ordering::Equal));

                if !visible_peaks.is_empty() {
                    let mut text_x: Vec<f32> = visible_peaks
                        .iter()
                        .map(|p| t.ppm_to_screen_x(p.ppm))
                        .collect();

                    let min_gap = 8.5_f32;
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

                    let y_elbow = axis_y + 11.0;
                    let y_text_start = axis_y + 22.0;
                    let font_peak = FontId::new(6.5, FontFamily::Proportional);

                    for (i, pk) in visible_peaks.iter().enumerate() {
                        let px = t.ppm_to_screen_x(pk.ppm);
                        let tx = text_x[i];

                        // 枠外にはみ出るものはスキップ (omit)
                        if tx < plot_rect.min.x + 2.0 || tx > plot_rect.max.x - 2.0 || px < plot_rect.min.x || px > plot_rect.max.x {
                            continue;
                        }

                        // ピーク位置からベースラインの上約 6px 付近から開始し、X軸を突き抜けて y_elbow まで伸ばす (GUI準拠)
                        let pk_sy = t.data_to_screen(pk.ppm, pk.intensity).y;
                        let y_start = (axis_y - 6.0).max(pk_sy + 1.5);

                        let stroke_lead = Stroke::new(style.main_spectrum.peak_lead_width, style.main_spectrum.peak_lead_color.to_color32());
                        painter.line_segment([Pos2::new(px, y_start), Pos2::new(px, y_elbow)], stroke_lead);
                        painter.line_segment([Pos2::new(px, y_elbow), Pos2::new(tx, y_text_start - 2.5)], stroke_lead);
                        painter.line_segment([Pos2::new(tx, y_text_start - 2.5), Pos2::new(tx, y_text_start)], stroke_lead);

                        let val_str = format!("{:.1$}", pk.ppm, settings.ppm_decimals);
                        let galley = painter.layout_no_wrap(val_str, font_peak.clone(), Color32::from_rgb(20, 20, 20));
                        let text_h = galley.size().y;
                        let pos = Pos2::new(tx + text_h * 0.5, y_text_start + 2.0);
                        let ts = egui::epaint::TextShape::new(pos, galley, Color32::from_rgb(20, 20, 20))
                            .with_angle(std::f32::consts::FRAC_PI_2);
                        painter.add(ts);
                    }
                }
            }

            // マルチビュー (拡大スペクトル、積分、ピーク引き出し線、X軸目盛り)
            if settings.multiview && !multiviews.is_empty() {
                for mv in multiviews {
                    let rel_x = (mv.geometry.x - main_screen_rect.min.x) / main_screen_rect.width().max(1.0);
                    let rel_y = (mv.geometry.y - main_screen_rect.min.y) / main_screen_rect.height().max(1.0);
                    let rel_w = mv.geometry.w / main_screen_rect.width().max(1.0);
                    let rel_h = mv.geometry.h / main_screen_rect.height().max(1.0);

                    let inset_x = plot_rect.min.x + rel_x * plot_rect.width();
                    let inset_y = plot_rect.min.y + rel_y * plot_rect.height();
                    let inset_w = rel_w * plot_rect.width();
                    let inset_h = rel_h * plot_rect.height();

                    if inset_w > 25.0 && inset_h > 25.0 {
                        let inset_rect = Rect::from_min_size(Pos2::new(inset_x, inset_y), vec2(inset_w, inset_h));
                        painter.rect_filled(inset_rect, 0.0, Color32::WHITE);
                        painter.rect_stroke(inset_rect, 0.0, Stroke::new(1.1_f32, Color32::from_gray(135)));

                        let mv_src_min = mv.src_x_min.min(mv.src_x_max);
                        let mv_src_max = mv.src_x_min.max(mv.src_x_max);

                        let mut mv_y_min = f64::INFINITY;
                        let mut mv_y_max = f64::NEG_INFINITY;
                        for idx in 0..ppm_arr.len().min(spec_arr.len()) {
                            let p = ppm_arr[idx];
                            if p >= mv_src_min && p <= mv_src_max {
                                let v = spec_arr[idx];
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

                        let inset_axis_y = inset_rect.max.y - 12.0;
                        let inset_plot_h = (inset_axis_y - inset_rect.min.y - 4.0).max(10.0);
                        let inset_plot_w = (inset_rect.width() - 8.0).max(10.0);

                        let mv_ppm_to_x = |p: f64| -> f32 {
                            inset_rect.min.x + 4.0 + (((mv_src_max - p) / (mv_src_max - mv_src_min).max(1e-6)) as f32) * inset_plot_w
                        };
                        let mv_y_to_y = |y: f64| -> f32 {
                            inset_axis_y - (((y - y_min_adj) / (y_max_adj - y_min_adj).max(1e-6)) as f32) * inset_plot_h
                        };

                        // 1. 拡大スペクトル曲線 (GUI準拠の黒色)
                        let mut mv_points = Vec::new();
                        for idx in 0..ppm_arr.len().min(spec_arr.len()) {
                            let p = ppm_arr[idx];
                            if p >= mv_src_min && p <= mv_src_max {
                                let sx = mv_ppm_to_x(p);
                                let sy = mv_y_to_y(spec_arr[idx]);
                                mv_points.push(Pos2::new(sx, sy.min(inset_axis_y)));
                            }
                        }
                        if mv_points.len() > 1 {
                            painter.add(egui::epaint::PathShape::line(
                                mv_points,
                                Stroke::new(style.multiview.line_width, style.multiview.line_color.to_color32()),
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
                                if let Some(res) = compute_integral(spec_arr, ppm_arr, integ, 1.0, integration_ref_factor, 0.0) {
                                    if res.ppm.len() > 1 && res.total_area.abs() > 1e-12 {
                                        let mut intg_pts = Vec::new();
                                        for (&p, &cy) in res.ppm.iter().zip(res.curve_y.iter()) {
                                            if p >= mv_src_min && p <= mv_src_max {
                                                let bl = integ.baseline_y_at(p);
                                                let cum = cy - bl;
                                                let norm_y = (cum / res.total_area).clamp(0.0, 1.0);
                                                let target_data_y = y_min_adj + 0.20 * h_diff + norm_y * (0.45 * h_diff);
                                                intg_pts.push(Pos2::new(mv_ppm_to_x(p), mv_y_to_y(target_data_y)));
                                            }
                                        }
                                        if intg_pts.len() > 1 {
                                            painter.add(egui::epaint::PathShape::line(
                                                intg_pts,
                                                Stroke::new(style.multiview.integral_width, style.multiview.integral_color.to_color32()),
                                            ));
                                            let mid_p = (i_min.max(mv_src_min) + i_max.min(mv_src_max)) * 0.5;
                                            let mid_x = mv_ppm_to_x(mid_p);
                                            let top_y = mv_y_to_y(y_min_adj + 0.70 * h_diff);
                                            let val_text = format!("{:.1$}", res.normalized_value, settings.integral_decimals);
                                            let font_intg = FontId::new(6.5, FontFamily::Proportional);
                                            let color_intg = style.multiview.integral_color.to_color32();
                                            let galley = painter.layout_no_wrap(val_text, font_intg, color_intg);
                                            let text_len = galley.size().x;
                                            let text_h = galley.size().y;
                                            let start_y = (top_y - text_len).max(inset_rect.min.y + 2.0);
                                            let text_pos = Pos2::new(mid_x + text_h * 0.5, start_y);
                                            let ts = egui::epaint::TextShape::new(text_pos, galley, color_intg)
                                                .with_angle(std::f32::consts::FRAC_PI_2);
                                            painter.add(ts);
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
                                let mut screen_x_list: Vec<f32> = sorted_peaks.iter().map(|p| mv_ppm_to_x(p.ppm)).collect();
                                let min_gap_px = 7.5_f32;
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
                                    if !moved { break; }
                                }

                                let mv_font_peak = FontId::new(5.5, FontFamily::Proportional);
                                let mv_text_start_y = inset_rect.min.y + 3.0;
                                let text_len = 14.0_f32;
                                let text_bottom_y = mv_text_start_y + text_len;
                                let mv_elbow_y = text_bottom_y + 3.0;
                                let max_lead_y = (inset_rect.min.y + inset_plot_h * 0.25).max(mv_elbow_y + 3.0);

                                for (i, pk) in sorted_peaks.iter().enumerate() {
                                    let px = mv_ppm_to_x(pk.ppm);
                                    let tx = screen_x_list[i];

                                    // 枠外にはみ出るものはスキップ (omit)
                                    if tx < inset_rect.min.x + 3.0 || tx > inset_rect.max.x - 3.0 || px < inset_rect.min.x || px > inset_rect.max.x {
                                        continue;
                                    }

                                    let py = mv_y_to_y(pk.intensity);
                                    let clearance = 4.0_f32;
                                    let line_start_y = (py - clearance).min(max_lead_y);

                                    if line_start_y > mv_elbow_y + 1.0 {
                                        let stroke_lead = Stroke::new(style.multiview.peak_lead_width, style.multiview.peak_lead_color.to_color32());
                                        painter.line_segment([Pos2::new(px, line_start_y), Pos2::new(px, mv_elbow_y)], stroke_lead);
                                        painter.line_segment([Pos2::new(px, mv_elbow_y), Pos2::new(tx, text_bottom_y + 1.5)], stroke_lead);
                                        painter.line_segment([Pos2::new(tx, text_bottom_y + 1.5), Pos2::new(tx, text_bottom_y)], stroke_lead);
                                    }

                                    let val_str = format!("{:.1$}", pk.ppm, settings.ppm_decimals);
                                    let galley = painter.layout_no_wrap(val_str, mv_font_peak.clone(), Color32::from_rgb(20, 20, 20));
                                    let text_h = galley.size().y;
                                    let pos = Pos2::new(tx + text_h * 0.5, mv_text_start_y);
                                    let ts = egui::epaint::TextShape::new(pos, galley, Color32::from_rgb(20, 20, 20))
                                        .with_angle(std::f32::consts::FRAC_PI_2);
                                    painter.add(ts);
                                }
                            }
                        }

                        // 4. X 軸目盛り線 & PPM 数値ラベル (マルチビュー内)
                        painter.line_segment(
                            [Pos2::new(inset_rect.min.x + 4.0, inset_axis_y), Pos2::new(inset_rect.max.x - 4.0, inset_axis_y)],
                            Stroke::new(0.8_f32, Color32::BLACK),
                        );

                        let ppm_span = mv_src_max - mv_src_min;
                        let target_ticks = (inset_plot_w / 35.0).clamp(2.0, 5.0) as f64;
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
                            if m_x >= inset_rect.min.x && m_x <= inset_rect.max.x {
                                painter.line_segment(
                                    [Pos2::new(m_x, inset_axis_y), Pos2::new(m_x, inset_axis_y + 1.2)],
                                    Stroke::new(0.5_f32, Color32::from_gray(120)),
                                );
                            }
                        }

                        // メイン目盛り
                        for t_idx in start_tick..=end_tick {
                            let tick_ppm = t_idx as f64 * step;
                            let tick_x = mv_ppm_to_x(tick_ppm);
                            if tick_x >= inset_rect.min.x && tick_x <= inset_rect.max.x {
                                painter.line_segment(
                                    [Pos2::new(tick_x, inset_axis_y), Pos2::new(tick_x, inset_axis_y + 2.5)],
                                    Stroke::new(0.7_f32, Color32::BLACK),
                                );
                                if tick_x >= inset_rect.min.x + 8.0 && tick_x <= inset_rect.max.x - 8.0 {
                                    painter.text(
                                        Pos2::new(tick_x, inset_axis_y + 3.0),
                                        Align2::CENTER_TOP,
                                        format!("{:.1$}", tick_ppm, decimals),
                                        FontId::new(6.0, FontFamily::Proportional),
                                        Color32::BLACK,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        painter.text(
            plot_rect.center(),
            Align2::CENTER_CENTER,
            "No Spectrum Loaded",
            FontId::proportional(12.0),
            Color32::from_gray(160),
        );
    }

    // 3. 右側パラメータ領域 (画面のサイドパネルと100%同一の全行を表示)
    if has_side_info {
        let side_rect = Rect::from_min_max(
            Pos2::new(page_rect.max.x - margin_side - side_w, cur_y),
            Pos2::new(page_rect.max.x - margin_side, page_rect.max.y - margin_side),
        );
        let mut text_y = side_rect.min.y;

        if settings.info {
            // 1. Experimental Parameters (画面一番上と同じ)
            painter.text(
                Pos2::new(side_rect.min.x, text_y),
                Align2::LEFT_TOP,
                "Experimental Parameters",
                FontId::new(7.5, FontFamily::Proportional),
                Color32::from_rgb(33, 37, 41),
            );
            text_y += 10.0;

            let exp_rows = metadata.to_display_rows();
            for (k, v) in exp_rows {
                let s = format!("{}: {}", k, v);
                painter.text(
                    Pos2::new(side_rect.min.x, text_y),
                    Align2::LEFT_TOP,
                    s,
                    FontId::new(6.0, FontFamily::Proportional),
                    Color32::from_rgb(73, 80, 87),
                );
                text_y += 8.0;
                if text_y > side_rect.max.y - 80.0 { break; }
            }
            text_y += 4.0;

            // 2. FT Settings
            if text_y < side_rect.max.y - 40.0 {
                painter.text(
                    Pos2::new(side_rect.min.x, text_y),
                    Align2::LEFT_TOP,
                    "FT Settings",
                    FontId::new(7.5, FontFamily::Proportional),
                    Color32::from_rgb(33, 37, 41),
                );
                text_y += 10.0;

                let eff_points = (metadata.points * ft_settings.zf_factor).max(1);
                let dig_res = format!("{:.4} Hz/pt", metadata.spectral_width_hz / (eff_points as f64));
                let ft_items = [
                    format!("Win: {}", match ft_settings.window {
                        crate::core::WindowFunction::None => "None".to_string(),
                        crate::core::WindowFunction::Exponential { lb } => format!("Exp ({}Hz)", lb),
                        crate::core::WindowFunction::Gaussian { g1, g2, g3 } => format!("G({},{},{})", g1, g2, g3),
                    }),
                    format!("ZF: {}x", ft_settings.zf_factor),
                    format!("Res: {}", dig_res),
                    format!("GD: {}", if ft_settings.remove_digital_filter { "Removed" } else { "Kept" }),
                ];

                for item in ft_items {
                    painter.text(
                        Pos2::new(side_rect.min.x, text_y),
                        Align2::LEFT_TOP,
                        item,
                        FontId::new(6.0, FontFamily::Proportional),
                        Color32::from_rgb(73, 80, 87),
                    );
                    text_y += 8.0;
                }
                text_y += 4.0;
            }
        }

        if settings.jcoupling && !j_couplings.is_empty() && text_y < side_rect.max.y - 20.0 {
            painter.text(
                Pos2::new(side_rect.min.x, text_y),
                Align2::LEFT_TOP,
                "J Coupling",
                FontId::new(7.5, FontFamily::Proportional),
                Color32::from_rgb(33, 37, 41),
            );
            text_y += 10.0;

            for jc in j_couplings.iter().take(5) {
                let s = jc.text.clone();
                painter.text(
                    Pos2::new(side_rect.min.x, text_y),
                    Align2::LEFT_TOP,
                    s,
                    FontId::new(6.0, FontFamily::Proportional),
                    Color32::from_rgb(73, 80, 87),
                );
                text_y += 8.0;
            }
        }
    }
}
// ----------------------------------------------------------------------------
// SVG 生成 (crate::gui::export::svg へ分離・re-export)
// ----------------------------------------------------------------------------
pub use crate::gui::export::svg::{generate_complete_page_svg, generate_complete_page_svg_with_style};

// ----------------------------------------------------------------------------
// ネイティブ印刷 (crate::gui::print へ分離・re-export)
// ----------------------------------------------------------------------------
pub use crate::gui::print::execute_native_print;

