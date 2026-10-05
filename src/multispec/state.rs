use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::core::project::Project;

pub const MULTISPEC_PALETTE: &[[u8; 3]] = &[
    [37, 99, 235],   // Blue
    [220, 38, 38],   // Red
    [22, 163, 74],   // Green
    [147, 51, 234],  // Purple
    [234, 88, 12],   // Orange
    [13, 148, 136],  // Teal
    [190, 24, 93],   // Pink / Magenta
    [161, 98, 7],    // Amber / Brown
    [71, 85, 105],   // Slate
    [99, 102, 241],  // Indigo
];

fn default_true() -> bool {
    true
}

fn default_y_scale_max() -> f64 {
    100.0
}

fn default_y_scale_min() -> f64 {
    0.0
}

/// マルチスペクトル比較の個別スペクトル項目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSpecItem {
    pub id: String,
    pub name: String,
    pub source_path: Option<PathBuf>,
    pub color: [u8; 3],
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_y_scale_max")]
    pub y_scale_max: f64,
    #[serde(default = "default_y_scale_min")]
    pub y_scale_min: f64,
    #[serde(default)]
    pub y_offset: f64,
    #[serde(default = "default_true")]
    pub show_integral: bool,
    #[serde(skip)]
    pub project: Option<Project>,
}

impl MultiSpecItem {
    pub fn new(id: String, name: String, source_path: Option<PathBuf>, project: Project, color_idx: usize) -> Self {
        let color = MULTISPEC_PALETTE[color_idx % MULTISPEC_PALETTE.len()];
        Self {
            id,
            name,
            source_path,
            color,
            visible: true,
            y_scale_max: 100.0,
            y_scale_min: 0.0,
            y_offset: 0.0,
            show_integral: true,
            project: Some(project),
        }
    }
}

fn default_fixed_slot_height() -> f32 {
    140.0
}

/// スタック表示のレイアウト方式 (画面全体均等フィット vs 固定高スクロール)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StackLayoutMode {
    #[default]
    Fit,    // 画面内に全スペクトルが収まるように等分自動縮小
    Scroll, // 各スペクトルの高さを固定して縦スクロール可能にする
}

