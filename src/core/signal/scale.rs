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

/// PPM軸の主目盛り間隔 (step) と表示小数点桁数 (decimals) を計算する共通関数。
///
/// 画面表示、印刷プレビュー、SVGエクスポート、直接印刷で同一の計算を保証する。
pub fn calc_ppm_ticks(span: f64, auto_ticks: bool, tick_major: f64) -> (f64, usize) {
    if !auto_ticks && tick_major > 1e-4 {
        let s = tick_major;
        let d = if s < 0.0099 { 3 } else if s < 0.099 { 2 } else if s < 0.99 { 1 } else { 0 };
        (s, d)
    } else {
        let s_abs = span.abs();
        if s_abs <= 1e-6 {
            return (1.0, 0);
        }
        // 13C NMR (通常スパン 30〜300 ppm) は 10.0 ppm を標準とする
        if (30.0..=300.0).contains(&s_abs) {
            (10.0, 0)
        } else {
            let approx_ticks = 8.0;
            let rough_step = s_abs / approx_ticks;
            let exponent = rough_step.log10().floor() as i32;
            let base = 10.0_f64.powi(exponent);
            let fraction = rough_step / base;

            let s = if fraction < 1.5 {
                1.0 * base
            } else if fraction < 3.0 {
                2.0 * base
            } else if fraction < 7.0 {
                5.0 * base
            } else {
                10.0 * base
            };
            let d = (-exponent).max(0) as usize;
            (s, d)
        }
    }
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

    #[test]
    fn test_calc_ppm_ticks() {
        // 1H NMR 典型 (12 ppm) -> step = 2.0, dec = 0
        let (step1, dec1) = calc_ppm_ticks(12.0, true, 1.0);
        assert_eq!(step1, 2.0);
        assert_eq!(dec1, 0);

        // 13C NMR 典型 (220 ppm) -> step = 10.0, dec = 0
        let (step2, dec2) = calc_ppm_ticks(220.0, true, 1.0);
        assert_eq!(step2, 10.0);
        assert_eq!(dec2, 0);

        // 狭いズーム (1.0 ppm) -> step = 0.1, dec = 1
        let (step3, dec3) = calc_ppm_ticks(1.0, true, 1.0);
        assert!((step3 - 0.1).abs() < 1e-6);
        assert_eq!(dec3, 1);

        // 手動指定 (auto_ticks = false, tick_major = 0.5) -> step = 0.5, dec = 1
        let (step4, dec4) = calc_ppm_ticks(12.0, false, 0.5);
        assert_eq!(step4, 0.5);
        assert_eq!(dec4, 1);
    }
}
