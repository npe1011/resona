use std::f64::consts::PI;
use ndarray::Array1;
use num_complex::Complex64;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WindowFunction {
    None,
    #[serde(rename = "em")]
    Exponential {
        #[serde(default = "default_lb")]
        lb: f64,
    },
    #[serde(rename = "gm")]
    Gaussian {
        #[serde(default)]
        g1: f64,
        #[serde(default)]
        g2: f64,
        #[serde(default)]
        g3: f64,
    },
}

fn default_lb() -> f64 {
    0.3
}

impl Default for WindowFunction {
    fn default() -> Self {
        WindowFunction::Exponential { lb: 0.3 }
    }
}

/// FID 末尾の線形減衰処理 (Trapezoid 台形窓の末尾減衰部)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrapezoidDecay {
    #[serde(default = "default_trapezoid_enabled")]
    pub enabled: bool,
    /// 減衰開始位置 (%) 例: 80.0
    #[serde(default = "default_trapezoid_start")]
    pub start_percent: f64,
}

fn default_trapezoid_enabled() -> bool {
    true
}

fn default_trapezoid_start() -> f64 {
    80.0
}

impl Default for TrapezoidDecay {
    fn default() -> Self {
        Self {
            enabled: true,
            start_percent: 80.0,
        }
    }
}

/// 窓関数曲線の計算 (0..N の各点に対するスカラー重み)
pub fn compute_window_curve(
    window: &WindowFunction,
    n: usize,
    sw_hz: f64,
) -> Array1<f64> {
    let mut curve = Array1::ones(n);
    if n == 0 || sw_hz <= 0.0 {
        return curve;
    }

    match window {
        WindowFunction::None => curve,
        WindowFunction::Exponential { lb } => {
            let lb_pts = lb / sw_hz;
            for i in 0..n {
                curve[i] = (-PI * (i as f64) * lb_pts).exp();
            }
            curve
        }
        WindowFunction::Gaussian { g1, g2, g3 } => {
            let g1_pts = g1 / sw_hz;
            let g2_pts = g2 / sw_hz;
            let n_f64 = n as f64;
            for i in 0..n {
                let i_f64 = i as f64;
                let e = PI * i_f64 * g1_pts;
                let g = 0.6 * PI * g2_pts * (g3 * (n_f64 - 1.0) - i_f64);
                curve[i] = (e - g * g).exp();
            }
            curve
        }
    }
}

/// 窓関数を複素数FIDに適用する (互換用: Trapezoid なし)
pub fn apply_window(
    fid: &Array1<Complex64>,
    window: &WindowFunction,
    sw_hz: f64,
) -> Array1<Complex64> {
    apply_window_with_trapezoid(
        fid,
        window,
        &TrapezoidDecay {
            enabled: false,
            start_percent: 80.0,
        },
        sw_hz,
    )
}

/// Trapezoid (末尾線形減衰) 曲線の計算 (0..N の各点に対するスカラー重み)
/// 開始位置 (start_percent: %、初期値 80%) から末尾 100% (点 N - 1) に向けて 1.0 -> 0.0 に線形減衰させる
pub fn compute_trapezoid_curve(trapezoid: &TrapezoidDecay, n: usize) -> Array1<f64> {
    let mut curve = Array1::ones(n);
    if !trapezoid.enabled || n < 2 {
        return curve;
    }

    let r_start = (trapezoid.start_percent / 100.0).clamp(0.0, 1.0);
    if r_start >= 1.0 {
        return curve;
    }

    let denom = 1.0 - r_start;
    let n_minus_1 = (n - 1) as f64;
    for i in 0..n {
        let t = (i as f64) / n_minus_1;
        if t >= r_start {
            curve[i] = ((1.0 - t) / denom).clamp(0.0, 1.0);
        }
    }
    curve
}

/// メイン窓関数と Trapezoid 末尾減衰の合成曲線 (0..N の各点に対する積)
pub fn compute_total_window_curve(
    window: &WindowFunction,
    trapezoid: &TrapezoidDecay,
    n: usize,
    sw_hz: f64,
) -> Array1<f64> {
    let mut curve = compute_window_curve(window, n, sw_hz);
    if trapezoid.enabled && n >= 2 {
        let trap = compute_trapezoid_curve(trapezoid, n);
        for i in 0..n {
            curve[i] *= trap[i];
        }
    }
    curve
}

/// 窓関数および Trapezoid 末尾減衰を複素数FIDに適用する
pub fn apply_window_with_trapezoid(
    fid: &Array1<Complex64>,
    window: &WindowFunction,
    trapezoid: &TrapezoidDecay,
    sw_hz: f64,
) -> Array1<Complex64> {
    let curve = compute_total_window_curve(window, trapezoid, fid.len(), sw_hz);
    let mut res = fid.clone();
    for i in 0..fid.len() {
        res[i] *= curve[i];
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_window() {
        let n = 100;
        let sw_hz = 1000.0;
        let win = WindowFunction::Exponential { lb: 1.0 };
        let curve = compute_window_curve(&win, n, sw_hz);
        assert_eq!(curve.len(), 100);
        assert!((curve[0] - 1.0).abs() < 1e-12);
        assert!(curve[99] < 1.0);
    }

    #[test]
    fn test_trapezoid_decay() {
        let n = 101;
        let trap = TrapezoidDecay {
            enabled: true,
            start_percent: 80.0,
        };
        let curve = compute_trapezoid_curve(&trap, n);
        assert_eq!(curve.len(), 101);
        // 0..=80 までは 1.0
        assert!((curve[0] - 1.0).abs() < 1e-12);
        assert!((curve[80] - 1.0).abs() < 1e-12);
        // 90 は中間 (0.5)
        assert!((curve[90] - 0.5).abs() < 1e-12);
        // 100 (末尾, 100%) は 0.0
        assert!((curve[100] - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_total_window_product() {
        let n = 101;
        let sw_hz = 1000.0;
        let win = WindowFunction::Exponential { lb: 1.0 };
        let trap = TrapezoidDecay {
            enabled: true,
            start_percent: 80.0,
        };
        let total = compute_total_window_curve(&win, &trap, n, sw_hz);
        let exp_only = compute_window_curve(&win, n, sw_hz);

        // 80% までは exp_only と一致
        assert!((total[0] - exp_only[0]).abs() < 1e-12);
        assert!((total[80] - exp_only[80]).abs() < 1e-12);
        // 末尾は 0.0
        assert!((total[100] - 0.0).abs() < 1e-12);
    }
}
