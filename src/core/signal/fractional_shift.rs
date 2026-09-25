use std::f64::consts::PI;
use ndarray::Array1;
use num_complex::Complex64;
use rustfft::FftPlanner;

/// フーリエシフト定理を用いて、サブサンプル精度でFIDの循環シフトを行う。
/// デジタルフィルタの群遅延 D (ポイント単位) を補正するために使用する。
pub fn remove_fractional_delay(fid: &Array1<Complex64>, shift_pts: f64) -> Array1<Complex64> {
    let n = fid.len();
    if n == 0 || shift_pts == 0.0 {
        return fid.clone();
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);

    // 1. Forward FFT
    let mut buffer: Vec<Complex64> = fid.to_vec();
    fft.process(&mut buffer);

    // 2. 線形位相回転: S_shifted[k] = S[k] * exp(j * 2 * pi * k_tilde * shift_pts / N)
    // k_tilde: 0..N/2 は k, N/2..N は k - N
    let n_f64 = n as f64;
    for k in 0..n {
        let k_tilde = if k < (n + 1) / 2 {
            k as f64
        } else {
            k as f64 - n_f64
        };
        let angle = 2.0 * PI * k_tilde * shift_pts / n_f64;
        let rot = Complex64::from_polar(1.0, angle);
        buffer[k] *= rot;
    }

    // 3. Inverse FFT
    ifft.process(&mut buffer);

    // rustfftのIFFTは正規化されていないため、1/Nでスケーリング
    let scale = 1.0 / n_f64;
    for val in &mut buffer {
        *val *= scale;
    }

    Array1::from_vec(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_shift() {
        let data = Array1::from_vec(vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(2.0, 1.0),
            Complex64::new(3.0, -1.0),
            Complex64::new(4.0, 2.0),
        ]);
        let shifted = remove_fractional_delay(&data, 0.0);
        for i in 0..data.len() {
            assert!((data[i] - shifted[i]).norm() < 1e-10);
        }
    }
}
