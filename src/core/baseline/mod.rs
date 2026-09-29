use ndarray::Array1;
use serde::{Deserialize, Serialize};

/// ベースライン補正手法およびパラメータ (状態管理・プロジェクト保存対応)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BaselineMethod {
    None,
    AirPLS {
        log_lambda: f64,
        #[serde(default = "default_airpls_max_iter")]
        max_iter: usize,
    },
    Polynomial {
        order: usize,
        #[serde(default = "default_poly_max_iter")]
        max_iter: usize,
    },
}

fn default_airpls_max_iter() -> usize {
    15
}

fn default_poly_max_iter() -> usize {
    10
}

impl Default for BaselineMethod {
    fn default() -> Self {
        BaselineMethod::None
    }
}

/// 五重対角バンドコレスキー分解を用いて (W + lam * D^T D) z = W * y を O(N) で高速に解く内部ソルバー。
/// D は 2 階差分作用素。
fn solve_band_cholesky_order2(y: &[f64], w: &[f64], lam: f64) -> Vec<f64> {
    let n = y.len();
    if n < 4 {
        return y.to_vec();
    }

    // D^T D のバンド成分 (主対角, 第1副対角, 第2副対角)
    let mut d_diag = vec![6.0 * lam; n];
    d_diag[0] = 1.0 * lam;
    d_diag[1] = 5.0 * lam;
    d_diag[n - 2] = 5.0 * lam;
    d_diag[n - 1] = 1.0 * lam;

    let mut d_sub1 = vec![-4.0 * lam; n - 1];
    d_sub1[0] = -2.0 * lam;
    d_sub1[n - 2] = -2.0 * lam;

    let d_sub2 = vec![1.0 * lam; n - 2];

    // M = W + lam * D^T D
    let mut m_diag = vec![0.0; n];
    for i in 0..n {
        m_diag[i] = w[i] + d_diag[i];
    }

    // コレスキー分解 M = L * L^T (bandwidth = 2)
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
    let mut z = vec![0.0; n];
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

    z
}

/// Adaptive Iteratively Reweighted Penalized Least Squares (airPLS) によるベースライン補正。
/// Z.-M. Zhang et al., Analyst 2010, 135, 1138-1146 に準拠。
///
/// ALS の非対称パラメータ p を自動適応化し、ピーク足元の削れ (erosion) を大幅に抑制する。
pub fn baseline_airpls(y: &Array1<f64>, lam: f64, max_iter: usize) -> Array1<f64> {
    let n = y.len();
    if n < 4 {
        return Array1::zeros(n);
    }

    let temp_y;
    let y_slice = match y.as_slice() {
        Some(s) => s,
        None => {
            temp_y = y.to_vec();
            &temp_y
        }
    };
    let sum_abs_y: f64 = y_slice.iter().map(|v| v.abs()).sum::<f64>().max(1e-12);

    let mut w = vec![1.0; n];
    let mut z = vec![0.0; n];

    for iter in 1..=max_iter {
        z = solve_band_cholesky_order2(y_slice, &w, lam);

        // 残差 d = y - z
        let mut sum_neg_d = 0.0;
        let mut neg_count = 0;
        for i in 0..n {
            let d = y_slice[i] - z[i];
            if d < 0.0 {
                sum_neg_d += d.abs();
                neg_count += 1;
            }
        }

        // 終了条件: 負の残差が信号全体の 0.1% 未満になったら収束
        if sum_neg_d < 0.001 * sum_abs_y || neg_count == 0 {
            break;
        }

        // 重み更新: d_i >= 0 はピーク領域 (w_i = 0), d_i < 0 はベースライン領域 (指数関数的重み)
        for i in 0..n {
            let d = y_slice[i] - z[i];
            if d >= 0.0 {
                w[i] = 0.0;
            } else {
                let ratio = (iter as f64) * d.abs() / sum_neg_d;
                // exp のオーバーフロー防止 (max 20.0 -> ~4.8e8)
                w[i] = ratio.min(20.0).exp();
            }
        }
    }

    Array1::from_vec(z)
}

