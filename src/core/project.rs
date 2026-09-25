use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use ndarray::Array1;
use ndarray_npy::{ReadNpyExt, WriteNpyExt};
use num_complex::Complex64;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::core::analysis::{IntegrationItem, JCouplingResultItem, PeakItem};
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
        }
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

        // 初期自動位相補正がONの場合
        if self.state.ft_settings.auto_phase {
            self.auto_phase();
        }

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
            self.spectrum_real = Some(spec);
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

    /// ALS ベースライン補正を適用する
    pub fn auto_baseline(&mut self, lam: f64, p: f64) {
        if let Some(ref spec) = self.spectrum_real {
            if self.baseline_array.is_none() {
                let (corrected, bl) = crate::core::baseline::apply_baseline_correction(spec, lam, p);
                self.spectrum_real = Some(corrected);
                self.baseline_array = Some(bl);
            }
        }
    }

    /// ベースライン補正を取り消す
    pub fn clear_baseline(&mut self) {
        if self.baseline_array.is_some() {
            self.update_phase(self.state.p0, self.state.p1);
        }
    }

    /// 化学シフトのリファレンスを設定し、スペクトル全体および全解析項目をシフトする
    pub fn set_shift_reference(&mut self, current_ppm: f64, target_ppm: f64) {
        let shift = target_ppm - current_ppm;
        if let Some(ref mut ppm) = self.ppm {
            *ppm += shift;
            self.state.shift_reference += shift;

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

        // 実部スペクトルの復元: apply_phase - baseline
        if let Some(ref unphased) = self.complex_spectrum_unphased {
            let mut real = apply_phase_and_extract_real(unphased, self.state.p0, self.state.p1);
            if let Some(ref bl) = self.baseline_array {
                real = real - bl;
            }
            self.spectrum_real = Some(real);
        }

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
