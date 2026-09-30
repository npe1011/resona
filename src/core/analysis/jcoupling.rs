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
}

