use std::f64::consts::PI;
use ndarray::Array1;
use num_complex::Complex64;

/// ACME 自動位相補正の目的関数スコアを計算する
/// Chen Li et al. (Journal of Magnetic Resonance 158 (2002) 164-168)
pub fn acme_score(unphased_complex: &Array1<Complex64>, p0_deg: f64, p1_deg: f64) -> f64 {
    let n = unphased_complex.len();
    if n < 2 {
        return 0.0;
    }

    let p0_rad = p0_deg * PI / 180.0;
    let p1_rad = p1_deg * PI / 180.0;
    let n_f64 = n as f64;

    // 1. 位相回転後の実部を計算
    let mut r = Vec::with_capacity(n);
    let mut max_r = f64::NEG_INFINITY;

    for k in 0..n {
        let phi = p0_rad + p1_rad * (k as f64) / n_f64;
        let rot = Complex64::from_polar(1.0, phi);
        let val = (unphased_complex[k] * rot).re;
        if val > max_r {
            max_r = val;
        }
        r.push(val);
    }

    if max_r <= 0.0 {
        return 1e10; // 最大値が正でなければ大きなペナルティ
    }

    // 2. 1階微分の絶対値: ds1[i] = |r[i+1] - r[i]| / 2
    let mut ds1 = Vec::with_capacity(n - 1);
    let mut sum_ds1 = 0.0;
    for i in 0..n - 1 {
        let diff = (r[i + 1] - r[i]).abs() / 2.0;
        sum_ds1 += diff;
        ds1.push(diff);
    }

    if sum_ds1 <= 0.0 {
        return 1e10;
    }

    // 3. エントロピーの計算
    let mut h = 0.0;
    let inv_sum = 1.0 / sum_ds1;
    for &d in &ds1 {
        let p = d * inv_sum;
        if p > 0.0 {
            h -= p * p.ln();
        }
    }

    // 4. 負値ペナルティ
    let mut sum_neg = 0.0;
    let mut penalty_sum = 0.0;
    for &val in &r {
        let a_neg = val - val.abs(); // r < 0 なら 2 * val, r >= 0 なら 0
        sum_neg += a_neg;
        if a_neg < 0.0 {
            let half = a_neg / 2.0;
            penalty_sum += half * half;
        }
    }

    let penalty = if sum_neg < 0.0 {
        1000.0 * penalty_sum
    } else {
        0.0
    };

    (h + penalty) / n_f64 / max_r
}

