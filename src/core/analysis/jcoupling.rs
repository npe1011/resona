use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// J-coupling 解析候補
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JCouplingCandidate {
    pub pattern: String,
    pub j_vals: Vec<f64>,
    pub error: f64,
    pub text: String,
}

/// J-coupling 結果テーブルに保存される項目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JCouplingResultItem {
    pub text: String,
    pub ppm: f64,
}

struct PatternDef {
    n_spaces: usize,
    intensities: &'static [f64],
}

fn get_pattern_def(name: &str) -> Option<PatternDef> {
    match name {
        "s" => Some(PatternDef { n_spaces: 0, intensities: &[1.0] }),
        "d" => Some(PatternDef { n_spaces: 1, intensities: &[1.0, 1.0] }),
        "t" => Some(PatternDef { n_spaces: 2, intensities: &[1.0, 2.0, 1.0] }),
        "q" => Some(PatternDef { n_spaces: 3, intensities: &[1.0, 3.0, 3.0, 1.0] }),
        "quint" => Some(PatternDef { n_spaces: 4, intensities: &[1.0, 4.0, 6.0, 4.0, 1.0] }),
        "sext" => Some(PatternDef { n_spaces: 5, intensities: &[1.0, 5.0, 10.0, 10.0, 5.0, 1.0] }),
        "sept" => Some(PatternDef { n_spaces: 6, intensities: &[1.0, 6.0, 15.0, 20.0, 15.0, 6.0, 1.0] }),
        _ => None,
    }
}

/// 結合定数 J 群から理論的な分裂ピーク木 (周波数オフセット -> 相対強度) を生成する
fn generate_tree(parts: &[&str], j_vals: &[f64]) -> BTreeMap<i64, f64> {
    // 浮動小数点数キーの比較を安定させるため、0.001 Hz 単位の整数(i64)でキー管理
    let scale = 1000.0;
    let mut peaks = BTreeMap::new();
    peaks.insert(0i64, 1.0);

    for (&part, &j) in parts.iter().zip(j_vals.iter()) {
        let def = match get_pattern_def(part) {
            Some(d) => d,
            None => continue,
        };

        let mut new_peaks = BTreeMap::new();
        let start = -((def.n_spaces as f64) * j) / 2.0;

        for (i, &intensity) in def.intensities.iter().enumerate() {
            let pos = start + (i as f64) * j;
            let pos_int = (pos * scale).round() as i64;

            for (&p_pos, &p_int) in &peaks {
                let np_pos = p_pos + pos_int;
                *new_peaks.entry(np_pos).or_insert(0.0) += p_int * intensity;
            }
        }
        peaks = new_peaks;
    }

    peaks
}

/// 候補パターンと J 値から、中心からの相対オフセット (Hz) と相対強度のペア一覧を生成する
pub fn generate_theoretical_peaks(pattern: &str, j_vals: &[f64]) -> Vec<(f64, f64)> {
    if pattern == "s" {
        return vec![(0.0, 1.0)];
    }
    if pattern == "m" || j_vals.is_empty() {
        return Vec::new();
    }

    let tree = match pattern {
        "d" | "t" | "q" | "quint" | "sext" | "sept" => generate_tree(&[pattern], j_vals),
        "dd" => generate_tree(&["d", "d"], j_vals),
        "dt" => generate_tree(&["d", "t"], j_vals),
        "td" => generate_tree(&["t", "d"], j_vals),
        "dq" => generate_tree(&["d", "q"], j_vals),
        "qd" => generate_tree(&["q", "d"], j_vals),
        "tt" => generate_tree(&["t", "t"], j_vals),
        "ddd" => generate_tree(&["d", "d", "d"], j_vals),
        _ => return Vec::new(),
    };

    let scale = 1000.0;
    let mut res: Vec<(f64, f64)> = tree
        .into_iter()
        .map(|(pos_int, val)| ((pos_int as f64) / scale, val))
        .collect();

    let max_val = res.iter().map(|p| p.1).fold(0.0_f64, f64::max);
    if max_val > 0.0 {
        for p in &mut res {
            p.1 /= max_val;
        }
    }

    res
}