/// Modified Polynomial Fitting (ModPoly) による多項式ベースライン補正。
/// Lieber & Mahadevan-Jansen, Appl. Spectrosc. 2003, 57, 1363-1367 に準拠。
///
/// 緩やかな傾きやお椀型・S字型の歪みを、ピークを全く削ることなく安全に補正する。
pub fn baseline_polynomial(y: &Array1<f64>, order: usize, max_iter: usize) -> Array1<f64> {
    let n = y.len();
    let m = order.clamp(1, 6);
    if n <= m + 1 {
        return Array1::zeros(n);
    }

    // 数値安定化のため x を [-1.0, 1.0] に正規化
    let mut x_norm = vec![0.0; n];
    for i in 0..n {
        x_norm[i] = 2.0 * (i as f64) / ((n - 1) as f64) - 1.0;
    }

    // ヴァンデルモンド行列の基底列のべき乗和を事前計算 (A^T A の構築用)
    // pow_sum[k] = sum_{i=0}^{n-1} x_norm[i]^k (k = 0..2*m)
    let mut pow_sum = vec![0.0; 2 * m + 1];
    for &x in &x_norm {
        let mut cur = 1.0;
        for k in 0..=2 * m {
            pow_sum[k] += cur;
            cur *= x;
        }
    }

    // A^T A (サイズ (m+1) x (m+1))
    let dim = m + 1;
    let mut ata = vec![vec![0.0; dim]; dim];
    for r in 0..dim {
        for c in 0..dim {
            ata[r][c] = pow_sum[r + c];
        }
    }

    // 現在の信号 (初回は生スペクトル)
    let raw_y = y.to_vec();
    let mut current_y = raw_y.clone();
    let mut coeffs = vec![0.0; dim];

    for _ in 0..max_iter {
        // A^T * y_current の構築
        let mut aty = vec![0.0; dim];
        for i in 0..n {
            let val = current_y[i];
            let mut cur_x = 1.0;
            let x = x_norm[i];
            for k in 0..dim {
                aty[k] += cur_x * val;
                cur_x *= x;
            }
        }

        // ガウス消去法 (ピボット選択付き) で ata * coeffs = aty を解く
        if let Some(sol) = solve_linear_system(&ata, &aty) {
            coeffs = sol;
        } else {
            break;
        }

        // 多項式曲線 P(x) と残差の計算
        let mut neg_diff_sq_sum = 0.0;
        let mut neg_count = 0;
        let mut poly_vals = vec![0.0; n];

        for i in 0..n {
            let x = x_norm[i];
            let mut p_val = 0.0;
            let mut cur_x = 1.0;
            for k in 0..dim {
                p_val += coeffs[k] * cur_x;
                cur_x *= x;
            }
            poly_vals[i] = p_val;

            let diff = raw_y[i] - p_val;
            if diff < 0.0 {
                neg_diff_sq_sum += diff * diff;
                neg_count += 1;
            }
        }

        // 負の残差の標準偏差 (ノイズレベル推定)
        let sigma = if neg_count > 0 {
            (neg_diff_sq_sum / neg_count as f64).sqrt()
        } else {
            0.0
        };

        // 次の反復用の信号更新: ピーク部分 (y > P(x) + sigma) をクリップ
        let mut max_change = 0.0_f64;
        for i in 0..n {
            let target = if raw_y[i] > poly_vals[i] + sigma {
                poly_vals[i] + sigma
            } else {
                raw_y[i]
            };
            max_change = max_change.max((current_y[i] - target).abs());
            current_y[i] = target;
        }

        if max_change < 1e-5 {
            break;
        }
    }

    // 最終多項式曲線を計算して返す
    let mut baseline = vec![0.0; n];
    for i in 0..n {
        let x = x_norm[i];
        let mut p_val = 0.0;
        let mut cur_x = 1.0;
        for k in 0..dim {
            p_val += coeffs[k] * cur_x;
            cur_x *= x;
        }
        baseline[i] = p_val;
    }

    Array1::from_vec(baseline)
}

