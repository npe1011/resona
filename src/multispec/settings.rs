use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::core::project::DisplaySettings;
use crate::gui::config::{get_settings_dir, get_user_home_dir};
use crate::gui::dialogs::print_dialog::PrintOrientation;

/// MultiSpec 印刷設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSpecPrintSettings {
    #[serde(default)]
    pub printer_name: String,
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_true")]
    pub show_title: bool,
    #[serde(default = "default_true")]
    pub show_spectrum_names: bool,
    #[serde(default = "default_true")]
    pub show_integrals: bool,
    #[serde(default = "default_true")]
    pub show_axis: bool,
    #[serde(default = "default_false")]
    pub show_filepath: bool,
    #[serde(default)]
    pub orientation: PrintOrientation,
    #[serde(default = "default_margin_mm")]
    pub margin_mm: f64,
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

fn default_title() -> String {
    String::new()
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_margin_mm() -> f64 {
    10.0
}
fn default_auto_ticks() -> bool {
    false
}
fn default_tick_major() -> f64 {
    1.0
}
fn default_tick_minor() -> usize {
    10
}

impl Default for MultiSpecPrintSettings {
    fn default() -> Self {
        Self {
            printer_name: String::new(),
            title: default_title(),
            show_title: true,
            show_spectrum_names: true,
            show_integrals: true,
            show_axis: true,
            show_filepath: false,
            orientation: PrintOrientation::Landscape,
            margin_mm: default_margin_mm(),
            ppm_decimals: default_ppm_decimals(),
            integral_decimals: default_integral_decimals(),
            auto_ticks: default_auto_ticks(),
            tick_major: default_tick_major(),
            tick_minor: default_tick_minor(),
        }
    }
}

/// MultiSpec 専用のアプリケーション永続化設定 (~/.resona/multispec_settings.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSpecSettings {
    #[serde(default)]
    pub current_directory: Option<PathBuf>,
    #[serde(default = "default_ppm_decimals")]
    pub ppm_decimals: usize,
    #[serde(default = "default_integral_decimals")]
    pub integral_decimals: usize,
    #[serde(default = "default_stack_spacing")]
    pub stack_spacing: f64,
    #[serde(default = "default_auto_ticks")]
    pub auto_ticks: bool,
    #[serde(default = "default_tick_major")]
    pub tick_major: f64,
    #[serde(default = "default_tick_minor")]
    pub tick_minor: usize,
    #[serde(default)]
    pub display_settings: DisplaySettings,
    #[serde(default)]
    pub print_settings: MultiSpecPrintSettings,
}

fn default_ppm_decimals() -> usize { 3 }
fn default_integral_decimals() -> usize { 3 }
fn default_stack_spacing() -> f64 { 0.25 }

impl Default for MultiSpecSettings {
    fn default() -> Self {
        Self {
            current_directory: get_user_home_dir(),
            ppm_decimals: default_ppm_decimals(),
            integral_decimals: default_integral_decimals(),
            stack_spacing: default_stack_spacing(),
            auto_ticks: default_auto_ticks(),
            tick_major: default_tick_major(),
            tick_minor: default_tick_minor(),
            display_settings: DisplaySettings::default(),
            print_settings: MultiSpecPrintSettings::default(),
        }
    }
}

pub fn get_multispec_settings_path() -> Option<PathBuf> {
    get_settings_dir().map(|d| d.join("multispec_settings.json"))
}

impl MultiSpecSettings {
    pub fn load() -> Self {
        let path = match get_multispec_settings_path() {
            Some(p) => p,
            None => return Self::default(),
        };

        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = match get_multispec_settings_path() {
            Some(p) => p,
            None => return,
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
