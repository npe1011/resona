#![cfg(target_os = "windows")]

use std::path::Path;
use ndarray::Array1;

use crate::core::{AcquisitionMetadata, FtSettings, IntegrationItem, JCouplingResultItem, MultiviewItem, PeakItem};
use crate::core::analysis::integral::compute_integral;
use crate::core::signal::scale::calc_ppm_ticks;
use crate::gui::plot::transform::PlotTransform;
use crate::gui::dialogs::print_style_dialog::PrintStyleSettings;
use crate::gui::dialogs::print_dialog::{PrintOrientation, PrintSettings};

#[repr(C)]
#[allow(non_snake_case)]
pub struct DEVMODEW {
    pub dmDeviceName: [u16; 32],
    pub dmSpecVersion: u16,
    pub dmDriverVersion: u16,
    pub dmSize: u16,
    pub dmDriverExtra: u16,
    pub dmFields: u32,
    pub dmOrientation: i16,
    pub dmPaperSize: i16,
    pub dmPaperLength: i16,
    pub dmPaperWidth: i16,
    pub dmScale: i16,
    pub dmCopies: i16,
    pub dmDefaultSource: i16,
    pub dmPrintQuality: i16,
    pub dmColor: i16,
    pub dmDuplex: i16,
    pub dmYResolution: i16,
    pub dmTTOption: i16,
    pub dmCollate: i16,
    pub dmFormName: [u16; 32],
    pub dmLogPixels: u16,
    pub dmBitsPerPel: u32,
    pub dmPelsWidth: u32,
    pub dmPelsHeight: u32,
    pub dmDisplayFlags: u32,
    pub dmDisplayFrequency: u32,
    pub dmICMMethod: u32,
    pub dmICMIntent: u32,
    pub dmMediaType: u32,
    pub dmDitherType: u32,
    pub dmReserved1: u32,
    pub dmReserved2: u32,
    pub dmPanningWidth: u32,
    pub dmPanningHeight: u32,
}

pub const DM_ORIENTATION: u32 = 0x00000001;
pub const DM_COLOR: u32 = 0x00000800;
pub const DMORIENT_PORTRAIT: i16 = 1;
pub const DMORIENT_LANDSCAPE: i16 = 2;
pub const DMCOLOR_MONOCHROME: i16 = 1;
pub const DMCOLOR_COLOR: i16 = 2;
pub const DM_IN_BUFFER: u32 = 8;
pub const DM_IN_PROMPT: u32 = 4;
pub const DM_OUT_BUFFER: u32 = 2;

#[link(name = "winspool")]
unsafe extern "system" {
    pub fn OpenPrinterW(pPrinterName: *const u16, phPrinter: *mut isize, pDefault: *const std::ffi::c_void) -> i32;
    pub fn ClosePrinter(hPrinter: isize) -> i32;
    pub fn DocumentPropertiesW(
        hWnd: isize,
        hPrinter: isize,
        pDeviceName: *const u16,
        pDevModeOutput: *mut std::ffi::c_void,
        pDevModeInput: *mut std::ffi::c_void,
        fMode: u32,
    ) -> i32;
}

