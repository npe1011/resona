use std::cell::Cell;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use ndarray::Array1;
use ndarray_npy::{ReadNpyExt, WriteNpyExt};
use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::core::analysis::{
    auto_detect_integrations, auto_detect_reference_peak, compute_integral, pick_peaks,
    resolve_solvent_target_ppm, AutoSensitivity, IntegrationItem, JCouplingResultItem, PeakItem,
};
use crate::core::baseline::BaselineMethod;
use crate::core::error::{ResonaError, Result};
use crate::core::io::{AcquisitionMetadata, JeolJdfReader, NmrDataSource};
use crate::core::pipeline::{process_raw_fid, FtSettings};
use crate::core::signal::apply_phase_and_extract_real;

/// Multiview (拡大インセット表示) 項目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiviewItem {
    pub id: String,
    pub src_x_min: f64,
    pub src_x_max: f64,
    #[serde(default)]
    pub src_y_min: Option<f64>,
    #[serde(default)]
    pub src_y_max: Option<f64>,
    #[serde(default = "default_ratio")]
    pub ratio: f64,
    pub geometry: RectF,
}

pub fn default_ratio() -> f64 {
    5.0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub fn min_x(&self) -> f32 {
        self.x
    }
    pub fn min_y(&self) -> f32 {
        self.y
    }
    pub fn max_x(&self) -> f32 {
        self.x + self.w
    }
    pub fn max_y(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

/// 表示範囲設定 (ズーム状態)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DisplaySettings {
    #[serde(default)]
    pub x_min: Option<f64>,
    #[serde(default)]
    pub x_max: Option<f64>,
    #[serde(default)]
    pub y_min_scale: Option<f64>,
    #[serde(default)]
    pub y_max_scale: Option<f64>,
}

/// プロジェクト内の解析状態 (シリアライズ対象)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectState {
    #[serde(default)]
    pub p0: f64,
    #[serde(default)]
    pub p1: f64,
    #[serde(default)]
    pub shift_reference: f64,
    /// 基準ピークの化学シフト (ppm)
    #[serde(default)]
    pub reference_point: Option<f64>,
    #[serde(default)]
    pub peak_threshold: Option<f64>,
    #[serde(default)]
    pub peaks: Vec<PeakItem>,
    #[serde(default)]
    pub integrations: Vec<IntegrationItem>,
    #[serde(default = "default_scale")]
    pub integration_scale: f64,
    #[serde(default = "default_offset")]
    pub integration_offset: f64,
    #[serde(default = "default_scale")]
    pub integration_ref_area: f64,
    #[serde(default = "default_scale")]
    pub integration_ref_value: f64,
    #[serde(default)]
    pub multiviews: Vec<MultiviewItem>,
    #[serde(default)]
    pub j_couplings: Vec<JCouplingResultItem>,
    #[serde(default)]
    pub ft_settings: FtSettings,
    #[serde(default)]
    pub display_settings: DisplaySettings,
    #[serde(default)]
    pub baseline_method: BaselineMethod,
    #[serde(default)]
    pub auto_sensitivity: AutoSensitivity,
}

fn default_scale() -> f64 {
    1.0
}
fn default_offset() -> f64 {
    0.03
}

impl Default for ProjectState {
    fn default() -> Self {
        Self {
            p0: 0.0,
            p1: 0.0,
            shift_reference: 0.0,
            reference_point: None,
            peak_threshold: None,
            peaks: Vec::new(),
            integrations: Vec::new(),
            integration_scale: 1.0,
            integration_offset: 0.03,
            integration_ref_area: 1.0,
            integration_ref_value: 1.0,
            multiviews: Vec::new(),
            j_couplings: Vec::new(),
            ft_settings: FtSettings::default(),
            display_settings: DisplaySettings::default(),
            baseline_method: BaselineMethod::None,
            auto_sensitivity: AutoSensitivity::Middle,
        }
    }
}

/// 履歴管理 (Undo / Redo)
#[derive(Debug, Clone)]
pub struct HistoryManager {
    history: Vec<(ProjectState, Option<Array1<f64>>, Option<Array1<f64>>)>,
    current_idx: isize,
}

