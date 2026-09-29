use serde::{Deserialize, Serialize};

/// 各種 Auto 解析処理 (Peak Pick, Integrate, Multiview, FullAuto) の判定感度レベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoSensitivity {
    Low,
    Middle,
    High,
}

impl Default for AutoSensitivity {
    fn default() -> Self {
        AutoSensitivity::Middle
    }
}

impl AutoSensitivity {
    /// 表示用ラベル
    pub fn label(&self) -> &'static str {
        match self {
            AutoSensitivity::Low => "Low",
            AutoSensitivity::Middle => "Middle",
            AutoSensitivity::High => "High",
        }
    }

    /// ピーク検出のノイズ倍率 (閾値 = noise * factor)
    /// - High: 10.0 (従来のデフォルト設定。微小ピークや肩ピークも積極的に検出)
    /// - Middle: 25.0 (標準設定。微小不純物やノイズ揺らぎを抑制し、有意なピークを検出)
    /// - Low: 60.0 (主ピークのみ厳選検出)
    pub fn peak_noise_factor(&self) -> f64 {
        match self {
            AutoSensitivity::High => 10.0,
            AutoSensitivity::Middle => 25.0,
            AutoSensitivity::Low => 60.0,
        }
    }

    /// 積分検出のノイズ倍率 (有意ピーク領域の判定閾値 = median + factor * noise)
    /// - High: 15.0 (従来のデフォルト設定。微小ピークも積分区間に含める)
    /// - Middle: 30.0 (標準設定。中〜大ピーク群を積分区間として検出)
    /// - Low: 70.0 (主要な強ピークのみ積分区間として検出)
    pub fn integral_noise_factor(&self) -> f64 {
        match self {
            AutoSensitivity::High => 15.0,
            AutoSensitivity::Middle => 30.0,
            AutoSensitivity::Low => 70.0,
        }
    }

    /// Multiview 自動生成時の微小積分除外比率 (最大積分値に対する割合)
    /// - High: 0.0 (すべての積分区間をインセット化)
    /// - Middle: 0.02 (最大積分の 2% 未満の微小不純物区間を除外)
    /// - Low: 0.05 (最大積分の 5% 未満の区間を除外)
    pub fn multiview_min_area_ratio(&self) -> f64 {
        match self {
            AutoSensitivity::High => 0.0,
            AutoSensitivity::Middle => 0.02,
            AutoSensitivity::Low => 0.05,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_sensitivity_defaults_and_factors() {
        assert_eq!(AutoSensitivity::default(), AutoSensitivity::Middle);
        assert_eq!(AutoSensitivity::Middle.label(), "Middle");

        // High: 高感度 (閾値倍率が最小)
        assert_eq!(AutoSensitivity::High.peak_noise_factor(), 10.0);
        assert_eq!(AutoSensitivity::High.integral_noise_factor(), 15.0);
        assert_eq!(AutoSensitivity::High.multiview_min_area_ratio(), 0.0);

        // Middle: 中感度
        assert_eq!(AutoSensitivity::Middle.peak_noise_factor(), 25.0);
        assert_eq!(AutoSensitivity::Middle.integral_noise_factor(), 30.0);
        assert_eq!(AutoSensitivity::Middle.multiview_min_area_ratio(), 0.02);

        // Low: 低感度 (閾値倍率が最大)
        assert_eq!(AutoSensitivity::Low.peak_noise_factor(), 60.0);
        assert_eq!(AutoSensitivity::Low.integral_noise_factor(), 70.0);
        assert_eq!(AutoSensitivity::Low.multiview_min_area_ratio(), 0.05);
    }
}
