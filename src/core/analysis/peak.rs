use ndarray::Array1;
use serde::{Deserialize, Serialize};

/// 検出・登録されたピーク項目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeakItem {
    /// 化学シフト (ppm)
    pub ppm: f64,
    /// スペクトル強度
    pub intensity: f64,
    /// 自動検出されたピークか (手動追加の場合は false)
    #[serde(default = "default_true")]
    pub is_auto: bool,
}

fn default_true() -> bool {
    true
}

/// 中央値絶対偏差 (MAD: Median Absolute Deviation) による頑健なノイズレベル推定
pub fn estimate_noise_mad(spectrum: &Array1<f64>) -> f64 {
    let n = spectrum.len();
    if n == 0 {
        return 1.0;
    }

    let mut vals: Vec<f64> = spectrum.to_vec();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = vals[n / 2];

    let mut abs_diffs: Vec<f64> = spectrum.iter().map(|&v| (v - median).abs()).collect();
    abs_diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mad = abs_diffs[n / 2];

    if mad > 0.0 {
        mad / 0.6745
    } else {
        1.0
    }
}

/// 単一ピークのプロミネンス (突出度) を計算する (SciPy peak_prominences 準拠)
fn calculate_prominence(signal: &[f64], peak_idx: usize) -> f64 {
    let n = signal.len();
    let peak_val = signal[peak_idx];

    // 左方向の探索
    let mut left_min = peak_val;
    for j in (0..peak_idx).rev() {
        if signal[j] < left_min {
            left_min = signal[j];
        }
        if signal[j] > peak_val {
            break;
        }
    }

    // 右方向の探索
    let mut right_min = peak_val;
    for k in (peak_idx + 1)..n {
        if signal[k] < right_min {
            right_min = signal[k];
        }
        if signal[k] > peak_val {
            break;
        }
    }

    let base = left_min.max(right_min);
    (peak_val - base).max(0.0)
}

/// ピーク検出の内部ロジック (正または負の信号に対する極大値検出)
fn find_peaks_1d(
    signal: &[f64],
    threshold: f64,
    prominence_thresh: f64,
    min_distance: usize,
) -> Vec<usize> {
    let n = signal.len();
    if n < 3 {
        return Vec::new();
    }

    let mut candidate_indices = Vec::new();

    for i in 1..n - 1 {
        // 極大値かつ閾値以上
        if signal[i] > signal[i - 1] && signal[i] > signal[i + 1] && signal[i] >= threshold {
            let prom = calculate_prominence(signal, i);
            if prom >= prominence_thresh {
                candidate_indices.push(i);
            }
        }
    }

    if min_distance <= 1 || candidate_indices.is_empty() {
        return candidate_indices;
    }

    // 最小間隔フィルタリング (高いピークを優先)
    candidate_indices.sort_by(|&a, &b| {
        signal[b]
            .partial_cmp(&signal[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut kept = Vec::new();
    for &idx in &candidate_indices {
        let mut ok = true;
        for &k in &kept {
            if (idx as isize - k as isize).unsigned_abs() < min_distance {
                ok = false;
                break;
            }
        }
        if ok {
            kept.push(idx);
        }
    }

    kept.sort_unstable();
    kept
}

/// スペクトル全体から自動でピークピッキングを行う。
/// 手動追加ピーク (is_auto == false) は保持され、自動ピークのみが再計算・置換される。
pub fn pick_peaks(
    spectrum: &Array1<f64>,
    ppm: &Array1<f64>,
    threshold: f64,
    existing_peaks: &[PeakItem],
) -> Vec<PeakItem> {
    let n = spectrum.len();
    if n != ppm.len() || n == 0 {
        return existing_peaks.to_vec();
    }

    let noise = estimate_noise_mad(spectrum);
    let prominence_thresh = (noise * 2.5).max(threshold * 0.05);

    // 1. 正のピーク
    let s_slice = spectrum.as_slice().unwrap_or(&[]);
    let pos_peaks = find_peaks_1d(s_slice, threshold, prominence_thresh, 5);

    // 2. 負のピーク (-spectrum)
    let neg_signal: Vec<f64> = spectrum.iter().map(|&v| -v).collect();
    let neg_peaks = find_peaks_1d(&neg_signal, threshold, prominence_thresh, 5);

    let mut auto_indices: Vec<usize> = pos_peaks;
    for idx in neg_peaks {
        if !auto_indices.contains(&idx) {
            auto_indices.push(idx);
        }
    }
    auto_indices.sort_unstable();

    // 3. 手動ピーク (is_auto == false) を保持
    let mut result: Vec<PeakItem> = existing_peaks
        .iter()
        .filter(|p| !p.is_auto)
        .cloned()
        .collect();

    // 4. 新たな自動ピークを追加
    for idx in auto_indices {
        result.push(PeakItem {
            ppm: ppm[idx],
            intensity: spectrum[idx],
            is_auto: true,
        });
    }

    // 5. PPM の降順 (左から右) でソート
    result.sort_by(|a, b| {
        b.ppm
            .partial_cmp(&a.ppm)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    result
}

/// 単一ピークを手動追加する (最寄り極大値にスナップ)
pub fn snap_and_add_peak(
    spectrum: &Array1<f64>,
    ppm: &Array1<f64>,
    target_ppm: f64,
    existing_peaks: &[PeakItem],
) -> Vec<PeakItem> {
    let n = spectrum.len();
    if n == 0 || n != ppm.len() {
        return existing_peaks.to_vec();
    }

    // 最寄りのインデックス
    let mut closest_idx = 0;
    let mut min_diff = f64::INFINITY;
    for (i, &p) in ppm.iter().enumerate() {
        let diff = (p - target_ppm).abs();
        if diff < min_diff {
            min_diff = diff;
            closest_idx = i;
        }
    }

    // ±10点の範囲で極値を探索
    let start = closest_idx.saturating_sub(10);
    let end = (closest_idx + 10).min(n - 1);

    let mut best_idx = closest_idx;
    let mut best_abs = spectrum[closest_idx].abs();

    for i in start..=end {
        if spectrum[i].abs() > best_abs {
            best_abs = spectrum[i].abs();
            best_idx = i;
        }
    }

    let mut result = existing_peaks.to_vec();
    result.push(PeakItem {
        ppm: ppm[best_idx],
        intensity: spectrum[best_idx],
        is_auto: false, // 手動追加
    });

    result.sort_by(|a, b| {
        b.ppm
            .partial_cmp(&a.ppm)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_estimation() {
        let data = Array1::from_vec(vec![0.0, 0.1, -0.1, 0.05, -0.05, 100.0]); // 100 is outlier peak
        let noise = estimate_noise_mad(&data);
        assert!(noise < 1.0); // MAD ignores outlier 100.0
    }
}
