use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::gui::dialogs::print_dialog::PrintSettings;
use crate::gui::dialogs::FullAutoBaselineChoice;

/// アプリケーション全体の永続化設定 (~/.resona/settings.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub current_directory: Option<PathBuf>,

    #[serde(default = "default_ppm_decimals")]
    pub ppm_decimals: usize,

    #[serde(default = "default_integral_decimals")]
    pub integral_decimals: usize,

    #[serde(default = "default_multiview_ratio")]
    pub multiview_ratio: f64,

    #[serde(default = "default_multiview_auto_align")]
    pub multiview_auto_align: bool,

    #[serde(default)]
    pub full_auto_baseline_choice: FullAutoBaselineChoice,

    #[serde(default = "default_full_auto_airpls_lambda")]
    pub full_auto_airpls_lambda: f64,

    #[serde(default = "default_full_auto_poly_order")]
    pub full_auto_poly_order: usize,

    #[serde(default)]
    pub full_auto_integration: bool,

    #[serde(default)]
    pub print_settings: PrintSettings,
}

fn default_ppm_decimals() -> usize { 3 }
fn default_integral_decimals() -> usize { 3 }
fn default_multiview_ratio() -> f64 { 3.0 }
fn default_multiview_auto_align() -> bool { true }
fn default_full_auto_airpls_lambda() -> f64 { 8.0 }
fn default_full_auto_poly_order() -> usize { 3 }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            current_directory: get_user_home_dir(),
            ppm_decimals: default_ppm_decimals(),
            integral_decimals: default_integral_decimals(),
            multiview_ratio: default_multiview_ratio(),
            multiview_auto_align: default_multiview_auto_align(),
            full_auto_baseline_choice: FullAutoBaselineChoice::default(),
            full_auto_airpls_lambda: default_full_auto_airpls_lambda(),
            full_auto_poly_order: default_full_auto_poly_order(),
            full_auto_integration: false,
            print_settings: PrintSettings::default(),
        }
    }
}

/// OSに依存しないユーザーホームディレクトリの取得
pub fn get_user_home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(home);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// .resona 設定ディレクトリのパス (~/.resona)
pub fn get_settings_dir() -> Option<PathBuf> {
    get_user_home_dir().map(|h| h.join(".resona"))
}

/// settings.json のパス (~/.resona/settings.json)
pub fn get_settings_path() -> Option<PathBuf> {
    get_settings_dir().map(|d| d.join("settings.json"))
}

impl AppSettings {
    /// 設定ファイル (~/.resona/settings.json) から設定を読み出す
    pub fn load() -> Self {
        let home = get_user_home_dir();
        let path = match get_settings_path() {
            Some(p) => p,
            None => return Self::default(),
        };

        if !path.exists() {
            return Self::default();
        }

        let mut settings: Self = match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Warning: Failed to parse settings file {:?}: {}", path, e);
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!("Warning: Failed to read settings file {:?}: {}", path, e);
                Self::default()
            }
        };

        // 作業ディレクトリの検証: 存在しない場合はホームディレクトリにフォールバック
        if let Some(ref dir) = settings.current_directory {
            if !dir.exists() || !dir.is_dir() {
                settings.current_directory = home;
            }
        } else {
            settings.current_directory = home;
        }

        settings
    }

    /// 設定ファイル (~/.resona/settings.json) へ設定を保存する
    pub fn save(&self) -> Result<(), String> {
        let dir = get_settings_dir().ok_or_else(|| "Could not determine settings directory".to_string())?;
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create settings directory {:?}: {}", dir, e))?;
        }

        let path = dir.join("settings.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;

        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write settings file {:?}: {}", path, e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_settings_default() {
        let s = AppSettings::default();
        assert_eq!(s.ppm_decimals, 3);
        assert_eq!(s.integral_decimals, 3);
        assert!((s.multiview_ratio - 3.0).abs() < 1e-6);
        assert!(s.multiview_auto_align);
        assert_eq!(s.full_auto_baseline_choice, FullAutoBaselineChoice::None);
        assert!(!s.full_auto_integration);
        assert!(s.print_settings.spectrum);
    }

    #[test]
    fn test_app_settings_serialization_roundtrip() {
        let mut s = AppSettings::default();
        s.ppm_decimals = 4;
        s.integral_decimals = 2;
        s.multiview_ratio = 8.0;
        s.multiview_auto_align = false;
        s.full_auto_baseline_choice = FullAutoBaselineChoice::AirPLS;
        s.full_auto_integration = true;
        s.print_settings.filename = false;

        let json = serde_json::to_string(&s).unwrap();
        let decoded: AppSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.ppm_decimals, 4);
        assert_eq!(decoded.integral_decimals, 2);
        assert!((decoded.multiview_ratio - 8.0).abs() < 1e-6);
        assert!(!decoded.multiview_auto_align);
        assert_eq!(decoded.full_auto_baseline_choice, FullAutoBaselineChoice::AirPLS);
        assert!(decoded.full_auto_integration);
        assert!(!decoded.print_settings.filename);
    }
}
