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
    compute_integral, AcquisitionMetadata, FtSettings, IntegrationItem, JCouplingResultItem,
    MultiviewItem, PeakItem,
};
use crate::gui::dialogs::print_style_dialog::{
    show_print_style_dialog, PrintStyleDialogState, PrintStyleSettings,
};
use crate::gui::plot::transform::PlotTransform;

/// 印刷の向き設定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrintOrientation {
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

/// システムで検出されたプリンター情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPrinter {
    pub name: String,
    pub is_default: bool,
}

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
        }
    }
}

impl PrintDialogState {
    /// ダイアログを開き、非同期でプリンター一覧を検出する
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

/// OSのプリンター詳細設定ダイアログ (Print Preferences) を開く関数
#[cfg(target_os = "windows")]
pub fn open_printer_preferences(printer_name: &str) {
    #[link(name = "winspool")]
    unsafe extern "system" {
        fn OpenPrinterW(pPrinterName: *const u16, phPrinter: *mut isize, pDefault: *const std::ffi::c_void) -> i32;
        fn ClosePrinter(hPrinter: isize) -> i32;
        fn DocumentPropertiesW(
            hWnd: isize,
            hPrinter: isize,
            pDeviceName: *const u16,
            pDevModeOutput: *mut std::ffi::c_void,
            pDevModeInput: *mut std::ffi::c_void,
            fMode: u32,
        ) -> i32;
    }
    const DM_IN_PROMPT: u32 = 4;
    let wide_name: Vec<u16> = printer_name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut h_printer: isize = 0;
    unsafe {
        if OpenPrinterW(wide_name.as_ptr(), &mut h_printer, std::ptr::null()) != 0 {
            DocumentPropertiesW(0, h_printer, wide_name.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), DM_IN_PROMPT);
            ClosePrinter(h_printer);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn open_printer_preferences(_printer_name: &str) {}

/// OSネイティブにプリンター一覧を検出する関数
fn fetch_system_printers() -> Vec<SystemPrinter> {
    #[cfg(target_os = "windows")]
    {
        fetch_windows_printers()
    }
    #[cfg(target_os = "macos")]
    {
        fetch_macos_printers()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        fetch_linux_printers()
    }
}

#[cfg(target_os = "windows")]
fn fetch_windows_printers() -> Vec<SystemPrinter> {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct PRINTER_INFO_4W {
        pPrinterName: *const u16,
        pServerName: *const u16,
        Attributes: u32,
    }

    #[link(name = "winspool")]
    unsafe extern "system" {
        fn EnumPrintersW(
            flags: u32,
            name: *const u16,
            level: u32,
            pPrinterEnum: *mut u8,
            cbBuf: u32,
            pcbNeeded: *mut u32,
            pcReturned: *mut u32,
        ) -> i32;
        fn GetDefaultPrinterW(pszBuffer: *mut u16, pcchBuffer: *mut u32) -> i32;
    }

    const PRINTER_ENUM_LOCAL: u32 = 0x00000002;
    const PRINTER_ENUM_CONNECTIONS: u32 = 0x00000004;

    // デフォルトプリンター名の取得
    let mut default_printer_name = String::new();
    let mut def_buf = vec![0u16; 512];
    let mut def_len = def_buf.len() as u32;
    unsafe {
        if GetDefaultPrinterW(def_buf.as_mut_ptr(), &mut def_len) != 0 {
            if let Some(pos) = def_buf.iter().position(|&c| c == 0) {
                default_printer_name = String::from_utf16_lossy(&def_buf[..pos]);
            }
        }
    }

    let mut result = Vec::new();
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let mut needed = 0u32;
    let mut returned = 0u32;

    // まず必要なバッファサイズを取得
    unsafe {
        EnumPrintersW(
            flags,
            std::ptr::null(),
            4,
            std::ptr::null_mut(),
            0,
            &mut needed,
            &mut returned,
        );
    }

    if needed > 0 {
        let mut buffer = vec![0u8; needed as usize];
        let success = unsafe {
            EnumPrintersW(
                flags,
                std::ptr::null(),
                4,
                buffer.as_mut_ptr(),
                needed,
                &mut needed,
                &mut returned,
            )
        };

        if success != 0 && returned > 0 {
            let infos = buffer.as_ptr() as *const PRINTER_INFO_4W;
            for i in 0..returned as usize {
                unsafe {
                    let info = &*infos.add(i);
                    if !info.pPrinterName.is_null() {
                        let mut len = 0;
                        while *info.pPrinterName.add(len) != 0 {
                            len += 1;
                        }
                        let name = String::from_utf16_lossy(std::slice::from_raw_parts(info.pPrinterName, len));
                        let is_default = if !default_printer_name.is_empty() {
                            name.eq_ignore_ascii_case(&default_printer_name)
                        } else {
                            i == 0
                        };
                        result.push(SystemPrinter {
                            name,
                            is_default,
                        });
                    }
                }
            }
        }
    }

    // もし EnumPrintersW で取得できなかった場合のフォールバック (PowerShell を CREATE_NO_WINDOW で実行)
    if result.is_empty() {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let cmd = "Get-CimInstance Win32_Printer | Select-Object Name, Default | ConvertTo-Json";
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", cmd])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                #[derive(Deserialize)]
                struct WinPrinter {
                    #[serde(rename = "Name")]
                    name: String,
                    #[serde(rename = "Default")]
                    is_default: bool,
                }

                if let Ok(list) = serde_json::from_str::<Vec<WinPrinter>>(&text) {
                    for p in list {
                        result.push(SystemPrinter {
                            name: p.name,
                            is_default: p.is_default,
                        });
                    }
                } else if let Ok(single) = serde_json::from_str::<WinPrinter>(&text) {
                    result.push(SystemPrinter {
                        name: single.name,
                        is_default: single.is_default,
                    });
                }
            }
        }
    }

    if result.is_empty() {
        let name = if !default_printer_name.is_empty() {
            default_printer_name
        } else {
            "Microsoft Print to PDF".to_string()
        };
        result.push(SystemPrinter {
            name,
            is_default: true,
        });
    }

    result
}

#[cfg(target_os = "macos")]
fn fetch_macos_printers() -> Vec<SystemPrinter> {
    let default_name = std::process::Command::new("lpstat")
        .arg("-d")
        .output()
        .ok()
        .and_then(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            s.split(':').nth(1).map(|p| p.trim().to_string())
        })
        .unwrap_or_default();

