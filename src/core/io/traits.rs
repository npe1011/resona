use std::path::Path;
use ndarray::Array1;
use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use crate::core::error::Result;

/// NMR測定に関するメタデータ (Python版 ezNMR 完全準拠)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcquisitionMetadata {
    pub title: String,
    #[serde(default)]
    pub date_time: String,
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
    #[serde(default)]
    pub pulse_width_us: Option<f64>,
    #[serde(default)]
    pub pulse_power_attenuation_db: Option<f64>,
    #[serde(default)]
    pub pulse_shape: String,
    #[serde(default)]
    pub decoupling: String,
    #[serde(default)]
    pub decoupling_nucleus: String,
    #[serde(default)]
    pub decoupling_sequence: String,
    pub temperature_celsius: f64,
    #[serde(default)]
    pub spin_rate_hz: Option<f64>,
    pub instrument: String,
    #[serde(default)]
    pub probe: String,
    pub digital_filter_delay: Option<f64>,
    pub center_ppm: f64,
}

impl Default for AcquisitionMetadata {
    fn default() -> Self {
        Self {
            title: String::new(),
            date_time: String::new(),
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
            pulse_width_us: None,
            pulse_power_attenuation_db: None,
            pulse_shape: String::new(),
            decoupling: String::new(),
            decoupling_nucleus: String::new(),
            decoupling_sequence: String::new(),
            temperature_celsius: 25.0,
            spin_rate_hz: None,
            instrument: String::new(),
            probe: String::new(),
            digital_filter_delay: None,
            center_ppm: 0.0,
        }
    }
}

impl AcquisitionMetadata {
    /// Python版 (ezNMR) 完全準拠の表示用行リスト (項目名, フォーマット済値) を生成
    pub fn to_display_rows(&self) -> Vec<(&'static str, String)> {
        let mut rows = Vec::new();
        if !self.title.is_empty() {
            rows.push(("Title", self.title.clone()));
        }
        if !self.date_time.is_empty() {
            rows.push(("Date/Time", self.date_time.clone()));
        }
        if !self.nucleus.is_empty() {
            rows.push(("Nucleus", self.nucleus.clone()));
        }
        if !self.experiment.is_empty() {
            rows.push(("Experiment", self.experiment.clone()));
        }
        if self.obs_freq_mhz > 0.0 {
            rows.push(("Obs. Freq.", format!("{:.2} MHz", self.obs_freq_mhz)));
        }
        if self.spectral_width_hz > 0.0 {
            rows.push(("Spec. Width", format!("{:.2} Hz", self.spectral_width_hz)));
        }
        if self.center_ppm != 0.0 || self.points > 0 {
            rows.push(("Obs. Center", format!("{:.2} ppm", self.center_ppm)));
        }
        if self.points > 0 {
            rows.push(("Points", self.points.to_string()));
        }
        if self.scans > 0 {
            rows.push(("Scans", self.scans.to_string()));
        }
        if self.acquisition_time_sec > 0.0 {
            rows.push(("Acq. Time", format!("{:.4} sec", self.acquisition_time_sec)));
        }
        if self.relaxation_delay_sec > 0.0 {
            rows.push(("Relax. Delay", format!("{:.2} sec", self.relaxation_delay_sec)));
        }
        if self.pulse_angle_deg > 0.0 {
            rows.push(("Pls. Angle", format!("{:.1} deg", self.pulse_angle_deg)));
        }
        if let Some(pw) = self.pulse_width_us {
            rows.push(("Pls. Width", format!("{:.2} usec", pw)));
        }
        if let Some(pa) = self.pulse_power_attenuation_db {
            rows.push(("Pls. Pow. Atten.", format!("{:.1} dB", pa)));
        }
        if !self.pulse_shape.is_empty() {
            rows.push(("Pls. Shape", self.pulse_shape.clone()));
        }
        if !self.decoupling.is_empty() && self.decoupling != "FALSE" {
            rows.push(("Decoupl.", self.decoupling.clone()));
        }
        if !self.decoupling_nucleus.is_empty() {
            rows.push(("Decoupl. Nuc.", self.decoupling_nucleus.clone()));
        }
        if !self.decoupling_sequence.is_empty() {
            rows.push(("Decoupl. Seq.", self.decoupling_sequence.clone()));
        }
        if !self.solvent.is_empty() {
            rows.push(("Solvent", self.solvent.clone()));
        }
        if self.temperature_celsius != 0.0 {
            rows.push(("Temperature", format!("{:.1} C", self.temperature_celsius)));
        }
        if let Some(spin) = self.spin_rate_hz {
            rows.push(("Spin Rate", format!("{:.1} Hz", spin)));
        }
        if !self.instrument.is_empty() {
            rows.push(("Instrument", self.instrument.clone()));
        }
        if !self.probe.is_empty() {
            rows.push(("Probe", self.probe.clone()));
        }
        rows
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