pub fn clean_path(path: &std::path::Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{}", stripped))
    } else if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Undo/Redo 用のスナップショット (軽量プロパティ情報)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSpecSnapshot {
    #[serde(default)]
    pub title: String,
    pub items_meta: Vec<MultiSpecItemMeta>,
    pub selected_id: Option<String>,
    pub common_ppm_min: f64,
    pub common_ppm_max: f64,
    pub stack_spacing: f64,
    #[serde(default)]
    pub is_overlay: bool,
    #[serde(default)]
    pub stack_mode: StackLayoutMode,
    #[serde(default = "default_fixed_slot_height")]
    pub fixed_slot_height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSpecItemMeta {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
    pub visible: bool,
    pub y_scale_max: f64,
    pub y_scale_min: f64,
    pub y_offset: f64,
    pub show_integral: bool,
}

/// MultiSpec 専用の Undo/Redo 履歴管理
#[derive(Debug, Clone)]
pub struct MultiSpecHistory {
    pub history: Vec<MultiSpecSnapshot>,
    pub current_idx: isize,
}

impl MultiSpecHistory {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            current_idx: -1,
        }
    }

    pub fn commit(&mut self, snap: MultiSpecSnapshot) {
        if self.current_idx + 1 < (self.history.len() as isize) {
            self.history.truncate((self.current_idx + 1) as usize);
        }

        self.history.push(snap);
        if self.history.len() > 50 {
            self.history.remove(0);
        }
        self.current_idx = (self.history.len() as isize) - 1;
    }

    pub fn can_undo(&self) -> bool {
        self.current_idx > 0
    }

    pub fn can_redo(&self) -> bool {
        self.current_idx + 1 < (self.history.len() as isize)
    }

    pub fn undo(&mut self) -> Option<&MultiSpecSnapshot> {
        if self.can_undo() {
            self.current_idx -= 1;
            self.history.get(self.current_idx as usize)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&MultiSpecSnapshot> {
        if self.can_redo() {
            self.current_idx += 1;
            self.history.get(self.current_idx as usize)
        } else {
            None
        }
    }
}

impl Default for MultiSpecHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// MultiSpec 全体の状態管理
#[derive(Debug, Clone)]
pub struct MultiSpecState {
    pub title: String,
    pub items: Vec<MultiSpecItem>,
    pub selected_id: Option<String>,
    pub common_ppm_min: f64,
    pub common_ppm_max: f64,
    pub stack_spacing: f64,
    pub is_overlay: bool,
    pub stack_mode: StackLayoutMode,
    pub fixed_slot_height: f32,
    pub zoom_history: Vec<(f64, f64)>,
    pub history: MultiSpecHistory,
    pub next_item_num: usize,
}

impl Default for MultiSpecState {
    fn default() -> Self {
        Self {
            title: String::new(),
            items: Vec::new(),
            selected_id: None,
            common_ppm_min: -1.0,
            common_ppm_max: 10.0,
            stack_spacing: 0.25,
            is_overlay: false,
            stack_mode: StackLayoutMode::Fit,
            fixed_slot_height: 140.0,
            zoom_history: Vec::new(),
            history: MultiSpecHistory::new(),
            next_item_num: 1,
        }
    }
}

impl MultiSpecState {
    pub fn new() -> Self {
        let mut s = Self::default();
        let snap = s.create_snapshot();
        s.history.commit(snap);
        s
    }

    /// RSN ファイルから新規アイテムを追加する
    pub fn add_rsn_file<P: AsRef<std::path::Path>>(&mut self, path: P) -> crate::core::Result<String> {
        let p = path.as_ref();
        let mut proj = Project::new();
        proj.load_rsn(p)?;

        let filename = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Spectrum")
            .to_string();

        let id = format!("spec_{}", self.next_item_num);
        self.next_item_num += 1;

        let abs_path = std::fs::canonicalize(p)
            .map(|cp| clean_path(&cp))
            .unwrap_or_else(|_| clean_path(p));
        let item = MultiSpecItem::new(id.clone(), filename, Some(abs_path), proj, self.items.len());
        self.items.push(item);

        if self.selected_id.is_none() {
            self.selected_id = Some(id.clone());
        }

        self.update_common_ppm_range();
        self.auto_stack();
        self.push_history();

        Ok(id)
    }

    /// Project を直接アイテムとして追加する
    pub fn add_project(&mut self, name: String, source_path: Option<PathBuf>, project: Project) -> String {
        let id = format!("spec_{}", self.next_item_num);
        self.next_item_num += 1;

        let item = MultiSpecItem::new(id.clone(), name, source_path, project, self.items.len());
        self.items.push(item);

        if self.selected_id.is_none() {
            self.selected_id = Some(id.clone());
        }

        self.update_common_ppm_range();
        self.auto_stack();
        self.push_history();

        id
    }

    /// アイテムを削除する
    pub fn remove_item(&mut self, id: &str) -> bool {
        let before_len = self.items.len();
        self.items.retain(|it| it.id != id);
        if self.items.len() < before_len {
            if self.selected_id.as_deref() == Some(id) {
                self.selected_id = self.items.first().map(|it| it.id.clone());
            }
            self.auto_stack();
            self.push_history();
            true
        } else {
            false
        }
    }

    /// アイテムを1つ上に移動 (描画順序の繰り上げ)
    pub fn move_item_up(&mut self, id: &str) -> bool {
        if let Some(idx) = self.items.iter().position(|it| it.id == id) {
            if idx > 0 {
                self.items.swap(idx, idx - 1);
                self.auto_stack();
                self.push_history();
                return true;
            }
        }
        false
    }

    /// アイテムを1つ下に移動 (描画順序の繰り下げ)
    pub fn move_item_down(&mut self, id: &str) -> bool {
        if let Some(idx) = self.items.iter().position(|it| it.id == id) {
            if idx + 1 < self.items.len() {
                self.items.swap(idx, idx + 1);
                self.auto_stack();
                self.push_history();
                return true;
            }
        }
        false
    }

    /// アイテムを指定位置に移動 (ドラッグ＆ドロップ用)
    pub fn move_item_to(&mut self, from_idx: usize, to_idx: usize) -> bool {
        if from_idx >= self.items.len() || to_idx >= self.items.len() || from_idx == to_idx {
            return false;
        }
        let item = self.items.remove(from_idx);
        self.items.insert(to_idx, item);
        self.auto_stack();
        self.push_history();
        true
    }

    /// 全スペクトルの表示範囲を網羅するように共通 ppm 範囲を更新する
    pub fn update_common_ppm_range(&mut self) {
        let mut min_ppm = f64::INFINITY;
        let mut max_ppm = f64::NEG_INFINITY;

        for item in &self.items {
            if let Some(ref proj) = item.project {
                if let Some(ref ppm) = proj.ppm {
                    if let (Some(&first), Some(&last)) = (ppm.first(), ppm.last()) {
                        let p1 = first.min(last);
                        let p2 = first.max(last);
                        if p1 < min_ppm { min_ppm = p1; }
                        if p2 > max_ppm { max_ppm = p2; }
                    }
                }
            }
        }

        if min_ppm.is_finite() && max_ppm.is_finite() && max_ppm > min_ppm {
            self.common_ppm_min = min_ppm;
            self.common_ppm_max = max_ppm;
        }
    }

    /// 全スペクトルの表示可能全域 (最小 ppm, 最大 ppm) を取得
    pub fn get_full_ppm_range(&self) -> (f64, f64) {
        let mut min_ppm = f64::INFINITY;
        let mut max_ppm = f64::NEG_INFINITY;

        for item in &self.items {
            if let Some(ref proj) = item.project {
                if let Some(ref ppm) = proj.ppm {
                    if let (Some(&first), Some(&last)) = (ppm.first(), ppm.last()) {
                        let p1 = first.min(last);
                        let p2 = first.max(last);
                        if p1 < min_ppm { min_ppm = p1; }
                        if p2 > max_ppm { max_ppm = p2; }
                    }
                }
            }
        }

        if min_ppm.is_finite() && max_ppm.is_finite() && max_ppm > min_ppm {
            (min_ppm, max_ppm)
        } else {
            (-1.0, 10.0)
        }
    }

    /// ズーム範囲を設定し、直前の範囲をズーム履歴にプッシュ
    pub fn push_zoom(&mut self, min: f64, max: f64) {
        self.zoom_history.push((self.common_ppm_min, self.common_ppm_max));
        if self.zoom_history.len() > 30 {
            self.zoom_history.remove(0);
        }
        self.common_ppm_min = min;
        self.common_ppm_max = max;
    }

    /// ズームを1段階戻す (ダブルクリック / Backspace)
    pub fn undo_zoom(&mut self) -> bool {
        if let Some((prev_min, prev_max)) = self.zoom_history.pop() {
            self.common_ppm_min = prev_min;
            self.common_ppm_max = prev_max;
            true
        } else {
            false
        }
    }

    /// ズームを初期全体表示にリセット
    pub fn reset_zoom(&mut self) {
        self.zoom_history.clear();
        self.update_common_ppm_range();
    }

    /// スタック整列 (各スペクトルを縦に並べる)
    pub fn set_stack(&mut self) {
        self.is_overlay = false;
        for it in &mut self.items {
            it.y_offset = 0.0;
        }
    }

    /// 自動スタック整列 (互換用 alias)
    pub fn auto_stack(&mut self) {
        self.set_stack();
    }

    /// 重ね合わせ整列 (全スペクトルを同一ベースライン上に重ねる)
    pub fn set_overlay(&mut self) {
        self.is_overlay = true;
        for it in &mut self.items {
            it.y_offset = 0.0;
        }
        self.push_history();
    }

    /// 全スペクトルの Y-Scale Max を一括調整
    pub fn adjust_all_y_scale(&mut self, factor: f64) {
        for it in &mut self.items {
            it.y_scale_max = (it.y_scale_max * factor).clamp(1.0, 10000.0);
        }
    }

    /// 全スペクトルの Y-Scale Min を一括調整
    pub fn adjust_all_y_scale_min(&mut self, delta: f64) {
        for it in &mut self.items {
            it.y_scale_min = (it.y_scale_min + delta).clamp(0.0, 10000.0);
        }
    }

    /// 選択中のアイテムへの参照を取得
    pub fn selected_item(&self) -> Option<&MultiSpecItem> {
        self.selected_id.as_ref().and_then(|id| self.items.iter().find(|it| &it.id == id))
    }

    /// 選択中のアイテムへの可変参照を取得
    pub fn selected_item_mut(&mut self) -> Option<&mut MultiSpecItem> {
        if let Some(ref id) = self.selected_id.clone() {
            self.items.iter_mut().find(|it| &it.id == id)
        } else {
            None
        }
    }

    /// 現在のスナップショットを作成
    pub fn create_snapshot(&self) -> MultiSpecSnapshot {
        MultiSpecSnapshot {
            title: self.title.clone(),
            items_meta: self
                .items
                .iter()
                .map(|it| MultiSpecItemMeta {
                    id: it.id.clone(),
                    name: it.name.clone(),
                    color: it.color,
                    visible: it.visible,
                    y_scale_max: it.y_scale_max,
                    y_scale_min: it.y_scale_min,
                    y_offset: it.y_offset,
                    show_integral: it.show_integral,
                })
                .collect(),
            selected_id: self.selected_id.clone(),
            common_ppm_min: self.common_ppm_min,
            common_ppm_max: self.common_ppm_max,
            stack_spacing: self.stack_spacing,
            is_overlay: self.is_overlay,
            stack_mode: self.stack_mode,
            fixed_slot_height: self.fixed_slot_height,
        }
    }

    /// 現在の状態を Undo 履歴に保存
    pub fn push_history(&mut self) {
        let snap = self.create_snapshot();
        self.history.commit(snap);
    }

    /// Undo を実行
    pub fn undo(&mut self) -> bool {
        if let Some(snap) = self.history.undo().cloned() {
            self.apply_snapshot(&snap);
            true
        } else {
            false
        }
    }

    /// Redo を実行
    pub fn redo(&mut self) -> bool {
        if let Some(snap) = self.history.redo().cloned() {
            self.apply_snapshot(&snap);
            true
        } else {
            false
        }
    }

    fn apply_snapshot(&mut self, snap: &MultiSpecSnapshot) {
        self.title = snap.title.clone();
        self.selected_id = snap.selected_id.clone();
        self.common_ppm_min = snap.common_ppm_min;
        self.common_ppm_max = snap.common_ppm_max;
        self.stack_spacing = snap.stack_spacing;
        self.is_overlay = snap.is_overlay;
        self.stack_mode = snap.stack_mode;
        self.fixed_slot_height = snap.fixed_slot_height;

        // ID の並び順に合わせて items を並び替えてプロパティを更新
        let mut reordered: Vec<MultiSpecItem> = Vec::with_capacity(snap.items_meta.len());
        for meta in &snap.items_meta {
            if let Some(pos) = self.items.iter().position(|it| it.id == meta.id) {
                let mut it = self.items.remove(pos);
                it.name = meta.name.clone();
                it.color = meta.color;
                it.visible = meta.visible;
                it.y_scale_max = meta.y_scale_max;
                it.y_scale_min = meta.y_scale_min;
                it.y_offset = meta.y_offset;
                it.show_integral = meta.show_integral;
                reordered.push(it);
            }
        }
        // スナップショットに含まれなかった残りのアイテムがあれば末尾に追加
        reordered.extend(self.items.drain(..));
        self.items = reordered;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array1;

    fn create_dummy_project(min_ppm: f64, max_ppm: f64) -> Project {
        let mut proj = Project::new();
        let ppm = Array1::from_vec(vec![max_ppm, (max_ppm + min_ppm) * 0.5, min_ppm]);
        let spec = Array1::from_vec(vec![0.0, 100.0, 0.0]);
        proj.ppm = Some(ppm);
        proj.spectrum_real = Some(spec);
        proj
    }

    #[test]
    fn test_multispec_item_creation_and_auto_stack() {
        let mut state = MultiSpecState::new();
        let p1 = create_dummy_project(0.0, 10.0);
        let p2 = create_dummy_project(-1.0, 8.0);

        let id1 = state.add_project("Sample A".to_string(), None, p1);
        let id2 = state.add_project("Sample B".to_string(), None, p2);

        assert_eq!(state.items.len(), 2);
        assert_eq!(state.items[0].id, id1);
        assert_eq!(state.items[1].id, id2);

        // 共通 ppm 範囲が両方を網羅しているか
        assert_eq!(state.common_ppm_min, -1.0);
        assert_eq!(state.common_ppm_max, 10.0);

        // 自動スタックで is_overlay が false、各オフセットが 0.0 (プロット側で等間隔スロット配置)
        assert_eq!(state.is_overlay, false);
        assert_eq!(state.items[0].y_offset, 0.0);
        assert_eq!(state.items[1].y_offset, 0.0);

        // set_overlay で is_overlay が true
        state.set_overlay();
        assert_eq!(state.is_overlay, true);

        // ズーム履歴テスト
        state.push_zoom(1.0, 5.0);
        assert_eq!(state.common_ppm_min, 1.0);
        assert_eq!(state.common_ppm_max, 5.0);
        assert!(state.undo_zoom());
        assert_eq!(state.common_ppm_min, -1.0);
        assert_eq!(state.common_ppm_max, 10.0);

        // カラーが別々に割り振られているか
        assert_ne!(state.items[0].color, state.items[1].color);
    }

    #[test]
    fn test_multispec_history_undo_redo() {
        let mut state = MultiSpecState::new();
        let p1 = create_dummy_project(0.0, 10.0);
        let p2 = create_dummy_project(0.0, 10.0);

        let id1 = state.add_project("Sample 1".to_string(), None, p1);
        let id2 = state.add_project("Sample 2".to_string(), None, p2);

        // 初期状態で2アイテム
        assert_eq!(state.items.len(), 2);

        // 並び替え実行 (下へ移動)
        state.move_item_down(&id1);
        assert_eq!(state.items[0].id, id2);
        assert_eq!(state.items[1].id, id1);

        // Undo 実行
        assert!(state.undo());
        assert_eq!(state.items[0].id, id1);
        assert_eq!(state.items[1].id, id2);

        // Redo 実行
        assert!(state.redo());
        assert_eq!(state.items[0].id, id2);
        assert_eq!(state.items[1].id, id1);

        // move_item_to (ドラッグ＆ドロップ並び替え) テスト
        state.move_item_to(0, 1);
        assert_eq!(state.items[0].id, id1);
        assert_eq!(state.items[1].id, id2);

        // 3つ目のアイテムを追加してテスト
        let p3 = create_dummy_project(0.0, 10.0);
        let id3 = state.add_project("Sample 3".to_string(), None, p3);
        // 現在の並び: [id1, id2, id3]
        assert_eq!(state.items[0].id, id1);
        assert_eq!(state.items[1].id, id2);
        assert_eq!(state.items[2].id, id3);

        // 先頭 (0) を末尾 (2) へ移動 -> [id2, id3, id1]
        state.move_item_to(0, 2);
        assert_eq!(state.items[0].id, id2);
        assert_eq!(state.items[1].id, id3);
        assert_eq!(state.items[2].id, id1);

        // 末尾 (2) を先頭 (0) へ移動 -> [id1, id2, id3]
        state.move_item_to(2, 0);
        assert_eq!(state.items[0].id, id1);
        assert_eq!(state.items[1].id, id2);
        assert_eq!(state.items[2].id, id3);

        // title の保存 & Undo テスト
        state.title = "New Title".to_string();
        state.push_history();
        assert_eq!(state.title, "New Title");
        assert!(state.undo());
        assert_eq!(state.title, "");
    }
}