/// 2変数専用の Nelder-Mead シンプレックス最適化法 (SciPy fmin 完全互換)
pub fn nelder_mead_2d<F>(
    mut func: F,
    x0: [f64; 2],
    ftol: f64,
    xtol: f64,
    maxiter: usize,
) -> [f64; 2]
where
    F: FnMut([f64; 2]) -> f64,
{
    let nonzdelt = 0.05;
    let zdelt = 0.00025;

    // 初期シンプレックス (3頂点: [p0, p1])
    let mut sim = [[0.0; 2]; 3];
    sim[0] = x0;

    for k in 0..2 {
        let mut y = x0;
        if y[k].abs() > 1e-12 {
            y[k] = (1.0 + nonzdelt) * y[k];
        } else {
            y[k] = zdelt;
        }
        sim[k + 1] = y;
    }

    let mut fsim = [func(sim[0]), func(sim[1]), func(sim[2])];

    // パラメータ
    let rho = 1.0;   // reflection
    let chi = 2.0;   // expansion
    let psi = 0.5;   // contraction
    let sigma = 0.5; // shrink

    for _ in 0..maxiter {
        // ソート (昇順: 0が最善、2が最悪)
        let mut order = [0, 1, 2];
        if fsim[order[0]] > fsim[order[1]] {
            order.swap(0, 1);
        }
        if fsim[order[1]] > fsim[order[2]] {
            order.swap(1, 2);
        }
        if fsim[order[0]] > fsim[order[1]] {
            order.swap(0, 1);
        }

        let best = order[0];
        let worst = order[2];
        let second_worst = order[1];

        // 収束判定 (SciPy fmin 仕様)
        let max_f_diff = (fsim[worst] - fsim[best]).abs();
        let max_x_diff = (sim[worst][0] - sim[best][0])
            .abs()
            .max((sim[worst][1] - sim[best][1]).abs())
            .max((sim[second_worst][0] - sim[best][0]).abs())
            .max((sim[second_worst][1] - sim[best][1]).abs());

        if max_f_diff <= ftol && max_x_diff <= xtol {
            return sim[best];
        }

        // 最悪点以外の重心
        let xbar = [
            (sim[best][0] + sim[second_worst][0]) / 2.0,
            (sim[best][1] + sim[second_worst][1]) / 2.0,
        ];

        // 反射 (Reflection)
        let xr = [
            (1.0 + rho) * xbar[0] - rho * sim[worst][0],
            (1.0 + rho) * xbar[1] - rho * sim[worst][1],
        ];
        let fxr = func(xr);

        if fxr < fsim[best] {
            // 拡大 (Expansion)
            let xe = [
                (1.0 + rho * chi) * xbar[0] - rho * chi * sim[worst][0],
                (1.0 + rho * chi) * xbar[1] - rho * chi * sim[worst][1],
            ];
            let fxe = func(xe);
            if fxe < fxr {
                sim[worst] = xe;
                fsim[worst] = fxe;
            } else {
                sim[worst] = xr;
                fsim[worst] = fxr;
            }
        } else if fxr < fsim[second_worst] {
            sim[worst] = xr;
            fsim[worst] = fxr;
        } else {
            // 収縮 (Contraction)
            let mut perform_shrink = false;
            if fxr < fsim[worst] {
                // Outside contraction
                let xc = [
                    (1.0 + rho * psi) * xbar[0] - rho * psi * sim[worst][0],
                    (1.0 + rho * psi) * xbar[1] - rho * psi * sim[worst][1],
                ];
                let fxc = func(xc);
                if fxc <= fxr {
                    sim[worst] = xc;
                    fsim[worst] = fxc;
                } else {
                    perform_shrink = true;
                }
            } else {
                // Inside contraction
                let xcc = [
                    (1.0 - psi) * xbar[0] + psi * sim[worst][0],
                    (1.0 - psi) * xbar[1] + psi * sim[worst][1],
                ];
                let fxcc = func(xcc);
                if fxcc < fsim[worst] {
                    sim[worst] = xcc;
                    fsim[worst] = fxcc;
                } else {
                    perform_shrink = true;
                }
            }

            if perform_shrink {
                // 縮小 (Shrink)
                for k in [second_worst, worst] {
                    sim[k] = [
                        sim[best][0] + sigma * (sim[k][0] - sim[best][0]),
                        sim[best][1] + sigma * (sim[k][1] - sim[best][1]),
                    ];
                    fsim[k] = func(sim[k]);
                }
            }
        }
    }

    // 終了時の最善点を返す
    let mut best = 0;
    for i in 1..3 {
        if fsim[i] < fsim[best] {
            best = i;
        }
    }
    sim[best]
}

/// 未補正の複素数スペクトルに対して ACME 法を実行し、最適な (p0, p1) を算出する
pub fn autophase_acme(unphased_complex: &Array1<Complex64>) -> (f64, f64) {
    let mut initial_p0 = 0.0;
    let initial_score = acme_score(unphased_complex, 0.0, 0.0);

    // [0.0, 0.0] で全ペナルティ領域 (max_r <= 0 など) に入っている場合、
    // 粗い p0 グリッド走査で有効な初期位相を探索
    if initial_score >= 1e9 {
        let mut best_score = initial_score;
        for step in 1..12 {
            let p0_test = (step as f64) * 30.0;
            let score = acme_score(unphased_complex, p0_test, 0.0);
            if score < best_score {
                best_score = score;
                initial_p0 = p0_test;
            }
        }
    }

    let opt = nelder_mead_2d(
        |p| acme_score(unphased_complex, p[0], p[1]),
        [initial_p0, 0.0],
        1e-4,
        1e-4,
        500,
    );
    (opt[0], opt[1])
}

/// 未補正の複素数スペクトルに対して ACME 法を実行し、指定された pivot_k を基準とする最適な (p0, p1) を算出する
pub fn autophase_acme_with_pivot(unphased_complex: &Array1<Complex64>, pivot_k: usize) -> (f64, f64) {
    let (p0_zero, p1) = autophase_acme(unphased_complex);
    let n = unphased_complex.len();
    let p0_pivot = crate::core::signal::phase::convert_p0_for_new_pivot(p0_zero, p1, 0, pivot_k, n);
    (p0_pivot, p1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nelder_mead_quadratic() {
        // f(x, y) = (x - 3)^2 + (y + 4)^2
        let opt = nelder_mead_2d(
            |p| (p[0] - 3.0).powi(2) + (p[1] + 4.0).powi(2),
            [0.0, 0.0],
            1e-5,
            1e-5,
            200,
        );
        assert!((opt[0] - 3.0).abs() < 1e-3);
        assert!((opt[1] + 4.0).abs() < 1e-3);
    }
}
