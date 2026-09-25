use ndarray::Array1;
use num_complex::Complex64;

/// ゼロフィリング (Zero-filling) を適用する。
/// 元データサイズ N に対し、target_size (通常 N * zf_factor) まで末尾に 0 を付加する。
pub fn apply_zerofill(fid: &Array1<Complex64>, target_size: usize) -> Array1<Complex64> {
    let n = fid.len();
    if target_size <= n {
        return fid.clone();
    }

    let mut padded = Vec::with_capacity(target_size);
    padded.extend_from_slice(fid.as_slice().unwrap_or(&[]));
    padded.resize(target_size, Complex64::new(0.0, 0.0));

    Array1::from_vec(padded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zerofill() {
        let fid = Array1::from_vec(vec![Complex64::new(1.0, 0.0), Complex64::new(2.0, 0.0)]);
        let zf = apply_zerofill(&fid, 4);
        assert_eq!(zf.len(), 4);
        assert_eq!(zf[0], Complex64::new(1.0, 0.0));
        assert_eq!(zf[1], Complex64::new(2.0, 0.0));
        assert_eq!(zf[2], Complex64::new(0.0, 0.0));
        assert_eq!(zf[3], Complex64::new(0.0, 0.0));
    }
}