/// 理論ピーク群と半値幅 (FWHM, Hz) から、周波数オフセット (Hz) におけるローレンツ和強度を算出する
pub fn evaluate_lorentzian_sum(peaks: &[(f64, f64)], offset_hz: f64, fwhm_hz: f64) -> f64 {
    let gamma = (fwhm_hz * 0.5).max(1e-6);
    let mut sum = 0.0;
    for &(p_hz, weight) in peaks {
        let diff = (offset_hz - p_hz) / gamma;
        sum += weight / (1.0 + diff * diff);
    }
    sum
}

/// 理論ピークと観測ピークの一致スコアを計算
fn score_model(
    theo_peaks: &BTreeMap<i64, f64>,
    center_hz: f64,
    obs_peaks_hz: &[f64],
    obs_intensities: &[f64],
    tolerance: f64,
) -> Option<f64> {
    let scale = 1000.0;
    let n_obs = obs_peaks_hz.len();

    let mut theo_pos = Vec::new();
    let mut theo_int = Vec::new();

    let mut max_theo_int = 0.0f64;
    for (&pos_int, &val) in theo_peaks {
        let p_hz = (pos_int as f64) / scale + center_hz;
        theo_pos.push(p_hz);
        theo_int.push(val);
        if val > max_theo_int {
            max_theo_int = val;
        }
    }

    if max_theo_int <= 0.0 {
        return None;
    }
    for val in &mut theo_int {
        *val /= max_theo_int;
    }

    // 許容誤差以内の理論ピークをマージ
    let mut merged_pos = Vec::new();
    let mut merged_int = Vec::new();

    let mut curr_p = vec![theo_pos[0]];
    let mut curr_i = vec![theo_int[0]];

    for k in 1..theo_pos.len() {
        let mean_p: f64 = curr_p.iter().sum::<f64>() / (curr_p.len() as f64);
        if (theo_pos[k] - mean_p).abs() <= tolerance {
            curr_p.push(theo_pos[k]);
            curr_i.push(theo_int[k]);
        } else {
            merged_pos.push(curr_p.iter().sum::<f64>() / (curr_p.len() as f64));
            merged_int.push(curr_i.iter().sum::<f64>());
            curr_p = vec![theo_pos[k]];
            curr_i = vec![theo_int[k]];
        }
    }
    merged_pos.push(curr_p.iter().sum::<f64>() / (curr_p.len() as f64));
    merged_int.push(curr_i.iter().sum::<f64>());

    if merged_pos.len() != n_obs {
        return None;
    }

    // 位置平均誤差
    let mut pos_err_sum = 0.0;
    for i in 0..n_obs {
        pos_err_sum += (merged_pos[i] - obs_peaks_hz[i]).abs();
    }
    let pos_err = pos_err_sum / (n_obs as f64);

    if pos_err > tolerance {
        return None;
    }

    // 強度平均誤差
    let max_merged_int = merged_int.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if max_merged_int > 0.0 {
        for v in &mut merged_int {
            *v /= max_merged_int;
        }
    }

    let mut int_err_sum = 0.0;
    for i in 0..n_obs {
        int_err_sum += (merged_int[i] - obs_intensities[i]).abs();
    }
    let int_err = int_err_sum / (n_obs as f64);

    Some(pos_err + 0.1 * int_err)
}

