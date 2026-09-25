use std::path::Path;
use ndarray::Array1;
use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use crate::core::error::Result;

/// NMR測定に関するメタデータ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcquisitionMetadata {
    pub title: String,
    pub experiment: String,
    pub solvent: String,
    pub nucleus: String,
    pub obs_freq_mhz: f64,
    pub spectral_width_hz: f64,
    pub points: usize,
    pub scans: u32,
    pub acquisition_time_sec: f64,
    pub relaxation_delay_sec: f64,
    pub pulse_angle_deg: f64,
    pub temperature_celsius: f64,
    pub instrument: String,
    pub digital_filter_delay: Option<f64>,
    pub center_ppm: f64,
}

impl Default for AcquisitionMetadata {
    fn default() -> Self {
        Self {
            title: String::new(),
            experiment: String::new(),
            solvent: String::new(),
            nucleus: "1H".to_string(),
            obs_freq_mhz: 400.0,
            spectral_width_hz: 8000.0,
            points: 0,
            scans: 1,
            acquisition_time_sec: 0.0,
            relaxation_delay_sec: 0.0,
            pulse_angle_deg: 0.0,
            temperature_celsius: 25.0,
            instrument: String::new(),
            digital_filter_delay: None,
            center_ppm: 0.0,
        }
    }
}

/// 読み込まれた生FIDデータおよびメタデータ
#[derive(Debug, Clone)]
pub struct RawFid {
    /// DCオフセット除去済みの生FID複素数信号
    pub data: Array1<Complex64>,
    /// 測定メタデータ
    pub metadata: AcquisitionMetadata,
    /// デジタルフィルタの群遅延（ポイント数、小数）
    pub group_delay: Option<f64>,
}

/// NMR生データソースの抽象インターフェース
pub trait NmrDataSource {
    /// 指定されたファイルパスから生FIDデータを読み込む
    fn read_fid<P: AsRef<Path>>(path: P) -> Result<RawFid>;
}
