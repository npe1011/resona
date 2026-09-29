use ndarray::Array1;
use serde::{Deserialize, Serialize};

use super::peak::estimate_noise_mad;
use super::sensitivity::AutoSensitivity;

/// 積分区間項目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntegrationItem {
    /// 一意な識別子 (UUID または連番)
    pub id: String,
    /// 開始化学シフト (ppm, 通常は大きい値 / 低磁場側)
    pub start_ppm: f64,
    /// 終了化学シフト (ppm, 通常は小さい値 / 高磁場側)
    pub end_ppm: f64,
    /// 開始点での局所ベースライン Y レベル
    #[serde(default)]
    pub y_start: f64,
    /// 終了点での局所ベースライン Y レベル
    #[serde(default)]
    pub y_end: f64,
}

impl IntegrationItem {
    pub fn min_ppm(&self) -> f64 {
        self.start_ppm.min(self.end_ppm)
    }

    pub fn max_ppm(&self) -> f64 {
        self.start_ppm.max(self.end_ppm)
    }

    pub fn contains_ppm(&self, ppm: f64) -> bool {
        let min = self.min_ppm();
        let max = self.max_ppm();
        ppm >= min && ppm <= max
    }

    /// 指定した PPM における線形局所ベースラインの Y 値を補間計算
    pub fn baseline_y_at(&self, ppm: f64) -> f64 {
        let (x1, x2) = (self.start_ppm, self.end_ppm);
        let (y1, y2) = (self.y_start, self.y_end);
        if (x2 - x1).abs() > 1e-12 {
            y1 + (y2 - y1) / (x2 - x1) * (ppm - x1)
        } else {
            y1
        }
    }
}

/// 単一の積分区間に対する面積および累積積分曲線を計算する
#[derive(Debug, Clone)]
pub struct IntegralResult {
    /// 区間内の PPM 配列
    pub ppm: Vec<f64>,
    /// 表示用の積分曲線 Y 座標 (局所ベースライン + スケーリング積分 + オフセット)
    pub curve_y: Vec<f64>,
    /// 台形公式による総面積
    pub total_area: f64,
    /// リファレンス値換算後のプロトン数 / 表示値
    pub normalized_value: f64,
}

/// 単一区間の台形公式積分を計算する
pub fn compute_integral(
    spectrum: &Array1<f64>,
    ppm: &Array1<f64>,
    item: &IntegrationItem,
    global_scale: f64,
    ref_factor: f64,
    offset_factor: f64,
) -> Option<IntegralResult> {
    let n = spectrum.len();
    if n == 0 || n != ppm.len() {
        return None;
    }

    let mut x1 = item.start_ppm;
    let mut x2 = item.end_ppm;
    let mut y1 = item.y_start;
    let mut y2 = item.y_end;

    if x1 < x2 {
        std::mem::swap(&mut x1, &mut x2);
        std::mem::swap(&mut y1, &mut y2);
    }

    // インデックス探索
    let mut idx1 = 0;
    let mut min_d1 = f64::INFINITY;
    let mut idx2 = 0;
    let mut min_d2 = f64::INFINITY;

    for (i, &p) in ppm.iter().enumerate() {
        let d1 = (p - x1).abs();
        if d1 < min_d1 {
            min_d1 = d1;
            idx1 = i;
        }
        let d2 = (p - x2).abs();
        if d2 < min_d2 {
            min_d2 = d2;
            idx2 = i;
        }
    }

    if idx1 > idx2 {
        std::mem::swap(&mut idx1, &mut idx2);
    }

    if idx2 - idx1 < 2 {
        return None;
    }

    let region_ppm = &ppm.as_slice()?[idx1..=idx2];
    let region_spec = &spectrum.as_slice()?[idx1..=idx2];
    let m_points = region_ppm.len();

    // 局所線形ベースライン: y_bl = m * ppm + c
    let (m, c) = if (x2 - x1).abs() > 1e-12 {
        let slope = (y2 - y1) / (x2 - x1);
        let intercept = y1 - slope * x1;
        (slope, intercept)
    } else {
        (0.0, y1)
    };

    // 台形公式による累積積分
    let mut integral = vec![0.0; m_points];
    let mut total_area = 0.0;

    for i in 0..m_points - 1 {
        let local_bl_a = m * region_ppm[i] + c;
        let local_bl_b = m * region_ppm[i + 1] + c;

        let corr_a = region_spec[i] - local_bl_a;
        let corr_b = region_spec[i + 1] - local_bl_b;

        let dx = (region_ppm[i] - region_ppm[i + 1]).abs();
        let da = (corr_a + corr_b) / 2.0 * dx;

        total_area += da;
        integral[i + 1] = total_area;
    }

    // 表示用曲線の生成: y_bl + integral * scale + offset
    let offset = if offset_factor.abs() > 1e-12 {
        let max_spec = spectrum.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        max_spec * offset_factor
    } else {
        0.0
    };

    let mut curve_y = Vec::with_capacity(m_points);
    for i in 0..m_points {
        let local_bl = m * region_ppm[i] + c;
        curve_y.push(local_bl + integral[i] * global_scale + offset);
    }

    Some(IntegralResult {
        ppm: region_ppm.to_vec(),
        curve_y,
        total_area,
        normalized_value: total_area * ref_factor,
    })
}

