use ndarray::Array1;

/// 化学シフト (ppm) 軸の配列を生成する。
///
/// # 引数
/// * `n_pts` - スペクトルの総ポイント数 (Zero-filled size)
/// * `obs_freq_mhz` - 観測周波数 (MHz)
/// * `spectral_width_hz` - スペクトル幅 (Hz)
/// * `center_ppm` - 中心化学シフト (ppm)
///
/// # 戻り値
/// 単調減少の PPM 配列 (インデックス 0 が最大値/低磁場、末尾が最小値/高磁場)
pub fn compute_ppm_scale(
    n_pts: usize,
    obs_freq_mhz: f64,
    spectral_width_hz: f64,
    center_ppm: f64,
) -> Array1<f64> {
    if n_pts == 0 || obs_freq_mhz <= 0.0 {
        return Array1::zeros(0);
    }

    let car_hz = center_ppm * obs_freq_mhz;
    let n_f64 = n_pts as f64;
    let mut ppm = Array1::zeros(n_pts);

    for i in 0..n_pts {
        let freq_hz = car_hz + spectral_width_hz * (0.5 - (i as f64) / n_f64);
        ppm[i] = freq_hz / obs_freq_mhz;
    }

    ppm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ppm_scale() {
        let ppm = compute_ppm_scale(4, 400.0, 4000.0, 5.0);
        assert_eq!(ppm.len(), 4);
        assert!((ppm[0] - 10.0).abs() < 1e-10); // 5 + 4000/400 * 0.5 = 10
        assert!((ppm[2] - 5.0).abs() < 1e-10);  // 5 + 10 * 0 = 5
        assert!(ppm[0] > ppm[3]); // 単調減少
    }
}