/// プリンターの DEVMODEW を取得または構築し、カラー設定・用紙向きを反映する
pub fn get_or_create_devmode(
    printer_name: &str,
    orientation: PrintOrientation,
    existing_devmode: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let wide_name: Vec<u16> = printer_name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut h_printer: isize = 0;
    unsafe {
        if OpenPrinterW(wide_name.as_ptr(), &mut h_printer, std::ptr::null()) == 0 {
            return None;
        }

        let needed = DocumentPropertiesW(0, h_printer, wide_name.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
        if needed <= 0 {
            ClosePrinter(h_printer);
            return None;
        }

        let mut buf = if let Some(existing) = existing_devmode {
            if existing.len() >= needed as usize {
                existing.to_vec()
            } else {
                vec![0u8; needed as usize]
            }
        } else {
            vec![0u8; needed as usize]
        };

        if existing_devmode.is_none() {
            let ret = DocumentPropertiesW(
                0,
                h_printer,
                wide_name.as_ptr(),
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                std::ptr::null_mut(),
                DM_OUT_BUFFER,
            );
            if ret < 0 {
                ClosePrinter(h_printer);
                return None;
            }
        }

        if buf.len() >= std::mem::size_of::<DEVMODEW>() {
            let dm = buf.as_mut_ptr() as *mut DEVMODEW;
            (*dm).dmFields |= DM_COLOR | DM_ORIENTATION;
            (*dm).dmColor = DMCOLOR_COLOR;
            (*dm).dmOrientation = match orientation {
                PrintOrientation::Landscape => DMORIENT_LANDSCAPE,
                PrintOrientation::Portrait => DMORIENT_PORTRAIT,
            };

            DocumentPropertiesW(
                0,
                h_printer,
                wide_name.as_ptr(),
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                buf.as_ptr() as *mut std::ffi::c_void,
                DM_IN_BUFFER | DM_OUT_BUFFER,
            );
        }

        ClosePrinter(h_printer);
        Some(buf)
    }
}

// ----------------------------------------------------------------------------
// Windows GDI ネイティブ直接印刷エンジン (winspool / gdi32)
// ----------------------------------------------------------------------------
pub fn print_windows_native(
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
    devmode_opt: Option<&[u8]>,
) -> std::io::Result<Option<String>> {
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

    let devmode_buf = get_or_create_devmode(&settings.printer_name, settings.orientation, devmode_opt);
    let p_devmode = devmode_buf.as_ref().map(|b| b.as_ptr() as *const std::ffi::c_void).unwrap_or(std::ptr::null());

    let hdc = unsafe {
        CreateDCW(
            driver_name.as_ptr(),
            printer_name_wide.as_ptr(),
            std::ptr::null(),
            p_devmode,
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
        let (tick_interval, tick_dec) = calc_ppm_ticks(span, settings.auto_ticks, settings.tick_major);

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

                    let y_min_val = mv.src_y_min.unwrap_or_else(|| mv_y_min.min(0.0));
                    let y_max_val = mv.src_y_max.unwrap_or(mv_y_max);
                    let h_diff = (y_max_val - y_min_val).max(1e-6);
                    let y_min_adj = y_min_val - 0.02 * h_diff;
                    let y_max_adj = y_max_val + 0.40 * h_diff;

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

                    let minor_step = step / 10.0;
                    let start_minor = (mv_src_min / minor_step).ceil() as i64;
                    let end_minor = (mv_src_max / minor_step).floor() as i64;

                    let minor_pen = unsafe { CreatePen(PS_SOLID, 1, rgb(108, 117, 125)) };
                    let old_minor_p = unsafe { SelectObject(hdc, minor_pen) };

                    // サブ目盛り (10分割、短め、ラベルなし)
                    for m_idx in start_minor..=end_minor {
                        if m_idx % 10 == 0 { continue; }
                        let m_ppm = m_idx as f64 * minor_step;
                        let m_x = mv_ppm_to_x(m_ppm);
                        if m_x >= inset_x + (dpi_x * 6 / 72) && m_x <= inset_x + inset_w - (dpi_x * 6 / 72) {
                            unsafe {
                                MoveToEx(hdc, m_x, inset_axis_y, std::ptr::null_mut());
                                LineTo(hdc, m_x, inset_axis_y + (dpi_y * 1 / 72).max(1));
                            }
                        }
                    }
                    unsafe {
                        SelectObject(hdc, old_minor_p);
                        DeleteObject(minor_pen);
                    }

                    // メイン目盛り
                    for t_idx in start_tick..=end_tick {
                        let tick_ppm = t_idx as f64 * step;
                        let tick_x = mv_ppm_to_x(tick_ppm);
                        if tick_x >= inset_x + (dpi_x * 6 / 72) && tick_x <= inset_x + inset_w - (dpi_x * 6 / 72) {
                            unsafe {
                                MoveToEx(hdc, tick_x, inset_axis_y, std::ptr::null_mut());
                                LineTo(hdc, tick_x, inset_axis_y + (dpi_y * 2 / 72).max(2));
                            }
                            if tick_x >= inset_x + (dpi_x * 8 / 72) && tick_x <= inset_x + inset_w - (dpi_x * 8 / 72) {
                                let s = to_wide(&format!("{:.1$}", tick_ppm, decimals));
                                unsafe {
                                    TextOutW(hdc, tick_x, inset_axis_y + (dpi_y * 3 / 72), s.as_ptr(), (s.len() - 1) as i32);
                                }
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

            for jc in j_couplings.iter().take(6) {
                let s = to_wide(&jc.text);
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

    Ok(None)
}
