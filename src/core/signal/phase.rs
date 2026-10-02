use std::f64::consts::PI;
use ndarray::Array1;
use num_complex::Complex64;

/// 複素数スペクトルに対して 0次(p0)・1次(p1) 位相補正を適用する (複素数のまま返す)
/// pivot_k: P1回転のピボットとなる複素配列インデックス (0 <= pivot_k < n)
pub fn apply_phase_complex_with_pivot(
    spectrum: &Array1<Complex64>,
    p0_deg: f64,
    p1_deg: f64,
    pivot_k: usize,
) -> Array1<Complex64> {
    let n = spectrum.len();
    if n == 0 {
        return spectrum.clone();
    }

    let p0_rad = p0_deg * PI / 180.0;
    let p1_rad = p1_deg * PI / 180.0;
    let n_f64 = n as f64;
    let pivot_f64 = (pivot_k.min(n - 1)) as f64;

    let mut out = spectrum.clone();
    for k in 0..n {
        let phi = p0_rad + p1_rad * ((k as f64) - pivot_f64) / n_f64;
        let rot = Complex64::from_polar(1.0, phi);
        out[k] *= rot;
    }

    out
}

/// 複素数スペクトルに対して 0次(p0)・1次(p1) 位相補正を適用する (互換用: pivot_k = 0)
pub fn apply_phase_complex(
    spectrum: &Array1<Complex64>,
    p0_deg: f64,
    p1_deg: f64,
) -> Array1<Complex64> {
    apply_phase_complex_with_pivot(spectrum, p0_deg, p1_deg, 0)
}

/// 位相補正を適用し、実部抽出および左右反転 (nmrglueの rev(di(phased)) 準拠) を行って
/// 最終的な実部スペクトル (Y軸データ) を生成する
pub fn apply_phase_and_extract_real_with_pivot(
    spectrum: &Array1<Complex64>,
    p0_deg: f64,
    p1_deg: f64,
    pivot_k: usize,
) -> Array1<f64> {
    let phased = apply_phase_complex_with_pivot(spectrum, p0_deg, p1_deg, pivot_k);
    let n = phased.len();
    let mut real_reversed = Array1::zeros(n);

    for k in 0..n {
        // rev: [N - 1 - k]
        real_reversed[k] = phased[n - 1 - k].re;
    }

    real_reversed
}

/// 位相補正を適用し、実部抽出および左右反転 (互換用: pivot_k = 0)
pub fn apply_phase_and_extract_real(
    spectrum: &Array1<Complex64>,
    p0_deg: f64,
    p1_deg: f64,
) -> Array1<f64> {
    apply_phase_and_extract_real_with_pivot(spectrum, p0_deg, p1_deg, 0)
}

/// 複素スペクトルから振幅（Magnitude: |z| = sqrt(re^2 + im^2)）が最大となるインデックス k を探索する
pub fn find_max_magnitude_index(spectrum: &Array1<Complex64>) -> usize {
    let mut max_mag_sq = -1.0_f64;
    let mut best_k = 0;
    for (k, c) in spectrum.iter().enumerate() {
        let mag_sq = c.norm_sqr();
        if mag_sq > max_mag_sq {
            max_mag_sq = mag_sq;
            best_k = k;
        }
    }
    best_k
}

/// ピボットを old_k から new_k に変更した際に、現在のスペクトルの位相状態を完全に維持するための新しい P0 を計算する
pub fn convert_p0_for_new_pivot(p0: f64, p1: f64, old_k: usize, new_k: usize, n: usize) -> f64 {
    if n == 0 {
        return p0;
    }
    let delta_k = (new_k as f64) - (old_k as f64);
    let mut new_p0 = p0 + p1 * (delta_k / n as f64);
    // Wrap to [-180, 180]
    while new_p0 > 180.0 {
        new_p0 -= 360.0;
    }
    while new_p0 < -180.0 {
        new_p0 += 360.0;
    }
    new_p0
}