/// 指定範囲内のピーク群から J-coupling 多重線を自動解析する
pub fn analyze_multiplet(
    peaks_hz: Vec<f64>,
    intensities: Vec<f64>,
    shift_str: &str,
    shift_str_m: &str,
    nuclei_str: &str,
    tolerance: f64,
) -> Vec<JCouplingCandidate> {
    let n = peaks_hz.len();
    if n == 0 {
        return Vec::new();
    }

    // 昇順ソート
    let mut combined: Vec<(f64, f64)> = peaks_hz.into_iter().zip(intensities).collect();
    combined.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let peaks_hz: Vec<f64> = combined.iter().map(|p| p.0).collect();
    let mut intensities: Vec<f64> = combined.iter().map(|p| p.1).collect();

    let max_int = intensities.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if max_int > 0.0 {
        for v in &mut intensities {
            *v /= max_int;
        }
    }

    let width = if n > 1 { peaks_hz[n - 1] - peaks_hz[0] } else { 0.0 };
    let center = if n > 1 { (peaks_hz[n - 1] + peaks_hz[0]) / 2.0 } else { peaks_hz[0] };

    let mut candidates: Vec<JCouplingCandidate> = Vec::new();

    if n == 1 {
        candidates.push(JCouplingCandidate {
            pattern: "s".to_string(),
            j_vals: Vec::new(),
            error: 0.0,
            text: format!("{} (s, {})", shift_str, nuclei_str),
        });
    } else {
        // 1. 1次多重線
        for pat in &["d", "t", "q", "quint", "sext", "sept"] {
            if let Some(def) = get_pattern_def(pat) {
                if def.n_spaces > 0 {
                    let j = width / (def.n_spaces as f64);
                    let tree = generate_tree(&[pat], &[j]);
                    if let Some(err) = score_model(&tree, center, &peaks_hz, &intensities, tolerance) {
                        candidates.push(JCouplingCandidate {
                            pattern: pat.to_string(),
                            j_vals: vec![j],
                            error: err,
                            text: String::new(),
                        });
                    }
                }
            }
        }

        // 2. 2次多重線 (dd, dt, td, dq, qd, tt)
        let pairs = [
            ("dd", "d", "d"),
            ("dt", "d", "t"),
            ("td", "t", "d"),
            ("dq", "d", "q"),
            ("qd", "q", "d"),
            ("tt", "t", "t"),
        ];

        for (name, p1, p2) in pairs {
            let n1 = get_pattern_def(p1).unwrap().n_spaces as f64;
            let n2 = get_pattern_def(p2).unwrap().n_spaces as f64;

            let mut best_err = 999.0;
            let mut best_j = None;

            // J1 を 100 分割スキャン
            let j1_min = width / (n1 + n2);
            let j1_max = width / n1;
            let steps = 100;

            for step in 0..=steps {
                let j1 = j1_min + (j1_max - j1_min) * (step as f64) / (steps as f64);
                let j2 = (width - n1 * j1) / n2;

                if j2 < 0.0 || j1 < j2 {
                    continue;
                }

                let tree = generate_tree(&[p1, p2], &[j1, j2]);
                if let Some(err) = score_model(&tree, center, &peaks_hz, &intensities, tolerance) {
                    if err < best_err {
                        best_err = err;
                        best_j = Some(vec![j1, j2]);
                    }
                }
            }

            if let Some(j) = best_j {
                candidates.push(JCouplingCandidate {
                    pattern: name.to_string(),
                    j_vals: j,
                    error: best_err,
                    text: String::new(),
                });
            }
        }

        // 3. 3次多重線 (ddd) - ピーク数が 7 または 8 の条件付き探索
        if n == 7 || n == 8 {
            let mut best_err = 999.0;
            let mut best_j = None;

            let j1_min = width / 3.0;
            let j1_max = (width - 0.2).max(j1_min);
            let steps_j1 = 50;

            for s1 in 0..=steps_j1 {
                let j1 = j1_min + (j1_max - j1_min) * (s1 as f64) / (steps_j1 as f64);
                let rem = width - j1;
                if rem <= 0.0 {
                    continue;
                }

                let j2_min = rem / 2.0;
                let j2_max = j1.min(rem - 0.1);
                if j2_max < j2_min {
                    continue;
                }

                let steps_j2 = 30;
                for s2 in 0..=steps_j2 {
                    let j2 = j2_min + (j2_max - j2_min) * (s2 as f64) / (steps_j2 as f64);
                    let j3 = rem - j2;
                    if j3 <= 0.0 || j2 < j3 || j1 < j2 {
                        continue;
                    }

                    let tree = generate_tree(&["d", "d", "d"], &[j1, j2, j3]);
                    if let Some(err) = score_model(&tree, center, &peaks_hz, &intensities, tolerance) {
                        if err < best_err {
                            best_err = err;
                            best_j = Some(vec![j1, j2, j3]);
                        }
                    }
                }
            }

            if let Some(j) = best_j {
                // パラメータ数増加に対するペナルティ (+0.15) を付与して過剰適合を防止
                candidates.push(JCouplingCandidate {
                    pattern: "ddd".to_string(),
                    j_vals: j,
                    error: best_err + 0.15,
                    text: String::new(),
                });
            }
        }
    }

    // 0.01 Hz 未満の微小 J 値を除外
    candidates.retain(|c| c.j_vals.iter().all(|&j| j >= 0.01));

    // スコア昇順 (誤差最小順) にソート
    candidates.sort_by(|a, b| a.error.partial_cmp(&b.error).unwrap_or(std::cmp::Ordering::Equal));

    // 上位最大 4 候補を保持
    let mut top_candidates: Vec<JCouplingCandidate> = candidates.into_iter().take(4).collect();

    // テキストのフォーマット
    for c in &mut top_candidates {
        if c.pattern == "s" {
            c.text = format!("{} (s, {})", shift_str, nuclei_str);
        } else {
            let j_formatted: Vec<String> = c.j_vals.iter().map(|j| format!("{:.1}", j)).collect();
            c.text = format!(
                "{} ({}, J = {} Hz, {})",
                shift_str,
                c.pattern,
                j_formatted.join(", "),
                nuclei_str
            );
        }
    }

    // 常に5番目として multiplet 'm' を追加
    top_candidates.push(JCouplingCandidate {
        pattern: "m".to_string(),
        j_vals: Vec::new(),
        error: 999.0,
        text: format!("{} (m, {})", shift_str_m, nuclei_str),
    });

    top_candidates
}

