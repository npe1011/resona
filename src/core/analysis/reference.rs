use ndarray::Array1;

/// 一般的な NMR 溶媒情報
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolventInfo {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub proton_ppm: Option<f64>,
    pub carbon_ppm: Option<f64>,
}

/// 主要な重溶媒の標準ケミカルシフトテーブル
pub const KNOWN_SOLVENTS: &[SolventInfo] = &[
    SolventInfo {
        name: "CDCl3",
        aliases: &["cdcl3", "chloroform", "chloroform-d"],
        proton_ppm: Some(7.26),
        carbon_ppm: Some(77.16),
    },
    SolventInfo {
        name: "DMSO-d6",
        aliases: &["dmso", "dmso-d6", "dmsod6", "dimethyl sulfoxide"],
        proton_ppm: Some(2.50),
        carbon_ppm: Some(39.52),
    },
    SolventInfo {
        name: "CD3OD",
        aliases: &["cd3od", "methanol", "methanol-d4", "meod"],
        proton_ppm: Some(3.31),
        carbon_ppm: Some(49.00),
    },
    SolventInfo {
        name: "Acetone-d6",
        aliases: &["acetone", "acetone-d6", "acetoned6"],
        proton_ppm: Some(2.05),
        carbon_ppm: Some(29.84),
    },
    SolventInfo {
        name: "D2O",
        aliases: &["d2o", "water", "heavy water"],
        proton_ppm: Some(4.79),
        carbon_ppm: None,
    },
    SolventInfo {
        name: "CD3CN",
        aliases: &["cd3cn", "acetonitrile", "acetonitrile-d3"],
        proton_ppm: Some(1.94),
        carbon_ppm: Some(1.32),
    },
    SolventInfo {
        name: "C6D6",
        aliases: &["c6d6", "benzene", "benzene-d6"],
        proton_ppm: Some(7.16),
        carbon_ppm: Some(128.06),
    },
    SolventInfo {
        name: "TMS",
        aliases: &["tms", "tetramethylsilane"],
        proton_ppm: Some(0.00),
        carbon_ppm: Some(0.00),
    },
];

/// 溶媒文字列と核種から標準ケミカルシフト (ppm) を解決する
pub fn resolve_solvent_target_ppm(solvent_str: &str, nucleus: &str) -> Option<(&'static str, f64)> {
    let s_clean = solvent_str
        .to_lowercase()
        .replace(['-', '_', ' '], "");
    if s_clean.is_empty() {
        return None;
    }

    let is_13c = nucleus.contains("13C") || nucleus.contains("C13");

    for info in KNOWN_SOLVENTS {
        for alias in info.aliases {
            let alias_clean = alias.to_lowercase().replace(['-', '_', ' '], "");
            if s_clean.contains(&alias_clean) || alias_clean.contains(&s_clean) {
                let target = if is_13c {
                    info.carbon_ppm
                } else {
                    info.proton_ppm
                };
                if let Some(ppm) = target {
                    return Some((info.name, ppm));
                }
            }
        }
    }
    None
}