/// 指定された PPM 範囲 [p_low, p_high] 内において、
/// 局所極大（または極小）のピークの中で最も振幅（絶対値）の大きいピークの ppm を探す。
/// 極大点が見つからない場合は、範囲内で絶対値が最大となる点の ppm を探す。
/// 範囲内にデータ点がなければ None を返す。
pub fn find_highest_peak_in_range(
    spectrum: &Array1<f64>,
    ppm: &Array1<f64>,
    p_low: f64,
    p_high: f64,
) -> Option<f64> {
    let n = spectrum.len().min(ppm.len());
    if n < 3 {
        return None;
    }

    let min_p = p_low.min(p_high);
    let max_p = p_low.max(p_high);

    let mut best_local_idx = None;
    let mut best_local_mag = f64::NEG_INFINITY;

    let mut fallback_idx = None;
    let mut fallback_mag = f64::NEG_INFINITY;

    for i in 0..n {
        let p = ppm[i];
        if p >= min_p && p <= max_p {
            let mag = spectrum[i].abs();
            if mag > fallback_mag {
                fallback_mag = mag;
                fallback_idx = Some(i);
            }

            if i > 0 && i < n - 1 {
                let v = spectrum[i];
                let is_peak = (v > spectrum[i - 1] && v > spectrum[i + 1])
                    || (v < spectrum[i - 1] && v < spectrum[i + 1]);
                if is_peak && mag > best_local_mag {
                    best_local_mag = mag;
                    best_local_idx = Some(i);
                }
            }
        }
    }

    let target_idx = best_local_idx.or(fallback_idx)?;
    Some(ppm[target_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_zero() {
        let spec = Array1::from_vec(vec![Complex64::new(1.0, 2.0), Complex64::new(3.0, 4.0)]);
        let real = apply_phase_and_extract_real(&spec, 0.0, 0.0);
        assert_eq!(real.len(), 2);
        assert_eq!(real[0], 3.0); // reversed
        assert_eq!(real[1], 1.0);
    }

    #[test]
    fn test_phase_pivot_invariance() {
        // ピボット点では P1 をいくら回しても位相角が変わらないことを検証
        let spec = Array1::from_vec(vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(2.0, 0.0),
            Complex64::new(3.0, 0.0),
            Complex64::new(4.0, 0.0),
        ]);
        let pivot_k = 2;
        let phased_p1_0 = apply_phase_complex_with_pivot(&spec, 45.0, 0.0, pivot_k);
        let phased_p1_90 = apply_phase_complex_with_pivot(&spec, 45.0, 90.0, pivot_k);
        let phased_p1_neg = apply_phase_complex_with_pivot(&spec, 45.0, -180.0, pivot_k);

        // pivot_k (インデックス 2) における位相回転は P1 の値によらず完全に一致するはず
        assert!((phased_p1_0[pivot_k].re - phased_p1_90[pivot_k].re).abs() < 1e-10);
        assert!((phased_p1_0[pivot_k].im - phased_p1_90[pivot_k].im).abs() < 1e-10);
        assert!((phased_p1_0[pivot_k].re - phased_p1_neg[pivot_k].re).abs() < 1e-10);
    }

    #[test]
    fn test_convert_p0_preserves_spectrum() {
        // ピボットを移動しても convert_p0_for_new_pivot を使えば全周波数の位相が不変であることを検証
        let n = 128;
        let mut data = Vec::with_capacity(n);
        for i in 0..n {
            data.push(Complex64::new((i as f64).sin(), (i as f64).cos()));
        }
        let spec = Array1::from_vec(data);

        let old_k = 30;
        let new_k = 85;
        let p0 = 25.0;
        let p1 = 60.0;

        let before = apply_phase_complex_with_pivot(&spec, p0, p1, old_k);
        let new_p0 = convert_p0_for_new_pivot(p0, p1, old_k, new_k, n);
        let after = apply_phase_complex_with_pivot(&spec, new_p0, p1, new_k);

        for k in 0..n {
            assert!((before[k].re - after[k].re).abs() < 1e-9, "Mismatch at k={}: before={}, after={}", k, before[k].re, after[k].re);
            assert!((before[k].im - after[k].im).abs() < 1e-9, "Mismatch at k={}: before={}, after={}", k, before[k].im, after[k].im);
        }
    }

    #[test]
    fn test_find_max_magnitude_index() {
        let spec = Array1::from_vec(vec![
            Complex64::new(1.0, 1.0),   // norm_sqr = 2
            Complex64::new(0.0, -5.0),  // norm_sqr = 25 (最大)
            Complex64::new(3.0, 3.0),   // norm_sqr = 18
        ]);
        assert_eq!(find_max_magnitude_index(&spec), 1);
    }

    #[test]
    fn test_find_highest_peak_in_range() {
        let ppm = Array1::from_vec(vec![10.0, 8.0, 6.0, 4.0, 2.0, 0.0]);
        let spec = Array1::from_vec(vec![1.0, 5.0, 2.0, 10.0, 3.0, 0.5]);

        // 範囲 [5.0, 9.0] -> 8.0 ppm (強度5.0) のピーク
        let peak1 = find_highest_peak_in_range(&spec, &ppm, 5.0, 9.0);
        assert_eq!(peak1, Some(8.0));

        // 範囲 [1.0, 9.0] -> 4.0 ppm (強度10.0) のピーク
        let peak2 = find_highest_peak_in_range(&spec, &ppm, 1.0, 9.0);
        assert_eq!(peak2, Some(4.0));

        // 負のピーク (絶対値で最大)
        let spec_neg = Array1::from_vec(vec![1.0, -12.0, 2.0, 10.0, 3.0, 0.5]);
        let peak_neg = find_highest_peak_in_range(&spec_neg, &ppm, 5.0, 9.0);
        assert_eq!(peak_neg, Some(8.0));

        // 範囲外 [-5.0, -1.0] -> None
        let peak3 = find_highest_peak_in_range(&spec, &ppm, -5.0, -1.0);
        assert_eq!(peak3, None);
    }
}