/// J-coupling のテキストからソート用化学シフト値 (ppm) を抽出する。
/// 単一化学シフト（例: "7.26 (d, ...)"）の場合はその値、
/// 範囲表記（例: "7.27-7.25 (m, ...)"）の場合は左端（最大値）を返す。
pub fn parse_jcoupling_sort_ppm(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    // 最初の '(' の前にある文字列を化学シフト部分とする
    let shift_full = trimmed.split('(').next()?.trim();
    if shift_full.is_empty() {
        return None;
    }
    // "7.27-7.25" や "7.27 - 7.25" のようなハイフン区切りの場合
    // 先頭の負符号 '-' を除外して次のハイフンを探す
    let search_start = if shift_full.starts_with('-') { 1 } else { 0 };
    if let Some(idx) = shift_full[search_start..].find('-') {
        let left_part = shift_full[..search_start + idx].trim();
        left_part.parse::<f64>().ok()
    } else {
        shift_full.split_whitespace().next()?.parse::<f64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doublet_analysis() {
        // 7.26 ppm (400 MHz -> ~2904 Hz) で J = 7.8 Hz のダブレット
        let peaks_hz = vec![2900.1, 2907.9];
        let intensities = vec![100.0, 100.0];
        let candidates = analyze_multiplet(
            peaks_hz,
            intensities,
            "7.26",
            "7.25-7.27",
            "1H",
            1.0,
        );

        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].pattern, "d");
        approx::assert_relative_eq!(candidates[0].j_vals[0], 7.8, epsilon = 0.1);
        assert!(candidates[0].text.contains("J = 7.8 Hz"));
    }

    #[test]
    fn test_parse_jcoupling_sort_ppm() {
        assert_eq!(parse_jcoupling_sort_ppm("7.26 (d, J = 7.8 Hz, 1H)"), Some(7.26));
        assert_eq!(parse_jcoupling_sort_ppm("7.27-7.25 (m, 1H)"), Some(7.27));
        assert_eq!(parse_jcoupling_sort_ppm("7.27 - 7.25 (m, 1H)"), Some(7.27));
        assert_eq!(parse_jcoupling_sort_ppm("-0.05--0.10 (m, 2H)"), Some(-0.05));
        assert_eq!(parse_jcoupling_sort_ppm("-0.05 (s, 1H)"), Some(-0.05));
        assert_eq!(parse_jcoupling_sort_ppm("invalid"), None);
    }

    #[test]
    fn test_generate_theoretical_peaks_and_lorentzian() {
        // singlet
        let s_peaks = generate_theoretical_peaks("s", &[]);
        assert_eq!(s_peaks, vec![(0.0, 1.0)]);

        // multiplet 'm'
        let m_peaks = generate_theoretical_peaks("m", &[]);
        assert!(m_peaks.is_empty());

        // doublet J = 8.0 Hz
        let d_peaks = generate_theoretical_peaks("d", &[8.0]);
        assert_eq!(d_peaks.len(), 2);
        approx::assert_relative_eq!(d_peaks[0].0, -4.0, epsilon = 1e-3);
        approx::assert_relative_eq!(d_peaks[1].0, 4.0, epsilon = 1e-3);
        approx::assert_relative_eq!(d_peaks[0].1, 1.0, epsilon = 1e-3);
        approx::assert_relative_eq!(d_peaks[1].1, 1.0, epsilon = 1e-3);

        // triplet J = 7.0 Hz
        let t_peaks = generate_theoretical_peaks("t", &[7.0]);
        assert_eq!(t_peaks.len(), 3);
        approx::assert_relative_eq!(t_peaks[0].0, -7.0, epsilon = 1e-3);
        approx::assert_relative_eq!(t_peaks[1].0, 0.0, epsilon = 1e-3);
        approx::assert_relative_eq!(t_peaks[2].0, 7.0, epsilon = 1e-3);
        // 強度比 1:2:1 正規化後 0.5:1.0:0.5
        approx::assert_relative_eq!(t_peaks[0].1, 0.5, epsilon = 1e-3);
        approx::assert_relative_eq!(t_peaks[1].1, 1.0, epsilon = 1e-3);
        approx::assert_relative_eq!(t_peaks[2].1, 0.5, epsilon = 1e-3);

        // evaluate_lorentzian_sum
        let center_val = evaluate_lorentzian_sum(&t_peaks, 0.0, 1.0);
        let off_val = evaluate_lorentzian_sum(&t_peaks, 20.0, 1.0);
        assert!(center_val > off_val);

        // ddd theoretical peaks
        let ddd_peaks = generate_theoretical_peaks("ddd", &[11.0, 6.0, 2.5]);
        assert_eq!(ddd_peaks.len(), 8);
    }

    #[test]
    fn test_ddd_analysis() {
        // J1 = 11.0, J2 = 6.0, J3 = 2.5 Hz (center = 2000.0 Hz)
        // 8 peaks: +-9.75, +-7.25, +-3.75, +-1.25
        let center = 2000.0;
        let peaks_hz = vec![
            center - 9.75,
            center - 7.25,
            center - 3.75,
            center - 1.25,
            center + 1.25,
            center + 3.75,
            center + 7.25,
            center + 9.75,
        ];
        let intensities = vec![100.0; 8];

        let candidates = analyze_multiplet(
            peaks_hz,
            intensities,
            "5.00",
            "4.98-5.02",
            "1H",
            0.5,
        );

        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].pattern, "ddd");
        assert_eq!(candidates[0].j_vals.len(), 3);
        approx::assert_relative_eq!(candidates[0].j_vals[0], 11.0, epsilon = 0.5);
        approx::assert_relative_eq!(candidates[0].j_vals[1], 6.0, epsilon = 0.5);
        approx::assert_relative_eq!(candidates[0].j_vals[2], 2.5, epsilon = 0.5);
        assert!(candidates[0].text.contains("ddd, J ="));
    }
}

