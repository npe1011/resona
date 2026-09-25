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

/// 窓関数を複素数FIDに適用する
pub fn apply_window(
    fid: &Array1<Complex64>,
    window: &WindowFunction,
    sw_hz: f64,
) -> Array1<Complex64> {
    let curve = compute_window_curve(window, fid.len(), sw_hz);
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
}
