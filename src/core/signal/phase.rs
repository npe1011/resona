use std::f64::consts::PI;
use ndarray::Array1;
use num_complex::Complex64;

/// 複素数スペクトルに対して 0次(p0)・1次(p1) 位相補正を適用する (複素数のまま返す)
pub fn apply_phase_complex(
    spectrum: &Array1<Complex64>,
    p0_deg: f64,
    p1_deg: f64,
) -> Array1<Complex64> {
    let n = spectrum.len();
    if n == 0 {
        return spectrum.clone();
    }

    let p0_rad = p0_deg * PI / 180.0;
    let p1_rad = p1_deg * PI / 180.0;
    let n_f64 = n as f64;

    let mut out = spectrum.clone();
    for k in 0..n {
        let phi = p0_rad + p1_rad * (k as f64) / n_f64;
        let rot = Complex64::from_polar(1.0, phi);
        out[k] *= rot;
    }

    out
}

/// 位相補正を適用し、実部抽出および左右反転 (nmrglueの rev(di(phased)) 準拠) を行って
/// 最終的な実部スペクトル (Y軸データ) を生成する
pub fn apply_phase_and_extract_real(
    spectrum: &Array1<Complex64>,
    p0_deg: f64,
    p1_deg: f64,
) -> Array1<f64> {
    let phased = apply_phase_complex(spectrum, p0_deg, p1_deg);
    let n = phased.len();
    let mut real_reversed = Array1::zeros(n);

    for k in 0..n {
        // rev: [N - 1 - k]
        real_reversed[k] = phased[n - 1 - k].re;
    }

    real_reversed
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
}
