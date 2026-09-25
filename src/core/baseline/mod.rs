use ndarray::Array1;

/// Asymmetric Least Squares Smoothing (ALS) による自動ベースライン補正。
/// Paul H. C. Eilers (Analytical Chemistry 2003, 75, 3631-3636) に準拠。
///
/// 五重対角バンドコレスキー分解 (Band Cholesky) を用いることで、
/// O(N) の計算量・メモリ効率で極めて高速に計算を行う。
///
/// # 引数
/// * `y` - 実部スペクトル (1次元配列)
/// * `lam` - 平滑度パラメータ λ (通常 1e4 〜 1e9, デフォルト 1e8)
/// * `p` - 非対称重み p (通常 0.001 〜 0.1, デフォルト 0.005)
/// * `niter` - 反復回数 (通常 10 回)
pub fn baseline_als(y: &Array1<f64>, lam: f64, p: f64, niter: usize) -> Array1<f64> {
    let n = y.len();
    if n < 4 {
        return Array1::zeros(n);
    }

    let mut w = vec![1.0; n];
    let mut z = vec![0.0; n];

    // D^T D のバンド成分 (主対角, 第1副対角, 第2副対角)
    // 行列サイズ N
    // 主対角: [1, 5, 6, 6, ..., 6, 5, 1] * lam
    // 第1副対角: [-2, -4, -4, ..., -4, -2] * lam (長さ N-1)
    // 第2副対角: [1, 1, ..., 1] * lam (長さ N-2)
    let mut d_diag = vec![6.0 * lam; n];
    d_diag[0] = 1.0 * lam;
    d_diag[1] = 5.0 * lam;
    d_diag[n - 2] = 5.0 * lam;
    d_diag[n - 1] = 1.0 * lam;

    let mut d_sub1 = vec![-4.0 * lam; n - 1];
    d_sub1[0] = -2.0 * lam;
    d_sub1[n - 2] = -2.0 * lam;

    let d_sub2 = vec![1.0 * lam; n - 2];

    for _ in 0..niter {
        // M = W + lam * D^T D
        // M の主対角
        let mut m_diag = vec![0.0; n];
        for i in 0..n {
            m_diag[i] = w[i] + d_diag[i];
        }

        // コレスキー分解 M = L * L^T
        // L: 下三角バンド行列 (bandwidth 2)
        // l_0: 主対角 (長さ N)
        // l_1: 第1下対角 (長さ N-1)
        // l_2: 第2下対角 (長さ N-2)
        let mut l_0 = vec![0.0; n];
        let mut l_1 = vec![0.0; n - 1];
        let mut l_2 = vec![0.0; n - 2];

        for i in 0..n {
            let mut diag_val = m_diag[i];
            if i >= 1 {
                diag_val -= l_1[i - 1] * l_1[i - 1];
            }
            if i >= 2 {
                diag_val -= l_2[i - 2] * l_2[i - 2];
            }

            let l0 = if diag_val > 0.0 { diag_val.sqrt() } else { 1e-12 };
            l_0[i] = l0;

            if i + 1 < n {
                let mut sub1_val = d_sub1[i];
                if i >= 1 {
                    sub1_val -= l_2[i - 1] * l_1[i - 1];
                }
                l_1[i] = sub1_val / l0;
            }

            if i + 2 < n {
                let sub2_val = d_sub2[i];
                l_2[i] = sub2_val / l0;
            }
        }

        // 前進代入: L * v = w * y
        let mut v = vec![0.0; n];
        for i in 0..n {
            let mut rhs = w[i] * y[i];
            if i >= 1 {
                rhs -= l_1[i - 1] * v[i - 1];
            }
            if i >= 2 {
                rhs -= l_2[i - 2] * v[i - 2];
            }
            v[i] = rhs / l_0[i];
        }

        // 後退代入: L^T * z = v
        for i in (0..n).rev() {
            let mut rhs = v[i];
            if i + 1 < n {
                rhs -= l_1[i] * z[i + 1];
            }
            if i + 2 < n {
                rhs -= l_2[i] * z[i + 2];
            }
            z[i] = rhs / l_0[i];
        }

        // 重みの更新: w_i = p if y_i > z_i else (1 - p)
        for i in 0..n {
            w[i] = if y[i] > z[i] { p } else { 1.0 - p };
        }
    }

    Array1::from_vec(z)
}

/// スペクトルから ALS ベースラインを減算した補正後スペクトルと、ベースライン配列を返す。
pub fn apply_baseline_correction(
    spectrum_real: &Array1<f64>,
    lam: f64,
    p: f64,
) -> (Array1<f64>, Array1<f64>) {
    let baseline = baseline_als(spectrum_real, lam, p, 10);
    let corrected = spectrum_real - &baseline;
    (corrected, baseline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_als_against_scipy() {
        let y = Array1::from_vec(vec![
            10.0, 12.0, 11.0, 50.0, 80.0, 60.0, 13.0, 12.0, 11.0, 10.0,
        ]);
        let z = baseline_als(&y, 1000.0, 0.005, 10);

        // SciPyの計算結果:
        // [10.453027, 10.448915, 10.444352, 10.438896, 10.432105, 10.423738, 10.413901, 10.402945, 10.391238, 10.379154]
        let expected = [
            10.453027, 10.448915, 10.444352, 10.438896, 10.432105, 10.423738, 10.413901,
            10.402945, 10.391238, 10.379154,
        ];

        for i in 0..10 {
            assert_relative_eq!(z[i], expected[i], epsilon = 1e-4);
        }
    }
}
