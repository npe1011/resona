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

    let mut i = 1;
    while i < n - 1 {
        // 極大値かつ閾値以上 (平坦な山頂もサポート)
        if signal[i] >= threshold && signal[i] > signal[i - 1] {
            let mut j = i;
            while j < n - 1 && (signal[j + 1] - signal[i]).abs() < 1e-12 {
                j += 1;
            }
            if j < n - 1 && signal[j] > signal[j + 1] {
                // [i..=j] が極大プラトー。中央を極大値インデックスとする
                let peak_idx = (i + j) / 2;
                let prom = calculate_prominence(signal, peak_idx);
                if prom >= prominence_thresh {
                    candidate_indices.push(peak_idx);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
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
    // 肩ピークや微小な近接ピークも逃さず拾えるよう、プロミネンス基準をノイズレベルの0.3倍（30%）とし、
    // 閾値依存の項を極小（0.001倍）に設定
    let prominence_thresh = (noise * 0.3).max(threshold * 0.001).max(1e-6);

    // 1. 正のピーク (min_distance = 2 で近接肩ピークを保持)
    let temp_spec;
    let s_slice = match spectrum.as_slice() {
        Some(s) => s,
        None => {
            temp_spec = spectrum.to_vec();
            &temp_spec
        }
    };
    let pos_peaks = find_peaks_1d(s_slice, threshold, prominence_thresh, 2);

    // 2. 負のピーク (-spectrum)
    let neg_signal: Vec<f64> = spectrum.iter().map(|&v| -v).collect();
    let neg_peaks = find_peaks_1d(&neg_signal, threshold, prominence_thresh, 2);

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

/// 指定した PPM 範囲 [p_low, p_high] 内で最適なピークを探索して追加する。
/// - ドラッグ範囲外の近傍ピークを拾わない。
/// - すでに登録済みのピークと重複（近接）するものは再度拾わない。
/// - 範囲内に未登録の極大値があればそれを優先し、なければ範囲内の未登録最大値を採用する。
pub fn add_peak_in_range(
    spectrum: &Array1<f64>,
    ppm: &Array1<f64>,
    p_low: f64,
    p_high: f64,
    existing_peaks: &[PeakItem],
) -> Vec<PeakItem> {
    let n = spectrum.len();
    if n == 0 || n != ppm.len() {
        return existing_peaks.to_vec();
    }

    // 1. [p_low, p_high] に含まれるインデックスを抽出
    let mut range_indices: Vec<usize> = Vec::new();
    for (i, &p) in ppm.iter().enumerate() {
        if p >= p_low && p <= p_high {
            range_indices.push(i);
        }
    }

    // 範囲内の点が取れなかった場合（クリック等で微小区間の場合）は、最も近い1点
    if range_indices.is_empty() {
        let center_p = (p_low + p_high) * 0.5;
        let mut closest = 0;
        let mut min_d = f64::INFINITY;
        for (i, &p) in ppm.iter().enumerate() {
            let d = (p - center_p).abs();
            if d < min_d {
                min_d = d;
                closest = i;
            }
        }
        range_indices.push(closest);
    }

    // 2. 重複判定の許容差 (PPM)
    let dt_ppm = if n > 1 {
        (ppm[0] - ppm[n - 1]).abs() / (n - 1) as f64
    } else {
        0.001
    };
    let tol_ppm = dt_ppm * 2.0;

    // 既存ピークと重複しない（未登録の）インデックスを抽出
    let available_indices: Vec<usize> = range_indices
        .into_iter()
        .filter(|&idx| {
            let p_val = ppm[idx];
            existing_peaks
                .iter()
                .all(|ep| (ep.ppm - p_val).abs() > tol_ppm)
        })
        .collect();

    // 範囲内のピークが既にすべて拾われている場合は何もしない (重複追加防止)
    if available_indices.is_empty() {
        return existing_peaks.to_vec();
    }

    // 3. available_indices の中で候補を選択
    // まず未登録の局所極大値 (山) を探す
    let mut local_maxima: Vec<usize> = Vec::new();
    for &idx in &available_indices {
        let v = spectrum[idx];
        let v_prev = if idx > 0 { spectrum[idx - 1] } else { f64::NEG_INFINITY };
        let v_next = if idx + 1 < n { spectrum[idx + 1] } else { f64::NEG_INFINITY };
        if v > v_prev && v > v_next {
            local_maxima.push(idx);
        }
    }

    let best_idx = if !local_maxima.is_empty() {
        // 極大値がある場合: 最も強度の大きい極大値を選ぶ
        local_maxima
            .into_iter()
            .max_by(|&a, &b| {
                spectrum[a]
                    .abs()
                    .partial_cmp(&spectrum[b].abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap()
    } else {
        // 極大値がない場合 (斜面や単調傾斜): 範囲内で最も強度の大きい点を選ぶ
        available_indices
            .into_iter()
            .max_by(|&a, &b| {
                spectrum[a]
                    .abs()
                    .partial_cmp(&spectrum[b].abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap()
    };

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

    let dt_ppm = if n > 1 {
        (ppm[0] - ppm[n - 1]).abs() / (n - 1) as f64
    } else {
        0.001
    };
    let tol_ppm = dt_ppm * 2.0;

    // すでにターゲット位置にピークが存在する場合は重複して追加しない
    if existing_peaks.iter().any(|ep| (ep.ppm - target_ppm).abs() <= tol_ppm) {
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

    // ごく近傍 (±3点) の範囲で極値を探索 (広範囲のスナップで遠くの巨大ピークを拾うのを防止)
    let start = closest_idx.saturating_sub(3);
    let end = (closest_idx + 3).min(n - 1);

    let mut best_idx = closest_idx;
    let mut best_abs = spectrum[closest_idx].abs();

    for i in start..=end {
        if spectrum[i].abs() > best_abs {
            best_abs = spectrum[i].abs();
            best_idx = i;
        }
    }

    // スナップした結果が既存ピークと重複しないか再確認
    if existing_peaks.iter().any(|ep| (ep.ppm - ppm[best_idx]).abs() <= tol_ppm) {
        return existing_peaks.to_vec();
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

    #[test]
    fn test_shoulder_peak_detection() {
        // 親ピーク(100.0)のすぐ隣にある肩ピーク(30.0)
        let s = vec![0.0, 5.0, 100.0, 20.0, 30.0, 10.0, 0.0];
        let p = vec![6.0, 5.0, 4.0, 3.0, 2.0, 1.0, 0.0];
        let spec = Array1::from_vec(s);
        let ppm = Array1::from_vec(p);

        let peaks = pick_peaks(&spec, &ppm, 15.0, &[]);
        // 親ピーク (4.0 ppm) と肩ピーク (2.0 ppm) の両方が拾われること
        assert_eq!(peaks.len(), 2);
        assert!((peaks[0].ppm - 4.0).abs() < 1e-5);
        assert!((peaks[1].ppm - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_add_peak_in_range_no_out_of_bounds_snap() {
        // インデックス 1 に巨大ピーク (100.0)、インデックス 4 に小さなコブ (15.0)
        let s = vec![0.0, 100.0, 10.0, 12.0, 15.0, 8.0, 0.0];
        let p = vec![6.0, 5.0, 4.0, 3.0, 2.0, 1.0, 0.0];
        let spec = Array1::from_vec(s);
        let ppm = Array1::from_vec(p);

        // 巨大ピーク(5.0 ppm)は既存ピークとする
        let existing = vec![PeakItem { ppm: 5.0, intensity: 100.0, is_auto: true }];

        // コブのある範囲 [1.5, 3.5] ppm をドラッグ
        let result = add_peak_in_range(&spec, &ppm, 1.5, 3.5, &existing);
        assert_eq!(result.len(), 2);
        // 新しく追加されたピークは 2.0 ppm (強度15.0) であり、範囲外の巨大ピーク (5.0 ppm) にスナップしない
        assert!((result[1].ppm - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_add_peak_duplicate_prevention() {
        let s = vec![0.0, 50.0, 0.0];
        let p = vec![2.0, 1.0, 0.0];
        let spec = Array1::from_vec(s);
        let ppm = Array1::from_vec(p);

        let existing = vec![PeakItem { ppm: 1.0, intensity: 50.0, is_auto: false }];

        // 既に存在する 1.0 ppm の範囲 [0.5, 1.5] を再度ドラッグして Add しても重複追加されない
        let result = add_peak_in_range(&spec, &ppm, 0.5, 1.5, &existing);
        assert_eq!(result.len(), 1);
    }
}