    let output = std::process::Command::new("lpstat")
        .arg("-p")
        .output();

    let mut result = Vec::new();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("printer ") {
                if let Some(name) = rest.split_whitespace().next() {
                    let is_default = name == default_name;
                    result.push(SystemPrinter {
                        name: name.to_string(),
                        is_default,
                    });
                }
            }
        }
    }

    if result.is_empty() {
        result.push(SystemPrinter {
            name: "Default Printer".to_string(),
            is_default: true,
        });
    }

    result
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn fetch_linux_printers() -> Vec<SystemPrinter> {
    fetch_macos_printers()
}

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
                                    ui.selectable_value(
                                        &mut state.settings.printer_name,
                                        p.name.clone(),
                                        label,
                                    );
                                }
                            }
                        }
                    });

                let p_name = state.settings.printer_name.clone();
                if ui.button("Detail...").clicked() && !p_name.is_empty() {
                    std::thread::spawn(move || {
                        open_printer_preferences(&p_name);
                    });
                }

                if ui.button("Refresh").clicked() {
                    state.refresh_printers();
                }

                ui.separator();

                ui.label(RichText::new("Orientation").strong().size(12.0));
                ui.selectable_value(
                    &mut state.settings.orientation,
                    PrintOrientation::Landscape,
                    "Landscape",
                );
                ui.selectable_value(
                    &mut state.settings.orientation,
                    PrintOrientation::Portrait,
                    "Portrait",
                );
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
                            ) {
                                Ok(()) => {
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
                            if let Some(target) = rfd::FileDialog::new()
                                .set_title("Export Complete Report as Vector SVG")
                                .add_filter("Scalable Vector Graphics", &["svg"])
                                .set_file_name("resona_report.svg")
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

    show_print_style_dialog(ctx, &mut state.style_dialog_state, &mut state.style_settings);

    if should_close {
        is_open = false;
    }
    state.is_open = is_open;
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
            let (tick_interval, tick_dec) = if !settings.auto_ticks && settings.tick_major > 1e-4 {
                let s = settings.tick_major;
                let d = if s < 0.0099 { 3 } else if s < 0.099 { 2 } else if s < 0.99 { 1 } else { 0 };
                (s, d)
            } else {
                let s = if span > 300.0 {
                    20.0
                } else if span > 25.0 {
                    10.0
                } else if span > 12.0 {
                    2.0
                } else if span > 4.0 {
                    1.0
                } else if span > 1.5 {
                    0.5
                } else {
                    0.1
                };
                let d = if s < 0.0099 { 3 } else if s < 0.099 { 2 } else if s < 0.99 { 1 } else { 0 };
                (s, d)
            };

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

                        let h_diff = (mv_y_max - mv_y_min).max(1e-6);
                        let y_min_adj = mv_y_min - 0.05 * h_diff;
                        let y_max_adj = mv_y_max + 0.50 * h_diff;

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

                        for t_idx in start_tick..=end_tick {
                            let tick_ppm = t_idx as f64 * step;
                            let tick_x = mv_ppm_to_x(tick_ppm);
                            if tick_x >= inset_rect.min.x + 8.0 && tick_x <= inset_rect.max.x - 8.0 {
                                painter.line_segment(
                                    [Pos2::new(tick_x, inset_axis_y), Pos2::new(tick_x, inset_axis_y + 2.5)],
                                    Stroke::new(0.7_f32, Color32::BLACK),
                                );
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

            for (i, jc) in j_couplings.iter().take(5).enumerate() {
                let s = format!("#{}: {}", i + 1, jc.text);
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

            for (i, jc) in j_couplings.iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{tx1}" y="{ty}" font-size="8" font-family="sans-serif" fill="#212529">#{idx} {text}</text>
"##,
                    tx1 = side_x + 4.0,
                    ty = side_y + 10.0,
                    idx = i + 1,
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

    // 1. スペクトル曲線
    if settings.spectrum && !ppm.is_empty() && !spectrum.is_empty() {
        let n_pts = ppm.len().min(spectrum.len());
        let mut path_data = String::new();
        let target_res = (plot_w * 2.0) as usize;
        let step = (n_pts / target_res).max(1);

        let mut first = true;
        for i in (0..n_pts).step_by(step) {
            let p = ppm[i];
            if p >= p_low && p <= p_high {
                let sx = ppm_to_x(p);
                let sy = y_to_y(spectrum[i]).min(axis_y);
                if first {
                    path_data.push_str(&format!("M {:.2} {:.2} ", sx, sy));
                    first = false;
                } else {
                    path_data.push_str(&format!("L {:.2} {:.2} ", sx, sy));
                }
            }
        }
        svg.push_str(&format!(
            r##"<path d="{}" stroke="{}" stroke-width="{:.2}" fill="none" />
"##,
            path_data,
            style.main_spectrum.line_color.to_hex(),
            style.main_spectrum.line_width,
        ));
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
    let (tick_interval, tick_dec) = if !settings.auto_ticks && settings.tick_major > 1e-4 {
        let s = settings.tick_major;
        let d = if s < 0.0099 { 3 } else if s < 0.099 { 2 } else if s < 0.99 { 1 } else { 0 };
        (s, d)
    } else {
        let s = if span > 300.0 {
            20.0
        } else if span > 25.0 {
            10.0
        } else if span > 12.0 {
            2.0
        } else if span > 4.0 {
            1.0
        } else if span > 1.5 {
            0.5
        } else {
            0.1
        };
        let d = if s < 0.0099 { 3 } else if s < 0.099 { 2 } else if s < 0.99 { 1 } else { 0 };
        (s, d)
    };

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

                let h_diff = (mv_y_max - mv_y_min).max(1e-6);
                let y_min_adj = mv_y_min - 0.05 * h_diff;
                let y_max_adj = mv_y_max + 0.50 * h_diff;

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

                for t_idx in start_tick..=end_tick {
                    let tick_ppm = t_idx as f64 * step;
                    let tick_x = mv_ppm_to_x(tick_ppm);
                    if tick_x >= inset_x + 10.0 && tick_x <= inset_x + inset_w - 10.0 {
                        svg.push_str(&format!(
                            r##"<line x1="{tx:.1}" y1="{y1:.1}" x2="{tx:.1}" y2="{y2:.1}" stroke="#212529" stroke-width="0.7" />
<text x="{tx:.1}" y="{ty:.1}" font-size="7" text-anchor="middle" font-family="sans-serif" fill="#212529">{val:.prec$}</text>
"##,
                            tx = tick_x,
                            y1 = inset_axis_y,
                            y2 = inset_axis_y + 3.0,
                            ty = inset_axis_y + 11.0,
                            val = tick_ppm,
                            prec = decimals,
                        ));
                    }
                }
            }
        }
    }

    svg
}

/// OSネイティブ直接印刷の実行 (ブラウザは一切起動せず、OS印刷スプーラーへ直接ジョブ送信)
fn execute_native_print(
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
) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        print_windows_native(
            settings,
            style,
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

    #[cfg(target_os = "macos")]
    {
        print_macos_native(
            settings,
            style,
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

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Printing is not supported on this OS"))
    }
}

// ----------------------------------------------------------------------------
// Windows GDI ネイティブ直接印刷エンジン (winspool / gdi32)
// ----------------------------------------------------------------------------
#[cfg(target_os = "windows")]
fn print_windows_native(
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

    #[repr(C)]
    #[allow(non_snake_case)]
    struct TEXTMETRICW {
        tmHeight: i32,
        tmAscent: i32,
        tmDescent: i32,
        tmInternalLeading: i32,
        tmExternalLeading: i32,
        tmAveCharWidth: i32,
        tmMaxCharWidth: i32,
        tmWeight: i32,
        tmOverhang: i32,
        tmDigitizedAspectX: i32,
        tmDigitizedAspectY: i32,
        tmFirstChar: u16,
        tmLastChar: u16,
        tmDefaultChar: u16,
        tmBreakChar: u16,
        tmItalic: u8,
        tmUnderlined: u8,
        tmStruckOut: u8,
        tmPitchAndFamily: u8,
        tmCharSet: u8,
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
        fn CreateSolidBrush(color: u32) -> isize;
        fn SelectObject(hdc: isize, hgdiobj: isize) -> isize;
        fn DeleteObject(ho: isize) -> i32;
        fn MoveToEx(hdc: isize, x: i32, y: i32, lppt: *mut POINT) -> i32;
        fn LineTo(hdc: isize, x: i32, y: i32) -> i32;
        fn Polyline(hdc: isize, apt: *const POINT, cpt: i32) -> i32;
        fn Rectangle(hdc: isize, left: i32, top: i32, right: i32, bottom: i32) -> i32;
        fn SetTextColor(hdc: isize, color: u32) -> u32;
        fn SetBkMode(hdc: isize, mode: i32) -> i32;
        fn SetTextAlign(hdc: isize, fMode: u32) -> u32;
        fn CreateFontIndirectW(lplf: *const LOGFONTW) -> isize;
        fn TextOutW(hdc: isize, x: i32, y: i32, lpString: *const u16, c: i32) -> i32;
        fn GetTextMetricsW(hdc: isize, lptm: *mut TEXTMETRICW) -> i32;
    }

    const HORZRES: i32 = 8;
    const VERTRES: i32 = 10;
    const LOGPIXELSX: i32 = 88;
    const LOGPIXELSY: i32 = 90;
    const PS_SOLID: i32 = 0;
    const PS_DASH: i32 = 1;
    const TRANSPARENT: i32 = 1;
    const TA_TOP: u32 = 0;
    const TA_LEFT: u32 = 0;
    const TA_CENTER: u32 = 6;
    const FW_NORMAL: i32 = 400;
    const FW_BOLD: i32 = 700;

    fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let driver_name = to_wide("WINSPOOL");
    let printer_name_wide = if !settings.printer_name.is_empty() && settings.printer_name != "Default Printer" {
        to_wide(&settings.printer_name)
    } else {
        // デフォルトプリンター名の取得
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

    let doc_name = to_wide("Resona NMR Report");
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
            return Err(std::io::Error::last_os_error());
        }
        if StartPage(hdc) <= 0 {
            EndDoc(hdc);
            DeleteDC(hdc);
            return Err(std::io::Error::last_os_error());
        }

        SetBkMode(hdc, TRANSPARENT);
    }

    // フォント作成ヘルパー
    let make_font = |pt_size: f64, bold: bool, rotation_deg: i32| -> isize {
        let height = -((pt_size * (dpi_y as f64) / 72.0).round() as i32);
        let mut lf = LOGFONTW {
            lfHeight: height,
            lfWidth: 0,
            lfEscapement: rotation_deg * 10,
            lfOrientation: rotation_deg * 10,
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
        let font_name = to_wide("Segoe UI");
        for (i, &c) in font_name.iter().take(31).enumerate() {
            lf.lfFaceName[i] = c;
        }
        unsafe { CreateFontIndirectW(&lf) }
    };

    let margin_x = ((0.25 * (dpi_x as f64)).round() as i32).max(25);
    let margin_y = ((0.25 * (dpi_y as f64)).round() as i32).max(25);
    let mut cur_y = margin_y;

    // 1. ヘッダー (ファイル名: 高さを固定して File Name の有無でプロットが動かないようにする)
    let header_h = dpi_y * 22 / 72;
    if settings.filename {
        let title_font = make_font(9.0, true, 0);
        let old_font = unsafe { SelectObject(hdc, title_font) };
        unsafe { SetTextColor(hdc, rgb(73, 80, 87)) };

        let name_str = current_filepath
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled Spectrum".to_string());
        let wide_str = to_wide(&name_str);
        unsafe {
            TextOutW(hdc, margin_x, cur_y, wide_str.as_ptr(), (wide_str.len() - 1) as i32);
        }

        let line_pen = unsafe { CreatePen(PS_SOLID, 1, rgb(222, 226, 230)) };
        let old_pen = unsafe { SelectObject(hdc, line_pen) };
        unsafe {
            MoveToEx(hdc, margin_x, cur_y + (dpi_y * 14 / 72), std::ptr::null_mut());
            LineTo(hdc, dev_w - margin_x, cur_y + (dpi_y * 14 / 72));
            SelectObject(hdc, old_pen);
            DeleteObject(line_pen);
            SelectObject(hdc, old_font);
            DeleteObject(title_font);
        }
    }
    cur_y += header_h;

    let has_side = settings.info || (settings.jcoupling && !j_couplings.is_empty());
    let side_w = if has_side { dpi_x * 125 / 72 } else { 0 };
    let side_gap = if has_side { dpi_x * 8 / 72 } else { 0 };
    let plot_w = dev_w - margin_x * 2 - side_w - side_gap;
    let plot_h = dev_h - cur_y - margin_y;

    // 2. プロット描画 (ベクターPolyline / LineTo)
    if let (Some(ppm_arr), Some(spec_arr)) = (ppm, spectrum) {
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
            let p_min_data = ppm_arr.iter().cloned().fold(f64::INFINITY, f64::min);
            let p_max_data = ppm_arr.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let y_max_data = spec_arr.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(1.0);
            (p_min_data, p_max_data, -0.05 * y_max_data, y_max_data * 1.1, plot_w as f64, plot_h as f64, 0.0, 0.0)
        };

        let bottom_margin = dpi_y * 58 / 72;
        let actual_plot_h = plot_h - bottom_margin;
        let axis_y = cur_y + actual_plot_h;

        let ppm_to_x = |p: f64| -> i32 {
            margin_x + ((p_max - p) / (p_max - p_min).max(1e-6) * (plot_w as f64)).round() as i32
        };
        let y_to_y = |y: f64| -> i32 {
            axis_y - ((y - y_min) / (y_max - y_min).max(1e-6) * (actual_plot_h as f64)).round() as i32
        };

        let p_low = p_min.min(p_max);
        let p_high = p_min.max(p_max);

        // スペクトル曲線
        if settings.spectrum && !ppm_arr.is_empty() && !spec_arr.is_empty() {
            let spec_pen = unsafe {
                CreatePen(
                    PS_SOLID,
                    ((dpi_y as f64 * style.main_spectrum.line_width as f64 / 72.0).round() as i32).max(1),
                    rgb(style.main_spectrum.line_color.r, style.main_spectrum.line_color.g, style.main_spectrum.line_color.b),
                )
            };
            let old_pen = unsafe { SelectObject(hdc, spec_pen) };

            let mut pts: Vec<POINT> = Vec::new();
            for idx in 0..ppm_arr.len().min(spec_arr.len()) {
                let p = ppm_arr[idx];
                if p >= p_low && p <= p_high {
                    let sx = ppm_to_x(p);
                    let sy = y_to_y(spec_arr[idx]).min(axis_y);
                    pts.push(POINT { x: sx, y: sy });
                }
            }

            if pts.len() > 1 {
                unsafe {
                    Polyline(hdc, pts.as_ptr(), pts.len() as i32);
                }
            }

            unsafe {
                SelectObject(hdc, old_pen);
                DeleteObject(spec_pen);
            }
        }

        // X軸
        let axis_pen = unsafe { CreatePen(PS_SOLID, 1, rgb(33, 37, 41)) };
        let old_pen = unsafe { SelectObject(hdc, axis_pen) };
        unsafe {
            MoveToEx(hdc, margin_x, axis_y, std::ptr::null_mut());
            LineTo(hdc, margin_x + plot_w, axis_y);
        }

        // 目盛り & PPM ラベル
        let tick_font = make_font(8.0, false, 0);
        let old_font = unsafe { SelectObject(hdc, tick_font) };
        unsafe { SetTextColor(hdc, rgb(33, 37, 41)) };

        let span = (p_max - p_min).abs();
        let (tick_interval, tick_dec) = if !settings.auto_ticks && settings.tick_major > 1e-4 {
            let s = settings.tick_major;
            let d = if s < 0.0099 { 3 } else if s < 0.099 { 2 } else if s < 0.99 { 1 } else { 0 };
            (s, d)
        } else {
            let s = if span > 300.0 {
                20.0
            } else if span > 25.0 {
                10.0
            } else if span > 12.0 {
                2.0
            } else if span > 4.0 {
                1.0
            } else if span > 1.5 {
                0.5
            } else {
                0.1
            };
            let d = if s < 0.0099 { 3 } else if s < 0.099 { 2 } else if s < 0.99 { 1 } else { 0 };
            (s, d)
        };

        let minor_n = settings.tick_minor.max(1);
        let minor_step = tick_interval / (minor_n as f64);

        let first_tick = (p_low / tick_interval).ceil() * tick_interval;
        let mut cur_tick = first_tick;
        while cur_tick <= p_high {
            let tx = ppm_to_x(cur_tick);
            if tx >= margin_x && tx <= margin_x + plot_w {
                unsafe {
                    // 主目盛り線 (4pt)
                    MoveToEx(hdc, tx, axis_y, std::ptr::null_mut());
                    LineTo(hdc, tx, axis_y + (dpi_y * 4 / 72));
                    let val_str = to_wide(&format!("{:.1$}", cur_tick, tick_dec));
                    TextOutW(hdc, tx - (dpi_x * 8 / 72), axis_y + (dpi_y * 6 / 72), val_str.as_ptr(), (val_str.len() - 1) as i32);
                }
            }

            // サブ目盛り (N分割、2pt、数字なし)
            if minor_n > 1 {
                for m in 1..minor_n {
                    let sub_tick = cur_tick + (m as f64) * minor_step;
                    if sub_tick <= p_high {
                        let stx = ppm_to_x(sub_tick);
                        if stx >= margin_x && stx <= margin_x + plot_w {
                            unsafe {
                                MoveToEx(hdc, stx, axis_y, std::ptr::null_mut());
                                LineTo(hdc, stx, axis_y + (dpi_y * 2 / 72));
                            }
                        }
                    }
                }
            }

            cur_tick += tick_interval;
        }

        unsafe {
            SelectObject(hdc, old_pen);
            DeleteObject(axis_pen);
            SelectObject(hdc, old_font);
            DeleteObject(tick_font);
        }

        // ピーク (引き出し線 & 縦向き PPM 値)
        if settings.peak {
            let mut visible_peaks: Vec<&PeakItem> = peaks
                .iter()
                .filter(|p| p.ppm >= p_low && p.ppm <= p_high)
                .collect();
            visible_peaks.sort_by(|a, b| b.ppm.partial_cmp(&a.ppm).unwrap_or(std::cmp::Ordering::Equal));

            if !visible_peaks.is_empty() {
                let mut text_x: Vec<i32> = visible_peaks.iter().map(|p| ppm_to_x(p.ppm)).collect();
                let min_gap = dpi_x * 8 / 72;
                for _ in 0..200 {
                    let mut moved = false;
                    for i in 1..text_x.len() {
                        let diff = text_x[i] - text_x[i - 1];
                        if diff < min_gap {
                            let overlap = min_gap - diff;
                            text_x[i - 1] -= overlap / 2;
                            text_x[i] += overlap / 2;
                            moved = true;
                        }
                    }
                    if !moved { break; }
                }

                let y_elbow = axis_y + (dpi_y * 15 / 72);
                let y_text_start = axis_y + (dpi_y * 28 / 72);

                let lead_width = ((dpi_y as f64 * style.main_spectrum.peak_lead_width as f64 / 72.0).round() as i32).max(1);
                let lead_pen = unsafe {
                    CreatePen(
                        PS_SOLID,
                        lead_width,
                        rgb(style.main_spectrum.peak_lead_color.r, style.main_spectrum.peak_lead_color.g, style.main_spectrum.peak_lead_color.b),
                    )
                };
                let rot_font = make_font(6.5, false, 270);
                let old_font = unsafe { SelectObject(hdc, rot_font) };
                let mut tm: TEXTMETRICW = unsafe { std::mem::zeroed() };
                unsafe {
                    GetTextMetricsW(hdc, &mut tm);
                    SetTextColor(hdc, rgb(0, 0, 0));
                }

                let old_lead = unsafe { SelectObject(hdc, lead_pen) };
                for (i, pk) in visible_peaks.iter().enumerate() {
                    let px = ppm_to_x(pk.ppm);
                    let tx = text_x[i];

                    // 枠外にはみ出るものはスキップ (omit)
                    if tx < margin_x + 2 || tx > margin_x + plot_w - 2 || px < margin_x || px > margin_x + plot_w {
                        continue;
                    }

                    let pk_sy = y_to_y(pk.intensity);
                    let y_start = (axis_y - (dpi_y * 8 / 72)).max(pk_sy + (dpi_y * 2 / 72));

                    unsafe {
                        MoveToEx(hdc, px, y_start, std::ptr::null_mut());
                        LineTo(hdc, px, y_elbow);
                        LineTo(hdc, tx, y_text_start - (dpi_y * 3 / 72));
                        LineTo(hdc, tx, y_text_start);
                    }

                    let val_str = to_wide(&format!("{:.1$}", pk.ppm, settings.ppm_decimals));
                    let draw_x = tx + tm.tmHeight / 2;
                    let draw_y = y_text_start + (dpi_y * 2 / 72);
                    unsafe {
                        TextOutW(hdc, draw_x, draw_y, val_str.as_ptr(), (val_str.len() - 1) as i32);
                    }
                }

                unsafe {
                    SelectObject(hdc, old_lead);
                    SelectObject(hdc, old_font);
                    DeleteObject(rot_font);
                    DeleteObject(lead_pen);
                }
            }
        }

        // 積分 (本物の累積積分曲線 & 局所ベースライン)
        if settings.integrate {
            let intg_pen = unsafe {
                CreatePen(
                    PS_SOLID,
                    ((dpi_y as f64 * style.main_spectrum.integral_width as f64 / 72.0).round() as i32).max(1),
                    rgb(style.main_spectrum.integral_color.r, style.main_spectrum.integral_color.g, style.main_spectrum.integral_color.b),
                )
            };
            let bl_pen = unsafe { CreatePen(PS_DASH, 1, rgb(59, 130, 246)) };
            let intg_font = make_font(6.5, false, 270);
            let old_font = unsafe { SelectObject(hdc, intg_font) };
            let mut intg_tm: TEXTMETRICW = unsafe { std::mem::zeroed() };
            unsafe {
                GetTextMetricsW(hdc, &mut intg_tm);
                SetTextColor(hdc, rgb(style.main_spectrum.integral_color.r, style.main_spectrum.integral_color.g, style.main_spectrum.integral_color.b));
            }

            for it in integrations {
                let p_start = it.start_ppm.max(it.end_ppm);
                let p_end = it.start_ppm.min(it.end_ppm);
                let sx_start = ppm_to_x(p_start);
                let sx_end = ppm_to_x(p_end);

                if sx_end > margin_x && sx_start < margin_x + plot_w {
                    let old_p = unsafe { SelectObject(hdc, bl_pen) };
                    let bl_y1 = y_to_y(it.y_start);
                    let bl_y2 = y_to_y(it.y_end);
                    unsafe {
                        MoveToEx(hdc, sx_start, bl_y1, std::ptr::null_mut());
                        LineTo(hdc, sx_end, bl_y2);
                        SelectObject(hdc, intg_pen);
                    }

                    if let Some(res) = compute_integral(spec_arr, ppm_arr, it, integration_scale, integration_ref_factor, integration_offset) {
                        if res.ppm.len() > 1 && res.ppm.len() == res.curve_y.len() {
                            let mut pts = Vec::with_capacity(res.ppm.len());
                            let mut min_sy = i32::MAX;
                            for idx in 0..res.ppm.len() {
                                let sx = ppm_to_x(res.ppm[idx]);
                                let sy = y_to_y(res.curve_y[idx]).min(axis_y);
                                pts.push(POINT { x: sx, y: sy });
                                min_sy = min_sy.min(sy);
                            }
                            unsafe {
                                Polyline(hdc, pts.as_ptr(), pts.len() as i32);
                            }

                            let mid_x = (sx_start + sx_end) / 2;
                            let val_str = to_wide(&format!("{:.1$}", res.normalized_value, settings.integral_decimals));
                            let text_len = (dpi_y as f64 * (val_str.len() as f64) * 4.2 / 72.0).round() as i32;
                            let draw_x = mid_x + intg_tm.tmHeight / 2;
                            let draw_y = min_sy - text_len - (dpi_y * 3 / 72);
                            unsafe {
                                TextOutW(hdc, draw_x, draw_y, val_str.as_ptr(), (val_str.len() - 1) as i32);
                            }
                        }
                    }

                    unsafe { SelectObject(hdc, old_p) };
                }
            }

            unsafe {
                SelectObject(hdc, old_font);
                DeleteObject(intg_font);
                DeleteObject(intg_pen);
                DeleteObject(bl_pen);
            }
        }

        // マルチビュー (拡大スペクトル、積分、ピーク引き出し線、X軸目盛り)
        if settings.multiview && !multiviews.is_empty() {
            let mv_border_width = ((dpi_y as f64 * 1.1 / 72.0).round() as i32).max(1);
            let mv_pen = unsafe { CreatePen(PS_SOLID, mv_border_width, rgb(136, 136, 136)) };
            let white_brush = unsafe { CreateSolidBrush(rgb(255, 255, 255)) };

            for mv in multiviews {
                let rel_x = (mv.geometry.x as f64 - main_screen_min_x) / main_screen_w.max(1.0);
                let rel_y = (mv.geometry.y as f64 - main_screen_min_y) / main_screen_h.max(1.0);
                let rel_w = (mv.geometry.w as f64) / main_screen_w.max(1.0);
                let rel_h = (mv.geometry.h as f64) / main_screen_h.max(1.0);

                let inset_x = margin_x + (rel_x * (plot_w as f64)).round() as i32;
                let inset_y = cur_y + (rel_y * (plot_h as f64)).round() as i32;
                let inset_w = (rel_w * (plot_w as f64)).round() as i32;
                let inset_h = (rel_h * (plot_h as f64)).round() as i32;

                if inset_w > 30 && inset_h > 30 {
                    let old_p = unsafe { SelectObject(hdc, mv_pen) };
                    let old_b = unsafe { SelectObject(hdc, white_brush) };
                    unsafe {
                        Rectangle(hdc, inset_x, inset_y, inset_x + inset_w, inset_y + inset_h);
                        SelectObject(hdc, old_b);
                    }

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

                    let h_diff = (mv_y_max - mv_y_min).max(1e-6);
                    let y_min_adj = mv_y_min - 0.05 * h_diff;
                    let y_max_adj = mv_y_max + 0.50 * h_diff;

                    let inset_axis_y = inset_y + inset_h - (dpi_y * 14 / 72);
                    let inset_plot_h = (inset_axis_y - inset_y - (dpi_y * 6 / 72)).max(10);
                    let inset_plot_w = (inset_w - (dpi_x * 12 / 72)).max(10);

                    let mv_ppm_to_x = |p: f64| -> i32 {
                        inset_x + (dpi_x * 6 / 72) + (((mv_src_max - p) / (mv_src_max - mv_src_min).max(1e-6)) * (inset_plot_w as f64)).round() as i32
                    };
                    let mv_y_to_y = |y: f64| -> i32 {
                        inset_axis_y - (((y - y_min_adj) / (y_max_adj - y_min_adj).max(1e-6)) * (inset_plot_h as f64)).round() as i32
                    };

                    // 1. 拡大スペクトル曲線
                    let spec_pen = unsafe {
                        CreatePen(
                            PS_SOLID,
                            ((dpi_y as f64 * style.multiview.line_width as f64 / 72.0).round() as i32).max(1),
                            rgb(style.multiview.line_color.r, style.multiview.line_color.g, style.multiview.line_color.b),
                        )
                    };
                    let old_spec_pen = unsafe { SelectObject(hdc, spec_pen) };

                    let mut mv_pts = Vec::new();
                    for idx in 0..ppm_arr.len().min(spec_arr.len()) {
                        let p = ppm_arr[idx];
                        if p >= mv_src_min && p <= mv_src_max {
                            let sx = mv_ppm_to_x(p);
                            let sy = mv_y_to_y(spec_arr[idx]);
                            mv_pts.push(POINT { x: sx, y: sy.min(inset_axis_y) });
                        }
                    }

                    if mv_pts.len() > 1 {
                        unsafe {
                            Polyline(hdc, mv_pts.as_ptr(), mv_pts.len() as i32);
                        }
                    }

                    unsafe {
                        SelectObject(hdc, old_spec_pen);
                        DeleteObject(spec_pen);
                    }

                    // 2. 積分 (マルチビュー内)
                    if settings.integrate {
                        let mv_intg_pen = unsafe {
                            CreatePen(
                                PS_SOLID,
                                ((dpi_y as f64 * style.multiview.integral_width as f64 / 72.0).round() as i32).max(1),
                                rgb(style.multiview.integral_color.r, style.multiview.integral_color.g, style.multiview.integral_color.b),
                            )
                        };
                        let old_intg_p = unsafe { SelectObject(hdc, mv_intg_pen) };
                        let mv_intg_font = make_font(5.5, false, 270);
                        let old_font = unsafe { SelectObject(hdc, mv_intg_font) };
                        let mut mv_intg_tm: TEXTMETRICW = unsafe { std::mem::zeroed() };
                        unsafe {
                            GetTextMetricsW(hdc, &mut mv_intg_tm);
                            SetTextColor(hdc, rgb(style.multiview.integral_color.r, style.multiview.integral_color.g, style.multiview.integral_color.b));
                        }

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
                                            intg_pts.push(POINT { x: mv_ppm_to_x(p), y: mv_y_to_y(target_data_y) });
                                        }
                                    }
                                    if intg_pts.len() > 1 {
                                        unsafe {
                                            Polyline(hdc, intg_pts.as_ptr(), intg_pts.len() as i32);
                                        }
                                        let mid_p = (i_min.max(mv_src_min) + i_max.min(mv_src_max)) * 0.5;
                                        let tx = mv_ppm_to_x(mid_p);
                                        let ty = mv_y_to_y(y_min_adj + 0.70 * h_diff);
                                        let val_str = to_wide(&format!("{:.1$}", res.normalized_value, settings.integral_decimals));
                                        let text_len = (dpi_y as f64 * (val_str.len() as f64) * 3.6 / 72.0).round() as i32;
                                        let draw_x = tx + mv_intg_tm.tmHeight / 2;
                                        let draw_y = ty - text_len;
                                        unsafe {
                                            TextOutW(hdc, draw_x, draw_y, val_str.as_ptr(), (val_str.len() - 1) as i32);
                                        }
                                    }
                                }
                            }
                        }

                        unsafe {
                            SelectObject(hdc, old_intg_p);
                            SelectObject(hdc, old_font);
                            DeleteObject(mv_intg_font);
                            DeleteObject(mv_intg_pen);
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

                            let mut mv_text_x: Vec<i32> = sorted_peaks.iter().map(|p| mv_ppm_to_x(p.ppm)).collect();
                            let min_gap = dpi_x * 6 / 72;
                            for _ in 0..100 {
                                let mut moved = false;
                                for i in 1..mv_text_x.len() {
                                    let diff = mv_text_x[i] - mv_text_x[i - 1];
                                    if diff < min_gap {
                                        let overlap = min_gap - diff;
                                        mv_text_x[i - 1] -= overlap / 2;
                                        mv_text_x[i] += overlap / 2;
                                        moved = true;
                                    }
                                }
                                if !moved { break; }
                            }

                            let lead_width = ((dpi_y as f64 * style.multiview.peak_lead_width as f64 / 72.0).round() as i32).max(1);
                            let lead_pen = unsafe {
                                CreatePen(
                                    PS_SOLID,
                                    lead_width,
                                    rgb(style.multiview.peak_lead_color.r, style.multiview.peak_lead_color.g, style.multiview.peak_lead_color.b),
                                )
                            };
                            let old_lead = unsafe { SelectObject(hdc, lead_pen) };
                            let mv_rot_font = make_font(5.0, false, 270);
                            let old_font = unsafe { SelectObject(hdc, mv_rot_font) };
                            let mut mv_tm: TEXTMETRICW = unsafe { std::mem::zeroed() };
                            unsafe {
                                GetTextMetricsW(hdc, &mut mv_tm);
                                SetTextColor(hdc, rgb(0, 0, 0));
                            }

                            let text_len = dpi_y * 14 / 72;
                            let mv_text_start_y = inset_y + (dpi_y * 3 / 72);
                            let text_bottom_y = mv_text_start_y + text_len;
                            let mv_elbow_y = text_bottom_y + (dpi_y * 3 / 72);
                            let max_lead_y = (inset_y + inset_plot_h * 25 / 100).max(mv_elbow_y + (dpi_y * 2 / 72));

                            for (i, pk) in sorted_peaks.iter().enumerate() {
                                let px = mv_ppm_to_x(pk.ppm);
                                let tx = mv_text_x[i];

                                // 枠外にはみ出るものはスキップ (omit)
                                if tx < inset_x + 3 || tx > inset_x + inset_w - 3 || px < inset_x || px > inset_x + inset_w {
                                    continue;
                                }

                                let py = mv_y_to_y(pk.intensity);
                                let clearance = dpi_y * 6 / 72;
                                let line_start_y = (py - clearance).min(max_lead_y);

                                if line_start_y > mv_elbow_y + 1 {
                                    unsafe {
                                        MoveToEx(hdc, px, line_start_y, std::ptr::null_mut());
                                        LineTo(hdc, px, mv_elbow_y);
                                        LineTo(hdc, tx, text_bottom_y + (dpi_y * 1 / 72));
                                        LineTo(hdc, tx, text_bottom_y);
                                    }
                                }

                                let val_str = to_wide(&format!("{:.1$}", pk.ppm, settings.ppm_decimals));
                                let draw_x = tx + mv_tm.tmHeight / 2;
                                let draw_y = mv_text_start_y;
                                unsafe {
                                    TextOutW(hdc, draw_x, draw_y, val_str.as_ptr(), (val_str.len() - 1) as i32);
                                }
                            }

                            unsafe {
                                SelectObject(hdc, old_lead);
                                SelectObject(hdc, old_font);
                                DeleteObject(mv_rot_font);
                                DeleteObject(lead_pen);
                            }
                        }
                    }

                    // 4. X 軸目盛り線 & PPM 数値ラベル (マルチビュー内)
                    let axis_pen = unsafe { CreatePen(PS_SOLID, 1, rgb(33, 37, 41)) };
                    let old_ax_p = unsafe { SelectObject(hdc, axis_pen) };
                    unsafe {
                        MoveToEx(hdc, inset_x + (dpi_x * 6 / 72), inset_axis_y, std::ptr::null_mut());
                        LineTo(hdc, inset_x + inset_w - (dpi_x * 6 / 72), inset_axis_y);
                    }

                    let ppm_span = mv_src_max - mv_src_min;
                    let target_ticks = ((inset_plot_w as f64) / (dpi_x as f64 * 40.0 / 72.0)).clamp(2.0, 5.0);
                    let rough_step = (ppm_span / target_ticks).max(1e-6);
                    let exponent = rough_step.log10().floor();
                    let frac = rough_step / 10.0_f64.powf(exponent);
                    let nice_frac = if frac <= 1.5 { 1.0 } else if frac <= 3.0 { 2.0 } else if frac <= 7.0 { 5.0 } else { 10.0 };
                    let step = nice_frac * 10.0_f64.powf(exponent);
                    let start_tick = (mv_src_min / step).ceil() as i64;
                    let end_tick = (mv_src_max / step).floor() as i64;
                    let decimals = if step < 0.0099 { 3 } else if step < 0.099 { 2 } else if step < 0.99 { 1 } else { 0 };

                    let tick_font = make_font(6.0, false, 0);
                    let old_tick_f = unsafe { SelectObject(hdc, tick_font) };
                    unsafe {
                        SetTextAlign(hdc, TA_CENTER | TA_TOP);
                        SetTextColor(hdc, rgb(33, 37, 41));
                    }

                    for t_idx in start_tick..=end_tick {
                        let tick_ppm = t_idx as f64 * step;
                        let tick_x = mv_ppm_to_x(tick_ppm);
                        if tick_x >= inset_x + (dpi_x * 8 / 72) && tick_x <= inset_x + inset_w - (dpi_x * 8 / 72) {
                            unsafe {
                                MoveToEx(hdc, tick_x, inset_axis_y, std::ptr::null_mut());
                                LineTo(hdc, tick_x, inset_axis_y + (dpi_y * 2 / 72));
                            }
                            let s = to_wide(&format!("{:.1$}", tick_ppm, decimals));
                            unsafe {
                                TextOutW(hdc, tick_x, inset_axis_y + (dpi_y * 3 / 72), s.as_ptr(), (s.len() - 1) as i32);
                            }
                        }
                    }

                    unsafe {
                        SelectObject(hdc, old_tick_f);
                        SelectObject(hdc, old_ax_p);
                        SetTextAlign(hdc, TA_LEFT | TA_TOP);
                        DeleteObject(tick_font);
                        DeleteObject(axis_pen);
                    }

                    unsafe { SelectObject(hdc, old_p); }
                }
            }

            unsafe {
                DeleteObject(mv_pen);
                DeleteObject(white_brush);
            }
        }
    }

    // 3. 右側パラメータ表 (GDI: 画面のサイドパネルと100%同一の全行を表示)
    if has_side {
        let side_x = dev_w - margin_x - side_w;
        let mut text_y = cur_y;

        let hdr_font = make_font(8.0, true, 0);
        let cell_font = make_font(6.5, false, 0);

        if settings.info {
            let _old_font = unsafe { SelectObject(hdc, hdr_font) };
            unsafe { SetTextColor(hdc, rgb(33, 37, 41)) };
            let title_wide = to_wide("Experimental Parameters");
            unsafe {
                TextOutW(hdc, side_x, text_y, title_wide.as_ptr(), (title_wide.len() - 1) as i32);
                SelectObject(hdc, cell_font);
            }
            text_y += dpi_y * 12 / 72;

            let exp_rows = metadata.to_display_rows();
            for (k, v) in exp_rows {
                let row_wide = to_wide(&format!("{}: {}", k, v));
                unsafe {
                    TextOutW(hdc, side_x, text_y, row_wide.as_ptr(), (row_wide.len() - 1) as i32);
                }
                text_y += dpi_y * 9 / 72;
                if text_y > dev_h - margin_y - (dpi_y * 60 / 72) { break; }
            }

            text_y += dpi_y * 6 / 72;
            unsafe { SelectObject(hdc, hdr_font) };
            let ft_hdr = to_wide("FT Settings");
            unsafe {
                TextOutW(hdc, side_x, text_y, ft_hdr.as_ptr(), (ft_hdr.len() - 1) as i32);
                SelectObject(hdc, cell_font);
            }
            text_y += dpi_y * 12 / 72;

            let eff_points = (metadata.points * ft_settings.zf_factor).max(1);
            let dig_res = format!("{:.4} Hz/pt", metadata.spectral_width_hz / (eff_points as f64));
            let ft_rows = [
                format!("Window: {}", match ft_settings.window {
                    crate::core::WindowFunction::None => "None".to_string(),
                    crate::core::WindowFunction::Exponential { lb } => format!("Exp ({}Hz)", lb),
                    crate::core::WindowFunction::Gaussian { g1, g2, g3 } => format!("G({},{},{})", g1, g2, g3),
                }),
                format!("Zero Fill: {}x", ft_settings.zf_factor),
                format!("Res: {}", dig_res),
                format!("Group Delay: {}", if ft_settings.remove_digital_filter { "Removed" } else { "Kept" }),
            ];

            for r in ft_rows {
                let row_wide = to_wide(&r);
                unsafe {
                    TextOutW(hdc, side_x, text_y, row_wide.as_ptr(), (row_wide.len() - 1) as i32);
                }
                text_y += dpi_y * 9 / 72;
            }
        }

        if settings.jcoupling && !j_couplings.is_empty() && text_y < dev_h - margin_y - (dpi_y * 20 / 72) {
            text_y += dpi_y * 6 / 72;
            unsafe { SelectObject(hdc, hdr_font) };
            let jc_hdr = to_wide("J Coupling");
            unsafe {
                TextOutW(hdc, side_x, text_y, jc_hdr.as_ptr(), (jc_hdr.len() - 1) as i32);
                SelectObject(hdc, cell_font);
            }
            text_y += dpi_y * 12 / 72;

            for (i, jc) in j_couplings.iter().take(6).enumerate() {
                let s = to_wide(&format!("#{}: {}", i + 1, jc.text));
                unsafe {
                    TextOutW(hdc, side_x, text_y, s.as_ptr(), (s.len() - 1) as i32);
                }
                text_y += dpi_y * 9 / 72;
            }
        }

        unsafe {
            DeleteObject(hdr_font);
            DeleteObject(cell_font);
        }
    }

    unsafe {
        EndPage(hdc);
        EndDoc(hdc);
        DeleteDC(hdc);
    }

    Ok(())
}

// ----------------------------------------------------------------------------
// macOS CUPS ネイティブ直接印刷エンジン
// ----------------------------------------------------------------------------
#[cfg(target_os = "macos")]
fn print_macos_native(
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
) -> std::io::Result<()> {
    let temp_dir = std::env::temp_dir();
    let svg_path = temp_dir.join("resona_print_job.svg");

    let svg_content = generate_complete_page_svg_with_style(
        settings,
        style,
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

    fs::write(&svg_path, svg_content)?;

    let mut cmd = std::process::Command::new("lp");
    if !settings.printer_name.is_empty() && settings.printer_name != "Default Printer" {
        cmd.args(["-d", &settings.printer_name]);
    }
    cmd.args(["-o", "fit-to-page", &svg_path.to_string_lossy()]);
    let status = cmd.status()?;
    if !status.success() {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "lp command failed"));
    }
    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