impl HistoryManager {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            current_idx: -1,
        }
    }

    pub fn commit(
        &mut self,
        state: &ProjectState,
        ppm: &Option<Array1<f64>>,
        spectrum: &Option<Array1<f64>>,
    ) {
        let next_idx = (self.current_idx + 1) as usize;
        self.history.truncate(next_idx);
        self.history.push((state.clone(), ppm.clone(), spectrum.clone()));
        self.current_idx += 1;
    }

    pub fn undo(&mut self) -> Option<&(ProjectState, Option<Array1<f64>>, Option<Array1<f64>>)> {
        if self.current_idx > 0 {
            self.current_idx -= 1;
            self.history.get(self.current_idx as usize)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&(ProjectState, Option<Array1<f64>>, Option<Array1<f64>>)> {
        if self.current_idx + 1 < (self.history.len() as isize) {
            self.current_idx += 1;
            self.history.get(self.current_idx as usize)
        } else {
            None
        }
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Resona プロジェクト本体
#[derive(Debug, Clone)]
pub struct Project {
    /// 化学シフト軸 (ppm)
    pub ppm: Option<Array1<f64>>,
    /// 位相補正・ベースライン補正済みの実部スペクトル
    pub spectrum_real: Option<Array1<f64>>,
    /// 未位相補正の複素数スペクトル
    pub complex_spectrum_unphased: Option<Array1<Complex64>>,
    /// 生FIDデータ (Re-FT用)
    pub fid_raw: Option<Array1<Complex64>>,
    /// ALSで計算されたベースライン配列
    pub baseline_array: Option<Array1<f64>>,
    /// 測定メタデータ
    pub metadata: AcquisitionMetadata,
    /// ユーザー解析状態
    pub state: ProjectState,
    /// Undo/Redo 履歴
    pub history: HistoryManager,
    /// キャッシュされたノイズレベル (MAD)
    pub cached_noise_level: Cell<Option<f64>>,
    /// キャッシュされたスペクトルの最大強度
    pub cached_max_intensity: Cell<Option<f64>>,
}

/// Full Auto パイプライン処理の実行サマリ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FullAutoReport {
    pub p0: f64,
    pub p1: f64,
    pub baseline_applied: bool,
    pub baseline_desc: String,
    pub reference_applied: bool,
    pub reference_desc: String,
    pub peaks_count: usize,
    pub integrations_count: usize,
}

impl Project {
    pub fn new() -> Self {
        Self {
            ppm: None,
            spectrum_real: None,
            complex_spectrum_unphased: None,
            fid_raw: None,
            baseline_array: None,
            metadata: AcquisitionMetadata::default(),
            state: ProjectState::default(),
            history: HistoryManager::new(),
            cached_noise_level: Cell::new(None),
            cached_max_intensity: Cell::new(None),
        }
    }

    /// スペクトルの最大強度を取得する (キャッシュ付き)
    pub fn max_intensity(&self) -> f64 {
        if let Some(val) = self.cached_max_intensity.get() {
            return val;
        }
        let val = if let Some(ref spec) = self.spectrum_real {
            spec.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        } else {
            0.0
        };
        self.cached_max_intensity.set(Some(val));
        val
    }

    /// ノイズレベル (MAD) を取得する (キャッシュ付き)
    pub fn noise_level(&self) -> f64 {
        if let Some(val) = self.cached_noise_level.get() {
            return val;
        }
        let val = if let Some(ref spec) = self.spectrum_real {
            crate::core::analysis::estimate_noise_mad(spec)
        } else {
            1.0
        };
        self.cached_noise_level.set(Some(val));
        val
    }

    /// キャッシュを無効化する (スペクトル更新時に呼び出す)
    pub fn invalidate_cache(&self) {
        self.cached_noise_level.set(None);
        self.cached_max_intensity.set(None);
    }

    /// JDF ファイルを開き、初期化して処理を実行する
    pub fn load_jdf<P: AsRef<Path>>(&mut self, path: P, ft_settings: Option<FtSettings>) -> Result<()> {
        let raw_fid = JeolJdfReader::read_fid(&path)?;
        let settings = ft_settings.unwrap_or_default();

        let processed = process_raw_fid(&raw_fid, &settings, 0.0, 0.0)?;

        self.ppm = Some(processed.ppm);
        self.spectrum_real = Some(processed.spectrum_real);
        self.complex_spectrum_unphased = Some(processed.complex_spectrum_unphased);
        self.fid_raw = Some(raw_fid.data);
        self.metadata = raw_fid.metadata;
        self.baseline_array = None;

        self.state = ProjectState::default();
        self.state.ft_settings = settings;

        // 初期自動位相補正を実行
        self.auto_phase();

        self.invalidate_cache();
        self.history = HistoryManager::new();
        self.history.commit(&self.state, &self.ppm, &self.spectrum_real);

        Ok(())
    }

    /// 位相角を手動更新する
    pub fn update_phase(&mut self, p0: f64, p1: f64) {
        if let Some(ref unphased) = self.complex_spectrum_unphased {
            self.state.p0 = p0;
            self.state.p1 = p1;
            let spec = apply_phase_and_extract_real(unphased, p0, p1);

            // ベースライン補正が適用されていた場合はリセット
            self.baseline_array = None;
            self.state.baseline_method = BaselineMethod::None;
            self.spectrum_real = Some(spec);
            self.invalidate_cache();
            self.sync_analysis_to_spectrum();
        }
    }

    /// ACME 自動位相補正を実行する
    pub fn auto_phase(&mut self) -> (f64, f64) {
        if let Some(ref unphased) = self.complex_spectrum_unphased {
            let (p0, p1) = crate::core::autophase::autophase_acme(unphased);
            self.update_phase(p0, p1);
            (p0, p1)
        } else {
            (0.0, 0.0)
        }
    }

    /// スペクトル (spectrum_real) が Phase や Baseline の変更によって更新された際、
    /// 既存のピークの強度 (intensity) および積分の局所ベースライン端点 (y_start, y_end) を
    /// 新しいスペクトルに合わせて自動同期する
    pub fn sync_analysis_to_spectrum(&mut self) {
        if let (Some(ppm), Some(spec)) = (self.ppm.as_ref(), self.spectrum_real.as_ref()) {
            let n = ppm.len().min(spec.len());
            if n < 2 {
                return;
            }

            // 1. 各ピークの強度 (intensity) を現在のスペクトルから再サンプリング
            for peak in &mut self.state.peaks {
                let mut best_idx = 0;
                let mut min_diff = f64::MAX;
                for i in 0..n {
                    let diff = (ppm[i] - peak.ppm).abs();
                    if diff < min_diff {
                        min_diff = diff;
                        best_idx = i;
                    }
                }
                peak.intensity = spec[best_idx];
            }

            // 2. 各積分の局所ベースライン端点 (y_start, y_end) を現在のスペクトルレベルに同期
            for intg in &mut self.state.integrations {
                let mut idx_start = 0;
                let mut diff_start = f64::MAX;
                let mut idx_end = 0;
                let mut diff_end = f64::MAX;

                for i in 0..n {
                    let ds = (ppm[i] - intg.start_ppm).abs();
                    if ds < diff_start {
                        diff_start = ds;
                        idx_start = i;
                    }
                    let de = (ppm[i] - intg.end_ppm).abs();
                    if de < diff_end {
                        diff_end = de;
                        idx_end = i;
                    }
                }

                intg.y_start = spec[idx_start];
                intg.y_end = spec[idx_end];
            }
        }
    }

    /// ベースライン補正を適用する (手法とパラメータを BaselineMethod enum で指定)
    pub fn apply_baseline(&mut self, method: BaselineMethod) {
        if method == BaselineMethod::None {
            self.clear_baseline();
            return;
        }

        // 未補正の実部スペクトルを取得
        let base_real = if let Some(ref unphased) = self.complex_spectrum_unphased {
            apply_phase_and_extract_real(unphased, self.state.p0, self.state.p1)
        } else if let Some(ref spec) = self.spectrum_real {
            if let Some(ref bl) = self.baseline_array {
                spec + bl
            } else {
                spec.clone()
            }
        } else {
            return;
        };

        let (corrected, bl) = crate::core::baseline::apply_baseline_method(&base_real, method);
        self.spectrum_real = Some(corrected);
        self.baseline_array = bl;
        self.state.baseline_method = method;
        self.invalidate_cache();
        self.sync_analysis_to_spectrum();
    }

    /// ALS ベースライン補正を適用する (互換用)
    pub fn auto_baseline(&mut self, lam: f64, p: f64) {
        let _ = p;
        let log_lam = lam.log10();
        self.apply_baseline(BaselineMethod::AirPLS {
            log_lambda: log_lam,
            max_iter: 15,
        });
    }

    /// ベースライン補正を取り消す
    pub fn clear_baseline(&mut self) {
        self.state.baseline_method = BaselineMethod::None;
        if self.baseline_array.is_some() {
            self.update_phase(self.state.p0, self.state.p1);
            self.invalidate_cache();
        }
    }

    /// 化学シフトのリファレンスを設定し、スペクトル全体および全解析項目をシフトする
    pub fn set_shift_reference(&mut self, current_ppm: f64, target_ppm: f64) {
        let shift = target_ppm - current_ppm;
        if let Some(ref mut ppm) = self.ppm {
            *ppm += shift;
            self.state.shift_reference += shift;
            self.state.reference_point = Some(target_ppm);

            // ピークの追従
            for p in &mut self.state.peaks {
                p.ppm += shift;
            }

            // 積分の追従
            for intg in &mut self.state.integrations {
                intg.start_ppm += shift;
                intg.end_ppm += shift;
            }

            // Multiview (インセット) の追従 (Python版のバグ修正)
            for mv in &mut self.state.multiviews {
                mv.src_x_min += shift;
                mv.src_x_max += shift;
            }

            // J-coupling の追従 (Python版のバグ修正)
            for jc in &mut self.state.j_couplings {
                jc.ppm += shift;
            }
        }
    }

    /// 全ての解析処理 (Referenceシフト, Phase, Baseline, Peak, Integrate, Multiview, JCoupling) を初期状態にリセットする
    pub fn clear_all_processing(&mut self) {
        // 1. Reference シフトの巻き戻し
        if self.state.shift_reference.abs() > 1e-12 {
            if let Some(ref mut ppm) = self.ppm {
                *ppm -= self.state.shift_reference;
            }
        }
        self.state.shift_reference = 0.0;
        self.state.reference_point = None;

        // 2. Baseline のクリア
        self.baseline_array = None;
        self.state.baseline_method = BaselineMethod::None;

        // 3. Phase のリセット (P0=0, P1=0)
        if let Some(ref unphased) = self.complex_spectrum_unphased {
            self.state.p0 = 0.0;
            self.state.p1 = 0.0;
            self.spectrum_real = Some(apply_phase_and_extract_real(unphased, 0.0, 0.0));
        }

        // 4. 解析項目のクリア
        self.state.peaks.clear();
        self.state.peak_threshold = None;
        self.state.integrations.clear();
        self.state.integration_scale = 1.0;
        self.state.integration_offset = 0.03;
        self.state.integration_ref_area = 1.0;
        self.state.integration_ref_value = 1.0;
        self.state.multiviews.clear();
        self.state.j_couplings.clear();

        self.invalidate_cache();
    }

    /// Full Auto パイプラインを実行する
    ///
    /// 順序:
    /// 1. 全処理をクリア (clear_all_processing)
    /// 2. Phase-Auto (ACME)
    /// 3. Baseline-Auto (None でなければ適用)
    /// 4. Reference-Auto (メタデータの溶媒情報から解決し、有意なピークがあればシフト。見つからない/失敗したらスキップ)
    /// 5. Peak Pick-Auto (auto_sensitivity の閾値で検出)
    /// 6. Integrate-Auto (auto_sensitivity で領域検出 & スケール初期化)
    pub fn execute_full_auto(&mut self, baseline_method: BaselineMethod) -> FullAutoReport {
        // 1. 全処理クリア
        self.clear_all_processing();

        // 2. Phase-Auto
        let (p0, p1) = self.auto_phase();

        // 3. Baseline
        let (baseline_applied, baseline_desc) = if baseline_method != BaselineMethod::None {
            self.apply_baseline(baseline_method);
            let desc = match baseline_method {
                BaselineMethod::AirPLS { log_lambda, .. } => format!("airPLS (logλ={:.1})", log_lambda),
                BaselineMethod::Polynomial { order, .. } => format!("Polynomial (order={})", order),
                BaselineMethod::None => "None".to_string(),
            };
            (true, desc)
        } else {
            (false, "None".to_string())
        };

        // 4. Reference-Auto
        let (reference_applied, reference_desc) = {
            let solvent_str = &self.metadata.solvent;
            let nuc = &self.metadata.nucleus;
            let is_13c = nuc.contains("13C") || nuc.contains("C13");
            let delta = if is_13c { 1.00 } else { 0.10 };

            if let Some((solvent_name, target_ppm)) = resolve_solvent_target_ppm(solvent_str, nuc) {
                if let (Some(ppm), Some(spec)) = (&self.ppm, &self.spectrum_real) {
                    if let Some(peak_ppm) = auto_detect_reference_peak(spec, ppm, target_ppm, delta, 2.0) {
                        self.set_shift_reference(peak_ppm, target_ppm);
                        (true, format!("{}: {:.3} -> {:.3} ppm", solvent_name, peak_ppm, target_ppm))
                    } else {
                        (false, format!("{} peak not found in window ±{:.2} ppm (skipped)", solvent_name, delta))
                    }
                } else {
                    (false, "No spectrum data (skipped)".to_string())
                }
            } else if !solvent_str.is_empty() {
                (false, format!("Unknown solvent '{}' (skipped)", solvent_str))
            } else {
                (false, "No solvent in metadata (skipped)".to_string())
            }
        };

        // 5. Peak Pick-Auto
        let peaks_count = if let (Some(spec), Some(ppm)) = (&self.spectrum_real, &self.ppm) {
            let noise = self.noise_level();
            let factor = self.state.auto_sensitivity.peak_noise_factor();
            let thresh = noise * factor;
            self.state.peak_threshold = Some(thresh);
            self.state.peaks = pick_peaks(spec, ppm, thresh, &self.state.peaks);
            self.state.peaks.len()
        } else {
            0
        };

        // 6. Integrate-Auto
        let integrations_count = if let (Some(spec), Some(ppm)) = (&self.spectrum_real, &self.ppm) {
            self.state.integrations = auto_detect_integrations(spec, ppm, self.state.auto_sensitivity);
            let mut max_area = 0.0_f64;
            for intg in &self.state.integrations {
                if let Some(res) = compute_integral(spec, ppm, intg, 1.0, 1.0, 0.03) {
                    if res.total_area > max_area {
                        max_area = res.total_area;
                    }
                }
            }
            let max_spec = self.max_intensity();
            if max_area > 1e-12 && max_spec > 0.0 {
                self.state.integration_scale = (max_spec * 0.35) / max_area;
                self.state.integration_ref_area = max_area;
                self.state.integration_ref_value = 1.0;
            }
            self.state.integrations.len()
        } else {
            0
        };

        FullAutoReport {
            p0,
            p1,
            baseline_applied,
            baseline_desc,
            reference_applied,
            reference_desc,
            peaks_count,
            integrations_count,
        }
    }

    /// 現在の状態を履歴にコミットする
    pub fn push_history(&mut self) {
        self.history.commit(&self.state, &self.ppm, &self.spectrum_real);
    }

    /// Undo
    pub fn undo(&mut self) -> bool {
        if let Some(snapshot) = self.history.undo() {
            self.state = snapshot.0.clone();
            self.ppm = snapshot.1.clone();
            self.spectrum_real = snapshot.2.clone();
            self.invalidate_cache();
            true
        } else {
            false
        }
    }

    /// Redo
    pub fn redo(&mut self) -> bool {
        if let Some(snapshot) = self.history.redo() {
            self.state = snapshot.0.clone();
            self.ppm = snapshot.1.clone();
            self.spectrum_real = snapshot.2.clone();
            self.invalidate_cache();
            true
        } else {
            false
        }
    }

    /// プロジェクトを .rsn (または旧 .ez) ファイル (ZIPアーカイブ) として保存する
    pub fn save_rsn<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let file = File::create(p)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // 1. complex_spectrum.npy
        if let Some(ref complex_spec) = self.complex_spectrum_unphased {
            zip.start_file("complex_spectrum.npy", options)?;
            let mut cursor = Cursor::new(Vec::new());
            complex_spec.write_npy(&mut cursor)
                .map_err(|e| ResonaError::ProcessingError(format!("NPY write error: {}", e)))?;
            zip.write_all(&cursor.into_inner())?;
        }

        // 2. fid_raw.npy
        if let Some(ref fid_raw) = self.fid_raw {
            zip.start_file("fid_raw.npy", options)?;
            let mut cursor = Cursor::new(Vec::new());
            fid_raw.write_npy(&mut cursor)
                .map_err(|e| ResonaError::ProcessingError(format!("NPY write error: {}", e)))?;
            zip.write_all(&cursor.into_inner())?;
        }

        // 3. ppm.npy
        if let Some(ref ppm) = self.ppm {
            zip.start_file("ppm.npy", options)?;
            let mut cursor = Cursor::new(Vec::new());
            ppm.write_npy(&mut cursor)
                .map_err(|e| ResonaError::ProcessingError(format!("NPY write error: {}", e)))?;
            zip.write_all(&cursor.into_inner())?;
        }

        // 4. baseline.npy
        if let Some(ref bl) = self.baseline_array {
            zip.start_file("baseline.npy", options)?;
            let mut cursor = Cursor::new(Vec::new());
            bl.write_npy(&mut cursor)
                .map_err(|e| ResonaError::ProcessingError(format!("NPY write error: {}", e)))?;
            zip.write_all(&cursor.into_inner())?;
        }

        // 5. project.json
        #[derive(Serialize)]
        struct ProjectJsonData<'a> {
            metadata: &'a AcquisitionMetadata,
            state: &'a ProjectState,
        }

        let json_data = ProjectJsonData {
            metadata: &self.metadata,
            state: &self.state,
        };

        let json_str = serde_json::to_string_pretty(&json_data)
            .map_err(|e| ResonaError::ProcessingError(format!("JSON serialize error: {}", e)))?;

        zip.start_file("project.json", options)?;
        zip.write_all(json_str.as_bytes())?;

        zip.finish()?;
        Ok(())
    }

    /// .rsn (または旧 .ez) ファイル (ZIPアーカイブ) からプロジェクトを復元する
    pub fn load_rsn<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        // 1. complex_spectrum.npy
        if let Ok(mut entry) = archive.by_name("complex_spectrum.npy") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            let arr = Array1::<Complex64>::read_npy(Cursor::new(buf))
                .map_err(|e| ResonaError::ProcessingError(format!("NPY read error: {}", e)))?;
            self.complex_spectrum_unphased = Some(arr);
        }

        // 2. fid_raw.npy
        if let Ok(mut entry) = archive.by_name("fid_raw.npy") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            let arr = Array1::<Complex64>::read_npy(Cursor::new(buf))
                .map_err(|e| ResonaError::ProcessingError(format!("NPY read error: {}", e)))?;
            self.fid_raw = Some(arr);
        } else {
            self.fid_raw = None;
        }

        // 3. ppm.npy
        if let Ok(mut entry) = archive.by_name("ppm.npy") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            let arr = Array1::<f64>::read_npy(Cursor::new(buf))
                .map_err(|e| ResonaError::ProcessingError(format!("NPY read error: {}", e)))?;
            self.ppm = Some(arr);
        }

        // 4. baseline.npy
        if let Ok(mut entry) = archive.by_name("baseline.npy") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            let arr = Array1::<f64>::read_npy(Cursor::new(buf))
                .map_err(|e| ResonaError::ProcessingError(format!("NPY read error: {}", e)))?;
            self.baseline_array = Some(arr);
        } else {
            self.baseline_array = None;
        }

        // 5. project.json
        #[derive(Deserialize)]
        struct ProjectJsonData {
            #[serde(default)]
            metadata: AcquisitionMetadata,
            #[serde(default)]
            state: ProjectState,
        }

        if let Ok(mut entry) = archive.by_name("project.json") {
            let mut json_str = String::new();
            entry.read_to_string(&mut json_str)?;
            let data: ProjectJsonData = serde_json::from_str(&json_str)
                .map_err(|e| ResonaError::ProcessingError(format!("JSON parse error: {}", e)))?;
            self.metadata = data.metadata;
            self.state = data.state;
        }

        // 旧バージョンで保存されたプロジェクトの下位互換:
        // baseline.npy はあるが baseline_method が None の場合、デフォルト airPLS として扱う
        if self.baseline_array.is_some() && self.state.baseline_method == BaselineMethod::None {
            self.state.baseline_method = BaselineMethod::AirPLS {
                log_lambda: 8.0,
                max_iter: 15,
            };
        }

        // 実部スペクトルの復元: apply_phase - baseline
        if let Some(ref unphased) = self.complex_spectrum_unphased {
            let mut real = apply_phase_and_extract_real(unphased, self.state.p0, self.state.p1);
            if let Some(ref bl) = self.baseline_array {
                real = real - bl;
            }
            self.spectrum_real = Some(real);
        }

        self.invalidate_cache();
        self.history = HistoryManager::new();
        self.history.commit(&self.state, &self.ppm, &self.spectrum_real);

        Ok(())
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_cache_and_invalidation() {
        let mut proj = Project::new();
        proj.spectrum_real = Some(Array1::from_vec(vec![1.0, 50.0, 2.0, 10.0, 3.0]));
        proj.complex_spectrum_unphased = Some(Array1::from_vec(vec![
            Complex64::new(1.0, 0.0),
            Complex64::new(50.0, 0.0),
            Complex64::new(2.0, 0.0),
            Complex64::new(10.0, 0.0),
            Complex64::new(3.0, 0.0),
        ]));

        assert!(proj.cached_max_intensity.get().is_none());
        assert!(proj.cached_noise_level.get().is_none());

        // 初回計算
        let max_val = proj.max_intensity();
        assert_eq!(max_val, 50.0);
        assert_eq!(proj.cached_max_intensity.get(), Some(50.0));

        let noise = proj.noise_level();
        assert!(noise > 0.0);
        assert_eq!(proj.cached_noise_level.get(), Some(noise));

        // 位相変更によるキャッシュ無効化のテスト
        proj.update_phase(180.0, 0.0);
        assert!(proj.cached_max_intensity.get().is_none());
        assert!(proj.cached_noise_level.get().is_none());

        // 位相180度後の最大値 (元の-1.0〜-50.0なので最大値は-1.0)
        let new_max = proj.max_intensity();
        assert!((new_max - (-1.0)).abs() < 1e-6);
        assert_eq!(proj.cached_max_intensity.get(), Some(new_max));
    }

    #[test]
    fn test_project_baseline_state_and_undo() {
        let mut proj = Project::new();
        proj.spectrum_real = Some(Array1::from_vec(vec![10.0, 12.0, 14.0, 16.0, 18.0]));
        proj.complex_spectrum_unphased = Some(Array1::from_vec(vec![
            Complex64::new(10.0, 0.0),
            Complex64::new(12.0, 0.0),
            Complex64::new(14.0, 0.0),
            Complex64::new(16.0, 0.0),
            Complex64::new(18.0, 0.0),
        ]));
        proj.push_history();

        // 1. Polynomial baseline 適用
        let poly_method = BaselineMethod::Polynomial { order: 1, max_iter: 5 };
        proj.apply_baseline(poly_method);
        assert_eq!(proj.state.baseline_method, poly_method);
        assert!(proj.baseline_array.is_some());
        proj.push_history();

        // 2. airPLS baseline 適用
        let airpls_method = BaselineMethod::AirPLS { log_lambda: 6.0, max_iter: 10 };
        proj.apply_baseline(airpls_method);
        assert_eq!(proj.state.baseline_method, airpls_method);
        proj.push_history();

        // 3. Clear baseline
        proj.clear_baseline();
        assert_eq!(proj.state.baseline_method, BaselineMethod::None);
        assert!(proj.baseline_array.is_none());
        proj.push_history();

        // 4. Undo 検証
        assert!(proj.undo()); // back to airPLS
        assert_eq!(proj.state.baseline_method, BaselineMethod::AirPLS { log_lambda: 6.0, max_iter: 10 });

        assert!(proj.undo()); // back to Polynomial
        assert_eq!(proj.state.baseline_method, BaselineMethod::Polynomial { order: 1, max_iter: 5 });

        assert!(proj.undo()); // back to initial (None)
        assert_eq!(proj.state.baseline_method, BaselineMethod::None);

        // 5. Redo 検証
        assert!(proj.redo()); // back to Polynomial
        assert_eq!(proj.state.baseline_method, BaselineMethod::Polynomial { order: 1, max_iter: 5 });

        // 6. JSON シリアライズ・デシリアライズの整合性
        let serialized = serde_json::to_string(&proj.state).unwrap();
        let deserialized: ProjectState = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.baseline_method, proj.state.baseline_method);
        assert_eq!(deserialized.auto_sensitivity, proj.state.auto_sensitivity);
    }

    #[test]
    fn test_project_clear_all_processing_and_full_auto() {
        let mut proj = Project::new();
        let n = 256;
        let mut ppm_vec = vec![0.0; n];
        let mut unphased_vec = vec![Complex64::new(0.0, 0.0); n];
        for i in 0..n {
            ppm_vec[i] = 12.0 - (i as f64) * (14.0 / n as f64);
        }

        // 7.26 ppm 付近に CDCl3 ピーク (i ≈ 86)
        let cdcl3_idx = 86;
        for i in 75..=97 {
            let dx = (i as f64 - cdcl3_idx as f64) / 2.0;
            let val = 100.0 * (-0.5 * dx * dx).exp();
            unphased_vec[i] = Complex64::new(val, 0.0);
        }

        // 2.0 ppm 付近にサンプルピーク (i ≈ 182)
        let sample_idx = 182;
        for i in 170..=195 {
            let dx = (i as f64 - sample_idx as f64) / 3.0;
            let val = 60.0 * (-0.5 * dx * dx).exp();
            unphased_vec[i] += Complex64::new(val, 0.0);
        }

        proj.ppm = Some(Array1::from_vec(ppm_vec.clone()));
        proj.complex_spectrum_unphased = Some(Array1::from_vec(unphased_vec.clone()));
        proj.spectrum_real = Some(Array1::from_vec(unphased_vec.iter().map(|c| c.re).collect()));
        proj.metadata.solvent = "CDCl3".to_string();
        proj.metadata.nucleus = "1H".to_string();

        // 1. ダミーの処理状態を設定
        proj.state.peaks.push(PeakItem { ppm: 2.0, intensity: 60.0, is_auto: false });
        proj.state.integrations.push(IntegrationItem {
            id: "intg-1".to_string(),
            start_ppm: 2.5,
            end_ppm: 1.5,
            y_start: 0.0,
            y_end: 0.0,
        });
        proj.set_shift_reference(7.20, 7.26); // +0.06 ppm shift
        assert!((proj.state.shift_reference - 0.06).abs() < 1e-4);

        // 2. clear_all_processing の検証
        proj.clear_all_processing();
        assert_eq!(proj.state.shift_reference, 0.0);
        assert!(proj.state.peaks.is_empty());
        assert!(proj.state.integrations.is_empty());
        assert_eq!(proj.state.baseline_method, BaselineMethod::None);
        assert_eq!(proj.state.p0, 0.0);
        assert_eq!(proj.state.p1, 0.0);

        // 3. execute_full_auto の実行検証
        proj.push_history(); // 実行前の状態を履歴に保存
        let report = proj.execute_full_auto(BaselineMethod::AirPLS {
            log_lambda: 8.0,
            max_iter: 15,
        });

        assert!(report.baseline_applied);
        assert!(report.reference_applied, "CDCl3 reference should be detected and applied");
        assert!(report.peaks_count >= 2, "Both CDCl3 and sample peaks should be picked");
        assert!(report.integrations_count >= 1, "At least one region should be integrated");

        // 4. Full Auto 適用状態をコミットし、Undo で実行前のまっさらな状態に復元できるか
        proj.push_history(); // 実行後の状態をコミット (current_idx = 1)
        assert!(proj.undo()); // Undo で current_idx = 0 に戻る
        assert!(proj.state.peaks.is_empty());
        assert!(proj.state.integrations.is_empty());
    }

    #[test]
    fn test_sync_analysis_to_spectrum() {
        let mut proj = Project::new();
        let ppm = Array1::from_vec(vec![3.0, 2.0, 1.0]);
        // 初期スペクトル: [10.0, 50.0, 10.0]
        let spec_initial = Array1::from_vec(vec![10.0, 50.0, 10.0]);
        let unphased = Array1::from_vec(vec![
            Complex64::new(10.0, 0.0),
            Complex64::new(50.0, 0.0),
            Complex64::new(10.0, 0.0),
        ]);

        proj.ppm = Some(ppm);
        proj.complex_spectrum_unphased = Some(unphased);
        proj.spectrum_real = Some(spec_initial);

        // ピークと積分区間を登録
        proj.state.peaks.push(PeakItem { ppm: 2.0, intensity: 50.0, is_auto: true });
        proj.state.integrations.push(IntegrationItem {
            id: "intg-1".to_string(),
            start_ppm: 3.0,
            end_ppm: 1.0,
            y_start: 10.0,
            y_end: 10.0,
        });

        // ベースライン補正を適用 (平坦なオフセット補正相当: spec が [0.0, 40.0, 0.0] に変化)
        let poly_method = BaselineMethod::Polynomial { order: 0, max_iter: 10 };
        proj.apply_baseline(poly_method);

        // ピーク強度と積分の y_start, y_end が更新されたスペクトルに自動同期されているか
        let new_spec = proj.spectrum_real.as_ref().unwrap();
        assert_eq!(proj.state.peaks[0].intensity, new_spec[1], "Peak intensity should sync to new spectrum");
        assert_eq!(proj.state.integrations[0].y_start, new_spec[0], "Integration y_start should sync");
        assert_eq!(proj.state.integrations[0].y_end, new_spec[2], "Integration y_end should sync");
    }
}