/// 指定した target_ppm 周辺のピークを自動検出し、ピーク位置 (ppm) を返す。
/// 有意なピークが見つからなかった場合は None。
pub fn auto_detect_reference_peak(
    spectrum: &Array1<f64>,
    ppm: &Array1<f64>,
    target_ppm: f64,
    search_delta_ppm: f64,
    min_snr: f64,
) -> Option<f64> {
    let n = spectrum.len().min(ppm.len());
    if n == 0 {
        return None;
    }

    let search_min = target_ppm - search_delta_ppm;
    let search_max = target_ppm + search_delta_ppm;

    let mut best_i = None;
    let mut max_val = f64::NEG_INFINITY;

    for i in 0..n {
        let p = ppm[i];
        if p >= search_min && p <= search_max {
            let v = spectrum[i];
            if v > max_val {
                max_val = v;
                best_i = Some(i);
            }
        }
    }

    let mut vals: Vec<f64> = spectrum.to_vec();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = vals[n / 2];

    let mut abs_diffs: Vec<f64> = spectrum.iter().map(|&v| (v - median).abs()).collect();
    abs_diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mad = abs_diffs[n / 2];
    let noise = if mad > 0.0 { mad / 0.6745 } else { 1.0 };

    if let Some(idx) = best_i {
        // ベースライン中央値からの正味のピーク高さがノイズの min_snr 倍以上か判定
        if (max_val - median) > noise * min_snr {
            // 3点放物線補間によるサブサンプリング精度のピーク化学シフト算出
            if idx > 0 && idx < n - 1 {
                let y0 = spectrum[idx - 1];
                let y1 = spectrum[idx];
                let y2 = spectrum[idx + 1];
                let denom = y0 - 2.0 * y1 + y2;
                if denom < -1e-12 {
                    // 放物線の頂点インデックスの変位 delta_idx ([-1.0, 1.0])
                    let delta_idx = 0.5 * (y0 - y2) / denom;
                    if delta_idx.abs() <= 1.0 {
                        // ppm配列は一般に降順 (ppm[idx+1] < ppm[idx] < ppm[idx-1])
                        // インデックス増加方向のppm変化率 dppm/didx
                        let dppm_didx = (ppm[idx + 1] - ppm[idx - 1]) * 0.5;
                        return Some(ppm[idx] + delta_idx * dppm_didx);
                    }
                }
            }
            return Some(ppm[idx]);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_solvent_target_ppm() {
        // 1H
        assert_eq!(
            resolve_solvent_target_ppm("CDCl3", "1H"),
            Some(("CDCl3", 7.26))
        );
        assert_eq!(
            resolve_solvent_target_ppm("cdcl3", "1H"),
            Some(("CDCl3", 7.26))
        );
        assert_eq!(
            resolve_solvent_target_ppm("DMSO-d6", "1H"),
            Some(("DMSO-d6", 2.50))
        );
        assert_eq!(
            resolve_solvent_target_ppm("D2O", "1H"),
            Some(("D2O", 4.79))
        );
        assert_eq!(
            resolve_solvent_target_ppm("Unknown_Solvent", "1H"),
            None
        );

        // 13C
        assert_eq!(
            resolve_solvent_target_ppm("CDCl3", "13C"),
            Some(("CDCl3", 77.16))
        );
        assert_eq!(
            resolve_solvent_target_ppm("DMSO-d6", "13C"),
            Some(("DMSO-d6", 39.52))
        );
    }

    #[test]
    fn test_auto_detect_reference_peak() {
        let n = 200;
        let mut spec = vec![0.0; n];
        let mut ppm_vec = vec![0.0; n];
        for i in 0..n {
            ppm_vec[i] = 10.0 - (i as f64) * 0.05;
        }

        // 7.26 ppm 付近にピーク (index ≈ 55: 10.0 - 55 * 0.05 = 7.25 ppm)
        let peak_idx = 55;
        let target_ppm = 7.26;
        for i in 45..=65 {
            let dx = (i as f64 - peak_idx as f64) / 2.0;
            spec[i] += 50.0 * (-0.5 * dx * dx).exp();
        }

        // 微小ノイズ
        for i in 0..n {
            let pseudo_noise = ((i * 13 + 5) % 7) as f64 / 10.0 - 0.3;
            spec[i] += pseudo_noise;
        }

        let spectrum = Array1::from_vec(spec);
        let ppm = Array1::from_vec(ppm_vec);

        // 探索範囲 ±0.15 ppm 内で検出
        let detected = auto_detect_reference_peak(&spectrum, &ppm, target_ppm, 0.15, 2.0);
        assert!(detected.is_some());
        let peak_p = detected.unwrap();
        assert!((peak_p - 7.25).abs() < 0.02);

        // 範囲外 (例えば 2.50 ppm) では検出されない
        let not_found = auto_detect_reference_peak(&spectrum, &ppm, 2.50, 0.15, 2.0);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_parabolic_interpolation_subsample_precision() {
        // 離散点 (step = -0.02 ppm) の間に真のピークがあるケース
        let n = 50;
        let mut ppm_vec = vec![0.0; n];
        let mut spec_vec = vec![0.0; n];
        for i in 0..n {
            ppm_vec[i] = 7.70 - (i as f64) * 0.02;
        }
        // index 22 (ppm = 7.26) の近傍に放物線ピーク (頂点は u = 0.25, ppm = 7.255)
        // y = -(u - 0.25)^2 + 100 where u = i - 22
        // u = -1 (idx 21): -( -1.25 )^2 + 100 = 98.4375
        // u =  0 (idx 22): -( -0.25 )^2 + 100 = 99.9375
        // u =  1 (idx 23): -(  0.75 )^2 + 100 = 99.4375
        spec_vec[21] = 98.4375;
        spec_vec[22] = 99.9375;
        spec_vec[23] = 99.4375;

        let ppm = Array1::from_vec(ppm_vec);
        let spec = Array1::from_vec(spec_vec);

        let detected = auto_detect_reference_peak(&spec, &ppm, 7.26, 0.05, 2.0).unwrap();
        // 期待値: u = 0.25 なので、ppm = 7.26 + 0.25 * (-0.02) = 7.255
        assert!((detected - 7.255).abs() < 1e-6);
    }
}
