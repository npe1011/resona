use ndarray::Array1;
use num_complex::Complex64;
use rustfft::FftPlanner;

/// 1次元の配列に対して fftshift (0周波数を中心に移動) を行う
pub fn fftshift<T: Clone>(input: &[T]) -> Vec<T> {
    let n = input.len();
    let mid = (n + 1) / 2;
    let mut out = Vec::with_capacity(n);
    out.extend_from_slice(&input[mid..n]);
    out.extend_from_slice(&input[0..mid]);
    out
}

/// 前進高速フーリエ変換 (Forward FFT) を実行し、nmrglue準拠のNMR順序 (fftshift) で返す
pub fn forward_fft(fid: &Array1<Complex64>) -> Array1<Complex64> {
    let n = fid.len();
    if n == 0 {
        return Array1::from_vec(Vec::new());
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);

    let mut buffer: Vec<Complex64> = fid.to_vec();
    fft.process(&mut buffer);

    let shifted = fftshift(&buffer);
    Array1::from_vec(shifted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fftshift() {
        let v = vec![0, 1, 2, 3];
        assert_eq!(fftshift(&v), vec![2, 3, 0, 1]);
    }
}