/// 小型線形方程式系 (dim <= 7) をガウス消去法で解くヘルパー
fn solve_linear_system(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut mat = vec![vec![0.0; n + 1]; n];
    for r in 0..n {
        for c in 0..n {
            mat[r][c] = a[r][c];
        }
        mat[r][n] = b[r];
    }

    for i in 0..n {
        // 部分ピボット選択
        let mut max_row = i;
        let mut max_val = mat[i][i].abs();
        for r in (i + 1)..n {
            if mat[r][i].abs() > max_val {
                max_val = mat[r][i].abs();
                max_row = r;
            }
        }
        if max_val < 1e-14 {
            return None; // 特異行列
        }
        mat.swap(i, max_row);

        let pivot = mat[i][i];
        for c in i..=n {
            mat[i][c] /= pivot;
        }

        for r in 0..n {
            if r != i {
                let factor = mat[r][i];
                for c in i..=n {
                    mat[r][c] -= factor * mat[i][c];
                }
            }
        }
    }

    let mut x = vec![0.0; n];
    for i in 0..n {
        x[i] = mat[i][n];
    }
    Some(x)
}

/// 旧 ALS アルゴリズム (互換性用)
pub fn baseline_als(y: &Array1<f64>, lam: f64, p: f64, niter: usize) -> Array1<f64> {
    let n = y.len();
    if n < 4 {
        return Array1::zeros(n);
    }
    let y_slice = y.as_slice().unwrap_or(&[]);
    let mut w = vec![1.0; n];
    let mut z = vec![0.0; n];

    for _ in 0..niter {
        z = solve_band_cholesky_order2(y_slice, &w, lam);
        for i in 0..n {
            w[i] = if y_slice[i] > z[i] { p } else { 1.0 - p };
        }
    }

    Array1::from_vec(z)
}

/// BaselineMethod に基づいてベースライン補正を適用し、(補正後スペクトル, ベースライン配列) を返す。
pub fn apply_baseline_method(
    spectrum_real: &Array1<f64>,
    method: BaselineMethod,
) -> (Array1<f64>, Option<Array1<f64>>) {
    match method {
        BaselineMethod::None => (spectrum_real.clone(), None),
        BaselineMethod::AirPLS { log_lambda, max_iter } => {
            let lam = 10.0_f64.powf(log_lambda);
            let bl = baseline_airpls(spectrum_real, lam, max_iter);
            let corrected = spectrum_real - &bl;
            (corrected, Some(bl))
        }
        BaselineMethod::Polynomial { order, max_iter } => {
            let bl = baseline_polynomial(spectrum_real, order, max_iter);
            let corrected = spectrum_real - &bl;
            (corrected, Some(bl))
        }
    }
}

/// 互換用ラッパー
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

        let expected = [
            10.453027, 10.448915, 10.444352, 10.438896, 10.432105, 10.423738, 10.413901,
            10.402945, 10.391238, 10.379154,
        ];

        for i in 0..10 {
            assert_relative_eq!(z[i], expected[i], epsilon = 1e-4);
        }
    }

    #[test]
    fn test_airpls_baseline() {
        // ベースライン(10.0)の上にピークがあるデータ
        let mut y = vec![10.0; 50];
        y[20] = 100.0;
        y[21] = 150.0;
        y[22] = 100.0;
        let y_arr = Array1::from_vec(y);

        let bl = baseline_airpls(&y_arr, 1e5, 15);
        // ピークトップ(150.0)に引っ張られず、ベースライン(~10.0)に沿っていること
        assert_relative_eq!(bl[21], 10.0, epsilon = 2.0);
        assert_relative_eq!(bl[0], 10.0, epsilon = 1.0);
    }

    #[test]
    fn test_polynomial_baseline() {
        // 放物線状のベースライン (y = 5 * x^2 + 10) + ピーク
        let n = 51;
        let mut y = vec![0.0; n];
        for i in 0..n {
            let x = 2.0 * (i as f64) / ((n - 1) as f64) - 1.0;
            y[i] = 5.0 * x * x + 10.0;
        }
        y[25] += 80.0; // ピーク
        let y_arr = Array1::from_vec(y);

        let bl = baseline_polynomial(&y_arr, 2, 10);
        // 頂点付近でもピークに引っ張られず、元のベースライン(~10.0)に適合
        assert_relative_eq!(bl[25], 10.0, epsilon = 1.0);
        // 端部
        assert_relative_eq!(bl[0], 15.0, epsilon = 1.0);
    }
}
