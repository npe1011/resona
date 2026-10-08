use std::collections::HashSet;
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

fn default_y_scale() -> f64 {
    100.0
}

fn default_y_offset() -> f64 {
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
    #[serde(default = "default_y_scale")]
    pub y_scale: f64,
    #[serde(default = "default_y_offset")]
    pub y_offset: f64,
    #[serde(default = "default_true")]
    pub show_integral: bool,
    #[serde(skip)]
    pub project: Option<Project>,

    // --- レガシー互換フィールド (将来ここを削除すれば完全移行) ---
    #[serde(default, skip_serializing)]
    pub y_scale_max: Option<f64>,
    #[serde(default, skip_serializing)]
    pub y_scale_min: Option<f64>,
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
            y_scale: 100.0,
            y_offset: 0.0,
            show_integral: true,
            project: Some(project),
            y_scale_max: None,
            y_scale_min: None,
        }
    }

    /// レガシー形式 (y_scale_max, y_scale_min) から (y_scale, y_offset) への自動移行
    /// ※ 将来互換性を完全に削除する際は、このメソッド定義および load_rsm 内の呼び出し行を削除するだけで完了します。
    pub fn migrate_legacy_y(&mut self) {
        if let Some(old_max) = self.y_scale_max {
            self.y_scale = old_max;
        }
        if let Some(old_min) = self.y_scale_min {
            self.y_offset = old_min;
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
    pub y_scale: f64,
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
    pub selected_ids: HashSet<String>,
    pub common_ppm_min: f64,
    pub common_ppm_max: f64,
    pub stack_spacing: f64,
    pub is_overlay: bool,
    pub stack_mode: StackLayoutMode,
    pub fixed_slot_height: f32,
    pub zoom_history: Vec<(f64, f64)>,
    pub history: MultiSpecHistory,
    pub next_item_num: usize,
    pub rsm_path: Option<PathBuf>,
    pub is_dirty: bool,
}

impl Default for MultiSpecState {
    fn default() -> Self {
        Self {
            title: String::new(),
            items: Vec::new(),
            selected_id: None,
            selected_ids: HashSet::new(),
            common_ppm_min: -1.0,
            common_ppm_max: 10.0,
            stack_spacing: 0.25,
            is_overlay: false,
            stack_mode: StackLayoutMode::Fit,
            fixed_slot_height: 140.0,
            zoom_history: Vec::new(),
            history: MultiSpecHistory::new(),
            next_item_num: 1,
            rsm_path: None,
            is_dirty: false,
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

    /// 既存の全アイテムと絶対に重複しない新しいアイテム ID を生成する
    pub fn generate_unique_id(&mut self) -> String {
        let existing_ids: HashSet<&str> = self.items.iter().map(|it| it.id.as_str()).collect();
        loop {
            let candidate = format!("spec_{}", self.next_item_num);
            self.next_item_num += 1;
            if !existing_ids.contains(candidate.as_str()) {
                return candidate;
            }
        }
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

        let id = self.generate_unique_id();

        let abs_path = std::fs::canonicalize(p)
            .map(|cp| clean_path(&cp))
            .unwrap_or_else(|_| clean_path(p));
        let item = MultiSpecItem::new(id.clone(), filename, Some(abs_path), proj, self.items.len());
        self.items.push(item);

        if self.selected_id.is_none() {
            self.selected_id = Some(id.clone());
            self.selected_ids.insert(id.clone());
        }

        self.update_common_ppm_range();
        self.auto_stack();
        self.push_history();

        Ok(id)
    }

    /// Project を直接アイテムとして追加する
    pub fn add_project(&mut self, name: String, source_path: Option<PathBuf>, project: Project) -> String {
        let id = self.generate_unique_id();

        let item = MultiSpecItem::new(id.clone(), name, source_path, project, self.items.len());
        self.items.push(item);

        if self.selected_id.is_none() {
            self.selected_id = Some(id.clone());
            self.selected_ids.insert(id.clone());
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
        self.selected_ids.remove(id);
        if self.items.len() < before_len {
            if self.selected_id.as_deref() == Some(id) {
                self.selected_id = self
                    .selected_ids
                    .iter()
                    .next()
                    .cloned()
                    .or_else(|| self.items.first().map(|it| it.id.clone()));
                if let Some(ref sid) = self.selected_id {
                    self.selected_ids.insert(sid.clone());
                }
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

    /// 複数アイテム (ids) を元の相対順序を保ったまま drop_slot 位置へ一括移動 (ドラッグ＆ドロップ用)
    pub fn move_items_to(&mut self, ids: &HashSet<String>, drop_slot: usize) -> bool {
        if ids.is_empty() || self.items.is_empty() {
            return false;
        }

        let orig_ids: Vec<String> = self.items.iter().map(|it| it.id.clone()).collect();
        let slot_clamped = drop_slot.min(self.items.len());
        let insert_idx = orig_ids[..slot_clamped]
            .iter()
            .filter(|id| !ids.contains(*id))
            .count();

        let mut moving = Vec::new();
        let mut remaining = Vec::new();

        for it in self.items.drain(..) {
            if ids.contains(&it.id) {
                moving.push(it);
            } else {
                remaining.push(it);
            }
        }

        let insert_idx = insert_idx.min(remaining.len());
        remaining.splice(insert_idx..insert_idx, moving);

        let changed = remaining.iter().zip(orig_ids.iter()).any(|(a, b)| a.id != *b);
        self.items = remaining;
        if changed {
            self.auto_stack();
            self.push_history();
        }
        changed
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

    /// 全スペクトルの Y-Scale を一括調整
    pub fn adjust_all_y_scale(&mut self, factor: f64) {
        for it in &mut self.items {
            it.y_scale = (it.y_scale * factor).clamp(1.0, 10000.0);
        }
    }

    /// 全スペクトルの Y-Offset (ベースライン位置) を一括調整
    pub fn adjust_all_y_offset(&mut self, delta: f64) {
        for it in &mut self.items {
            it.y_offset = (it.y_offset + delta).clamp(-1000.0, 10000.0);
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

    /// 指定された ID が選択されているか判定
    pub fn is_selected(&self, id: &str) -> bool {
        self.selected_ids.contains(id) || self.selected_id.as_deref() == Some(id)
    }

    /// 単一選択 (既存の選択をクリアして 1 つだけ選択)
    pub fn select_single(&mut self, id: String) {
        self.selected_id = Some(id.clone());
        self.selected_ids.clear();
        self.selected_ids.insert(id);
    }

    /// 選択の切り替え (Ctrl + クリック)
    pub fn toggle_select(&mut self, id: &str) {
        if self.selected_ids.contains(id) {
            self.selected_ids.remove(id);
            if self.selected_id.as_deref() == Some(id) {
                self.selected_id = self.selected_ids.iter().next().cloned();
            }
        } else {
            self.selected_ids.insert(id.to_string());
            self.selected_id = Some(id.to_string());
        }
    }

    /// 範囲選択 (Shift + クリック)
    /// 直前の selected_id (アンカー) から target_id までの全アイテムを選択
    pub fn select_range(&mut self, target_id: &str) {
        let anchor_idx = self
            .selected_id
            .as_ref()
            .and_then(|id| self.items.iter().position(|it| &it.id == id));
        let target_idx = self.items.iter().position(|it| it.id == target_id);

        if let (Some(i1), Some(i2)) = (anchor_idx, target_idx) {
            let start = i1.min(i2);
            let end = i1.max(i2);
            self.selected_ids.clear();
            for it in &self.items[start..=end] {
                self.selected_ids.insert(it.id.clone());
            }
        } else {
            self.select_single(target_id.to_string());
        }
    }

    /// 全選択 (Ctrl + A)
    pub fn select_all(&mut self) {
        self.selected_ids.clear();
        for it in &self.items {
            self.selected_ids.insert(it.id.clone());
        }
        if self.selected_id.is_none() {
            self.selected_id = self.items.first().map(|it| it.id.clone());
        }
    }

    /// 選択全解除
    pub fn clear_selection(&mut self) {
        self.selected_ids.clear();
        self.selected_id = None;
    }

    /// 選択中スペクトルの Y-Scale を個別調整 (係数倍)
    pub fn adjust_selected_y_scale(&mut self, factor: f64) {
        let selected_id = self.selected_id.as_deref();
        for it in &mut self.items {
            if self.selected_ids.contains(&it.id) || selected_id == Some(&it.id) {
                it.y_scale = (it.y_scale * factor).clamp(1.0, 10000.0);
            }
        }
    }

    /// 選択中スペクトルの Y-Offset (ベースライン位置) を個別調整 (差分加算)
    pub fn adjust_selected_y_offset(&mut self, delta: f64) {
        let selected_id = self.selected_id.as_deref();
        for it in &mut self.items {
            if self.selected_ids.contains(&it.id) || selected_id == Some(&it.id) {
                it.y_offset = (it.y_offset + delta).clamp(-1000.0, 10000.0);
            }
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
                    y_scale: it.y_scale,
                    y_offset: it.y_offset,
                    show_integral: it.show_integral,
                })
                .collect(),
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
        self.is_dirty = true;
    }

    /// Undo を実行
    pub fn undo(&mut self) -> bool {
        if let Some(snap) = self.history.undo().cloned() {
            self.apply_snapshot(&snap);
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    /// Redo を実行
    pub fn redo(&mut self) -> bool {
        if let Some(snap) = self.history.redo().cloned() {
            self.apply_snapshot(&snap);
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    fn apply_snapshot(&mut self, snap: &MultiSpecSnapshot) {
        self.title = snap.title.clone();
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
                it.y_scale = meta.y_scale;
                it.y_offset = meta.y_offset;
                it.show_integral = meta.show_integral;
                reordered.push(it);
            }
        }
        // スナップショットに含まれなかった残りのアイテムがあれば末尾に追加
        reordered.extend(self.items.drain(..));
        self.items = reordered;

        // 削除されたアイテムが存在する場合のみ選択から除外 (選択状態は Undo 履歴に含めず維持)
        self.selected_ids.retain(|id| self.items.iter().any(|it| &it.id == id));
        if let Some(ref sid) = self.selected_id {
            if !self.items.iter().any(|it| &it.id == sid) {
                self.selected_id = self.selected_ids.iter().next().cloned();
            }
        }
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

    #[test]
    fn test_multispec_selection_and_shortcuts() {
        let mut state = MultiSpecState::new();
        let p1 = create_dummy_project(0.0, 10.0);
        let p2 = create_dummy_project(0.0, 10.0);
        let p3 = create_dummy_project(0.0, 10.0);

        let id1 = state.add_project("Sample 1".to_string(), None, p1);
        let id2 = state.add_project("Sample 2".to_string(), None, p2);
        let id3 = state.add_project("Sample 3".to_string(), None, p3);

        // 初期状態: 最初のアイテムが選択されている
        assert!(state.is_selected(&id1));
        assert!(!state.is_selected(&id2));
        assert!(!state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 1);

        // Shift 範囲選択 (id1 から id3 まで)
        state.select_range(&id3);
        assert!(state.is_selected(&id1));
        assert!(state.is_selected(&id2));
        assert!(state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 3);

        // Ctrl トグル選択 (id2 を除外)
        state.toggle_select(&id2);
        assert!(state.is_selected(&id1));
        assert!(!state.is_selected(&id2));
        assert!(state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 2);

        // 単一選択 (id2 のみ)
        state.select_single(id2.clone());
        assert!(!state.is_selected(&id1));
        assert!(state.is_selected(&id2));
        assert!(!state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 1);

        // 全選択 (Ctrl + A)
        state.select_all();
        assert!(state.is_selected(&id1));
        assert!(state.is_selected(&id2));
        assert!(state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 3);

        // 選択解除
        state.clear_selection();
        assert!(!state.is_selected(&id1));
        assert!(!state.is_selected(&id2));
        assert!(!state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 0);
    }

    #[test]
    fn test_multispec_adjust_selected_y_scale() {
        let mut state = MultiSpecState::new();
        let p1 = create_dummy_project(0.0, 10.0);
        let p2 = create_dummy_project(0.0, 10.0);

        let id1 = state.add_project("Sample 1".to_string(), None, p1);
        let _id2 = state.add_project("Sample 2".to_string(), None, p2);

        // 初期スケール値確認
        assert_eq!(state.items[0].y_scale, 100.0);
        assert_eq!(state.items[1].y_scale, 100.0);

        // id1 のみ選択してスケール調整
        state.select_single(id1.clone());
        state.adjust_selected_y_scale(1.2);
        assert!((state.items[0].y_scale - 120.0).abs() < 1e-4);
        assert_eq!(state.items[1].y_scale, 100.0);

        // 全選択してさらにスケール調整 (各自の現在の値に対して個別に倍率を掛ける)
        state.select_all();
        state.adjust_selected_y_scale(1.5);
        assert!((state.items[0].y_scale - 180.0).abs() < 1e-4);
        assert!((state.items[1].y_scale - 150.0).abs() < 1e-4);

        // Y-Offset (ベースライン位置) の個別処理
        state.adjust_selected_y_offset(10.0);
        assert_eq!(state.items[0].y_offset, 10.0);
        assert_eq!(state.items[1].y_offset, 10.0);
    }

    #[test]
    fn test_multispec_batch_move_items() {
        let mut state = MultiSpecState::new();
        let p1 = create_dummy_project(0.0, 10.0);
        let p2 = create_dummy_project(0.0, 10.0);
        let p3 = create_dummy_project(0.0, 10.0);
        let p4 = create_dummy_project(0.0, 10.0);

        let id1 = state.add_project("Sample 1".to_string(), None, p1);
        let id2 = state.add_project("Sample 2".to_string(), None, p2);
        let id3 = state.add_project("Sample 3".to_string(), None, p3);
        let id4 = state.add_project("Sample 4".to_string(), None, p4);

        // [id2, id3] を先頭 (slot 0) へ一括移動
        let mut move_ids = HashSet::new();
        move_ids.insert(id2.clone());
        move_ids.insert(id3.clone());

        assert!(state.move_items_to(&move_ids, 0));
        assert_eq!(state.items[0].id, id2);
        assert_eq!(state.items[1].id, id3);
        assert_eq!(state.items[2].id, id1);
        assert_eq!(state.items[3].id, id4);

        // Undo で復元確認
        assert!(state.undo());
        assert_eq!(state.items[0].id, id1);
        assert_eq!(state.items[1].id, id2);
        assert_eq!(state.items[2].id, id3);
        assert_eq!(state.items[3].id, id4);

        // 飛び飛びの [id1, id3] を末尾 (slot 4) へ一括移動 -> [id2, id4, id1, id3]
        let mut move_ids2 = HashSet::new();
        move_ids2.insert(id1.clone());
        move_ids2.insert(id3.clone());

        assert!(state.move_items_to(&move_ids2, 4));
        assert_eq!(state.items[0].id, id2);
        assert_eq!(state.items[1].id, id4);
        assert_eq!(state.items[2].id, id1);
        assert_eq!(state.items[3].id, id3);
    }

    #[test]
    fn test_multispec_four_items_independent_selection() {
        let mut state = MultiSpecState::new();
        let p1 = create_dummy_project(0.0, 10.0);
        let p2 = create_dummy_project(0.0, 10.0);
        let p3 = create_dummy_project(0.0, 10.0);
        let p4 = create_dummy_project(0.0, 10.0);

        let id1 = state.add_project("Sample 1".to_string(), None, p1);
        let id2 = state.add_project("Sample 2".to_string(), None, p2);
        let id3 = state.add_project("Sample 3".to_string(), None, p3);
        let id4 = state.add_project("Sample 4".to_string(), None, p4);

        // 全アイテムの ID が一意であることを確認
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id1, id4);
        assert_ne!(id2, id3);
        assert_ne!(id2, id4);
        assert_ne!(id3, id4);

        // 1 と 3 を複数選択
        state.select_single(id1.clone());
        state.toggle_select(&id3);
        assert!(state.is_selected(&id1));
        assert!(!state.is_selected(&id2));
        assert!(state.is_selected(&id3));
        assert!(!state.is_selected(&id4));

        // この状態で 2 を単一クリック選択
        state.select_single(id2.clone());
        // 2 だけが選択され、1, 3, 4 は一切選択されていないことを確認 (2, 4 が選択されるバグの再発防止)
        assert!(!state.is_selected(&id1));
        assert!(state.is_selected(&id2));
        assert!(!state.is_selected(&id3));
        assert!(!state.is_selected(&id4));
        assert_eq!(state.selected_ids.len(), 1);

        // 2 と 4 を選択
        state.toggle_select(&id4);
        assert!(state.is_selected(&id2));
        assert!(state.is_selected(&id4));
        assert_eq!(state.selected_ids.len(), 2);

        // 解析プロパティ変更 (タイトル変更など) を履歴に保存
        state.title = "Step 1".to_string();
        state.push_history();

        // 別の解析プロパティ変更 + 選択を 3 に変更
        state.title = "Step 2".to_string();
        state.select_single(id3.clone());
        state.push_history();
        assert!(state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 1);

        // Undo -> 解析プロパティ (title) は "Step 1" に戻るが、
        // 選択状態は解析内容ではないため、Undo で過去の選択に戻されず現在の選択 (id3) が維持されること
        assert!(state.undo());
        assert_eq!(state.title, "Step 1");
        assert!(state.is_selected(&id3));
        assert_eq!(state.selected_ids.len(), 1);
    }
}

