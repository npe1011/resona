use ndarray::Array1;
use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::core::error::Result;
use crate::core::io::{AcquisitionMetadata, RawFid};
use crate::core::signal::{
    apply_phase_and_extract_real, apply_window, apply_zerofill, compute_ppm_scale, forward_fft,
    remove_fractional_delay, WindowFunction,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FtSettings {
    #[serde(default)]
    pub window: WindowFunction,
    #[serde(default = "default_zf")]
    pub zf_factor: usize,
    #[serde(default = "default_true")]
    pub remove_digital_filter: bool,
    #[serde(default = "default_true")]
    pub auto_phase: bool,
}

fn default_zf() -> usize {
    4
}

fn default_true() -> bool {
    true
}

impl Default for FtSettings {
    fn default() -> Self {
        Self {
            window: WindowFunction::Exponential { lb: 0.12 },
            zf_factor: 4,
            remove_digital_filter: true,
            auto_phase: true,
        }
    }
}

/// 処理結果のスペクトルデータ構造体
#[derive(Debug, Clone)]
pub struct ProcessedSpectrum {
    /// 化学シフト軸 (ppm)
    pub ppm: Array1<f64>,
    /// 位相補正済みの実部スペクトル
    pub spectrum_real: Array1<f64>,
    /// 位相補正前の未補正複素数スペクトル (GUIスライダーでの高速位相更新用)
    pub complex_spectrum_unphased: Array1<Complex64>,
    /// 適用された 0次位相 (度)
    pub p0: f64,
    /// 適用された 1次位相 (度)
    pub p1: f64,
    /// 測定メタデータ
    pub metadata: AcquisitionMetadata,
    /// 処理に使用された設定
    pub ft_settings: FtSettings,
}

/// 生FIDデータに対し、デジタルフィルタ除去・窓関数・ゼロフィリング・FFT・位相補正・PPMスケール算出を行う。
pub fn process_raw_fid(
    raw_fid: &RawFid,
    ft_settings: &FtSettings,
    p0: f64,
    p1: f64,
) -> Result<ProcessedSpectrum> {
    let mut fid = raw_fid.data.clone();

    // 1. デジタルフィルタ群遅延 (Fractional Shift) の除去
    if ft_settings.remove_digital_filter {
        if let Some(delay) = raw_fid.group_delay {
            fid = remove_fractional_delay(&fid, delay);
        }
    }

    // 2. 窓関数 (Apodization)
    fid = apply_window(&fid, &ft_settings.window, raw_fid.metadata.spectral_width_hz);

    // 3. ゼロフィリング (Zero-filling)
    let target_size = raw_fid.metadata.points * ft_settings.zf_factor.max(1);
    let fid_zf = apply_zerofill(&fid, target_size);

    // 4. 前進フーリエ変換 (Forward FFT)
    let unphased_complex = forward_fft(&fid_zf);

    // 5. 位相補正と実部抽出 (左右反転)
    let spectrum_real = apply_phase_and_extract_real(&unphased_complex, p0, p1);

    // 6. PPMスケール算出
    let ppm = compute_ppm_scale(
        target_size,
        raw_fid.metadata.obs_freq_mhz,
        raw_fid.metadata.spectral_width_hz,
        raw_fid.metadata.center_ppm,
    );

    Ok(ProcessedSpectrum {
        ppm,
        spectrum_real,
        complex_spectrum_unphased: unphased_complex,
        p0,
        p1,
        metadata: raw_fid.metadata.clone(),
        ft_settings: ft_settings.clone(),
    })
}
