use std::path::Path;
use std::sync::{Arc, Mutex};
use serde::Deserialize;
use ndarray::Array1;

use crate::core::{AcquisitionMetadata, FtSettings, IntegrationItem, JCouplingResultItem, MultiviewItem, PeakItem};
use crate::gui::plot::transform::PlotTransform;
use crate::gui::dialogs::print_style_dialog::PrintStyleSettings;
use crate::gui::dialogs::print_dialog::{PrintOrientation, PrintSettings};

#[cfg(target_os = "windows")]
pub mod gdi;

#[cfg(target_os = "windows")]
pub use gdi::{DEVMODEW, get_or_create_devmode, print_windows_native};

/// 検出されたシステムプリンター情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPrinter {
    pub name: String,
    pub is_default: bool,
}

/// OSのプリンター詳細設定ダイアログ (Print Preferences) を開き、設定変更を devmode に保存する関数
#[cfg(target_os = "windows")]
pub fn open_printer_preferences(
    printer_name: &str,
    orientation: PrintOrientation,
    devmode_arc: Arc<Mutex<Option<Vec<u8>>>>,
) {
    use gdi::*;

    let wide_name: Vec<u16> = printer_name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut h_printer: isize = 0;
    unsafe {
        if OpenPrinterW(wide_name.as_ptr(), &mut h_printer, std::ptr::null()) == 0 {
            return;
        }

        let needed = DocumentPropertiesW(0, h_printer, wide_name.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
        if needed <= 0 {
            ClosePrinter(h_printer);
            return;
        }

        let existing = devmode_arc.lock().ok().and_then(|g| g.clone());
        let mut buf = if let Some(ext) = existing {
            if ext.len() >= needed as usize {
                ext
            } else {
                vec![0u8; needed as usize]
            }
        } else {
            let mut init_buf = vec![0u8; needed as usize];
            let ret = DocumentPropertiesW(
                0,
                h_printer,
                wide_name.as_ptr(),
                init_buf.as_mut_ptr() as *mut std::ffi::c_void,
                std::ptr::null_mut(),
                DM_OUT_BUFFER,
            );
            if ret >= 0 && init_buf.len() >= std::mem::size_of::<DEVMODEW>() {
                let dm = init_buf.as_mut_ptr() as *mut DEVMODEW;
                (*dm).dmFields |= DM_COLOR | DM_ORIENTATION;
                (*dm).dmColor = DMCOLOR_COLOR;
                (*dm).dmOrientation = match orientation {
                    PrintOrientation::Landscape => DMORIENT_LANDSCAPE,
                    PrintOrientation::Portrait => DMORIENT_PORTRAIT,
                };
            }
            init_buf
        };

        let ret = DocumentPropertiesW(
            0,
            h_printer,
            wide_name.as_ptr(),
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            buf.as_ptr() as *mut std::ffi::c_void,
            DM_IN_BUFFER | DM_IN_PROMPT | DM_OUT_BUFFER,
        );

        if ret == 1 {
            if let Ok(mut g) = devmode_arc.lock() {
                *g = Some(buf);
            }
        }

        ClosePrinter(h_printer);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn open_printer_preferences(
    _printer_name: &str,
    _orientation: PrintOrientation,
    _devmode_arc: Arc<Mutex<Option<Vec<u8>>>>,
) {}

/// OSネイティブにプリンター一覧を検出する関数
pub fn fetch_system_printers() -> Vec<SystemPrinter> {
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

/// OSネイティブ直接印刷の実行 (ブラウザは一切起動せず、OS印刷スプーラーへ直接ジョブ送信)
pub fn execute_native_print(
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
    #[cfg(target_os = "windows")]
    {
        gdi::print_windows_native(
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
            devmode_opt,
        )
    }

    #[cfg(target_os = "macos")]
    {
        let _ = devmode_opt;
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
        let _ = devmode_opt;
        let _ = (settings, style, main_transform, ppm, spectrum, peaks, integrations, integration_scale, integration_offset, integration_ref_factor, multiviews, metadata, ft_settings, j_couplings, current_filepath);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Printing is not supported on this OS"))
    }
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
) -> std::io::Result<Option<String>> {
    use std::fs;
    use crate::gui::export::svg::generate_complete_page_svg_with_style;

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

    // 1. rsvg-convert (librsvg) が環境にあれば高精度 PDF に変換して CUPS 印刷
    let has_rsvg = std::process::Command::new("rsvg-convert")
        .arg("-v")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_rsvg {
        let pdf_path = temp_dir.join("resona_print_job.pdf");
        let convert_ok = std::process::Command::new("rsvg-convert")
            .args(["-f", "pdf", "-o", pdf_path.to_str().unwrap()])
            .arg(&svg_path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if convert_ok {
            let mut cmd = std::process::Command::new("lp");
            if !settings.printer_name.is_empty() && settings.printer_name != "Default Printer" {
                cmd.args(["-d", &settings.printer_name]);
            }
            cmd.args(["-o", "fit-to-page", pdf_path.to_str().unwrap()]);
            let status = cmd.status()?;
            if status.success() {
                return Ok(None);
            }
        }
    }

    // 2. rsvg-convert が無い場合は、macOS 標準の Preview.app で SVG を開いて Cmd+P 印刷を案内
    let open_status = std::process::Command::new("open")
        .args(["-a", "Preview"])
        .arg(&svg_path)
        .status()
        .or_else(|_| std::process::Command::new("open").arg(&svg_path).status())?;

    if open_status.success() {
        Ok(Some("Opened report in Preview for high-quality printing. Please press Cmd+P.".to_string()))
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to open Preview for printing",
        ))
    }
}