/// スペクトル全体から自動で有意なピーク領域を検出し、積分区間リストを生成する
pub fn auto_detect_integrations(
    spectrum: &Array1<f64>,
    ppm: &Array1<f64>,
    sensitivity: AutoSensitivity,
) -> Vec<IntegrationItem> {
    let n = spectrum.len();
    if n < 10 || n != ppm.len() {
        return Vec::new();
    }

    let noise = estimate_noise_mad(spectrum);
    let mut vals: Vec<f64> = spectrum.to_vec();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = vals[n / 2];

    let thresh_low = median + 0.5 * noise;
    let thresh_high = median + sensitivity.integral_noise_factor() * noise;

    let mut items = Vec::new();
    let mut in_region = false;
    let mut start_idx = 0;
    let mut has_high_peak = false;

    for i in 0..n {
        if spectrum[i] > thresh_low {
            if !in_region {
                in_region = true;
                start_idx = i;
                has_high_peak = false;
            }
            if spectrum[i] > thresh_high {
                has_high_peak = true;
            }
        } else if in_region {
            in_region = false;
            let end_idx = i.saturating_sub(1);
            // 十分な高さのピークを含み、かつ一定の幅（5点以上）を持つ領域
            if has_high_peak && end_idx > start_idx + 5 {
                // 両端を少し広げる (マージン)
                let s_margin = start_idx.saturating_sub(5);
                let e_margin = (end_idx + 5).min(n - 1);

                let id = format!("intg-{}", items.len() + 1);
                items.push(IntegrationItem {
                    id,
                    start_ppm: ppm[s_margin],
                    end_ppm: ppm[e_margin],
                    y_start: median,
                    y_end: median,
                });
            }
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_detect_integrations_with_sensitivity() {
        let n = 300;
        let mut spec = vec![0.0; n];
        let mut ppm_vec = vec![0.0; n];
        for i in 0..n {
            ppm_vec[i] = 10.0 - (i as f64) * 0.05;
        }

        // ピーク1: 強いピーク (中心 80, 高さ 100.0)
        for i in 60..=100 {
            let dx = (i as f64 - 80.0) / 4.0;
            spec[i] += 100.0 * (-0.5 * dx * dx).exp();
        }

        // ピーク2: 小中ピーク (中心 200, 高さ 8.0)
        // ノイズ level ≈ 0.37 なので S/N ≈ 21.6 (Highの15倍は超えるがMiddleの30倍は下回る)
        for i in 180..=220 {
            let dx = (i as f64 - 200.0) / 4.0;
            spec[i] += 8.0 * (-0.5 * dx * dx).exp();
        }

        // 模擬ノイズ (振幅 ±0.5, MAD ≈ 0.74, noise ≈ 1.0)
        for i in 0..n {
            let pseudo_noise = ((i * 17 + 3) % 11) as f64 / 10.0 - 0.5;
            spec[i] += pseudo_noise;
        }

        let spectrum = Array1::from_vec(spec);
        let ppm = Array1::from_vec(ppm_vec);

        // High: 閾値 ~15.0 * noise => 高さ 22.0 も拾う (2区間)
        let intgs_high = auto_detect_integrations(&spectrum, &ppm, AutoSensitivity::High);
        assert_eq!(intgs_high.len(), 2, "High sensitivity should detect both peaks");

        // Middle: 閾値 ~30.0 * noise => 高さ 100.0 のみ拾う (1区間)
        let intgs_mid = auto_detect_integrations(&spectrum, &ppm, AutoSensitivity::Middle);
        assert_eq!(intgs_mid.len(), 1, "Middle sensitivity should only detect the major peak");

        // Low: 閾値 ~70.0 * noise => 高さ 100.0 のみ拾う (1区間)
        let intgs_low = auto_detect_integrations(&spectrum, &ppm, AutoSensitivity::Low);
        assert_eq!(intgs_low.len(), 1, "Low sensitivity should only detect the major peak");
    }

    #[test]
    fn test_compute_integral_negative_peak() {
        let n = 100;
        let mut spec = vec![0.0; n];
        let mut ppm_vec = vec![0.0; n];
        for i in 0..n {
            ppm_vec[i] = 5.0 - (i as f64) * 0.05;
        }

        // 負のピーク (反転シグナル): 中心 50, 高さ -20.0
        for i in 40..=60 {
            let dx = (i as f64 - 50.0) / 3.0;
            spec[i] = -20.0 * (-0.5 * dx * dx).exp();
        }

        let spectrum = Array1::from_vec(spec);
        let ppm = Array1::from_vec(ppm_vec);

        let item = IntegrationItem {
            id: "intg-neg".to_string(),
            start_ppm: 3.5,
            end_ppm: 1.5,
            y_start: 0.0,
            y_end: 0.0,
        };

        let res = compute_integral(&spectrum, &ppm, &item, 1.0, 1.0, 0.0).expect("Integration should succeed");
        assert!(res.total_area < 0.0, "Negative peak should have negative total_area, got {}", res.total_area);
        assert!(res.normalized_value < 0.0, "Negative peak should have negative normalized_value, got {}", res.normalized_value);
    }
}
