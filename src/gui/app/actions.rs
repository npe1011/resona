use std::path::{Path, PathBuf};
use egui::{Context, Pos2, Rect};

#[derive(Debug, Clone)]
pub enum FileDialogResult {
    OpenFile(PathBuf),
    SaveFile(PathBuf),
    SaveCancelled,
}
use crate::core::{
    auto_detect_integrations, compute_integral, pick_peaks, MultiviewItem, RectF,
};
use crate::gui::panels::ActionEvent;
use super::ResonaApp;

/// Multiview のサイズ計算
pub fn compute_multiview_size(
    src_x_min: f64,
    src_x_max: f64,
    view_ppm_span: f64,
    plot_width: f32,
    ratio: f64,
) -> (f32, f32) {
    let dx = (src_x_max - src_x_min).abs();
    let w_main = (dx / view_ppm_span.max(1e-6)) * (plot_width as f64);
    // Ratio = 5.0 を基準スケール(1.0)とし、どんな狭いピークでも Ratio に正比例してサイズが変化するように補正
    let base_scale = ((ratio / 5.0) as f32).max(0.1);
    let min_w = 140.0_f32 * base_scale;
    let calc_w = (w_main * ratio * 4.0) as f32;
    let w = calc_w.max(min_w).clamp(60.0, 950.0);
    let h = (w * 0.72).clamp(50.0, 650.0);
    (w, h)
}

/// 既存の拡大図に被らないように、プロット領域の左上から順に空き位置を行ベースで探索 (常に左から右へ整列)
pub fn find_non_overlapping_multiview_pos(
    existing: &[MultiviewItem],
    plot_rect: Rect,
    w: f32,
    h: f32,
) -> Pos2 {
    let start_x = plot_rect.min.x + 15.0;
    let start_y = plot_rect.min.y + 15.0;
    let max_x = plot_rect.max.x - 15.0;
    let max_y = (plot_rect.max.y - 45.0).max(start_y + h);
    let gap = 15.0_f32;

    if existing.is_empty() {
        return Pos2::new(start_x, start_y);
    }

    // 既存インセットを行ごとにグループ化 (Y座標がおおむね近いものを同じ行とする)
    let mut sorted_existing = existing.to_vec();
    sorted_existing.sort_by(|a, b| {
        let dy = a.geometry.y.partial_cmp(&b.geometry.y).unwrap_or(std::cmp::Ordering::Equal);
        if dy == std::cmp::Ordering::Equal {
            a.geometry.x.partial_cmp(&b.geometry.x).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            dy
        }
    });

    let mut rows: Vec<Vec<MultiviewItem>> = Vec::new();
    for mv in sorted_existing {
        if let Some(last_row) = rows.last_mut() {
            let row_y = last_row[0].geometry.y;
            if (mv.geometry.y - row_y).abs() < 40.0 {
                last_row.push(mv);
                continue;
            }
        }
        rows.push(vec![mv]);
    }

    // 各行を X 座標昇順 (左から右) にソート
    for row in rows.iter_mut() {
        row.sort_by(|a, b| a.geometry.x.partial_cmp(&b.geometry.x).unwrap_or(std::cmp::Ordering::Equal));
    }

    // 1. 各既存行の中で、途中の空きまたは行の右端に置けるか左から右へチェック
    for row in &rows {
        let mut cur_x = start_x;
        let row_y = row[0].geometry.y;
        for mv in row {
            if cur_x + w + gap <= mv.geometry.x {
                // 左側の空きスロットを発見！
                return Pos2::new(cur_x, row_y);
            }
            cur_x = mv.geometry.x + mv.geometry.w + gap;
        }

        // 行の右端に置けるか
        if cur_x + w <= max_x {
            return Pos2::new(cur_x, row_y);
        }
    }

    // 2. 既存の行に入らなければ、新しい行を一番下に作成 (常に左端 start_x から開始！)
    let max_bottom_y = existing
        .iter()
        .map(|mv| mv.geometry.y + mv.geometry.h)
        .fold(start_y, f32::max);

    let new_row_y = max_bottom_y + gap;
    if new_row_y + h <= max_y {
        Pos2::new(start_x, new_row_y)
    } else {
        // 画面全体が埋まっている場合は左上から少しずらして重ねる
        let offset = ((existing.len() as f32) * 20.0) % 80.0;
        Pos2::new(start_x + offset, start_y + offset)
    }
}

impl ResonaApp {
    /// アプリ設定 (~/.resona/settings.json) を保存
    pub fn save_app_settings(&self) {
        let settings = crate::gui::config::AppSettings {
            current_directory: self.current_directory.clone(),
            ppm_decimals: self.plot_style.ppm_decimals,
            integral_decimals: self.plot_style.integral_decimals,
            multiview_ratio: self.action_state.multiview_ratio,
            multiview_auto_align: self.action_state.multiview_auto_align,
            full_auto_baseline_choice: self.full_auto_dialog_state.baseline_choice,
            full_auto_airpls_lambda: self.full_auto_dialog_state.airpls_log_lambda,
            full_auto_poly_order: self.full_auto_dialog_state.poly_order,
            full_auto_integration: self.full_auto_dialog_state.enable_integration,
            print_settings: self.print_dialog_state.settings.clone(),
        };
        if let Err(e) = settings.save() {
            eprintln!("Warning: Failed to save app settings: {}", e);
        }
    }

    /// 全設定をデフォルトに初期化
    pub fn reset_settings_to_default(&mut self) {
        let def = crate::gui::config::AppSettings::default();
        self.plot_style.ppm_decimals = def.ppm_decimals;
        self.plot_style.integral_decimals = def.integral_decimals;
        self.display_dialog_state.ppm_decimals = def.ppm_decimals;
        self.display_dialog_state.integral_decimals = def.integral_decimals;
        self.action_state.multiview_ratio = def.multiview_ratio;
        self.action_state.multiview_auto_align = def.multiview_auto_align;
        self.full_auto_dialog_state.baseline_choice = def.full_auto_baseline_choice;
        self.full_auto_dialog_state.airpls_log_lambda = def.full_auto_airpls_lambda;
        self.full_auto_dialog_state.poly_order = def.full_auto_poly_order;
        self.full_auto_dialog_state.enable_integration = def.full_auto_integration;
        self.print_dialog_state.settings = def.print_settings.clone();
        if let Err(e) = def.save() {
            eprintln!("Warning: Failed to reset settings to default: {}", e);
        }
        self.status_message = "Settings reset to default".to_string();
    }

    /// 化学シフトのリファレンスを設定し、表示範囲とズーム履歴も連動して追従させる
    pub fn apply_shift_reference(&mut self, current_ppm: f64, target_ppm: f64) {
        let shift = target_ppm - current_ppm;
        self.project.set_shift_reference(current_ppm, target_ppm);
        if let Some(ref mut t) = self.transform {
            t.ppm_min += shift;
            t.ppm_max += shift;
        }
        for (z_min, z_max, _, _) in &mut self.zoom_history {
            *z_min += shift;
            *z_max += shift;
        }
        self.is_dirty = true;
    }

    /// 履歴に現在の解析状態を記録し、未保存フラグを立てる
    pub fn push_history(&mut self) {
        self.project.push_history();
        self.is_dirty = true;
    }

    /// ファイルまたはディレクトリを開く
    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) {
        let p = path.as_ref();
        let abs_path = if p.is_relative() {
            std::env::current_dir().unwrap_or_default().join(p)
        } else {
            p.to_path_buf()
        };

        let ext = abs_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let file_name = abs_path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let is_bruker_file = file_name == "fid"
            || file_name == "ser"
            || file_name == "acqus"
            || file_name == "acqu"
            || file_name == "audita.txt";
        let is_dir = abs_path.is_dir();

        let is_raw_data = ext == "jdf" || is_bruker_file || is_dir;

        let res = if ext == "jdf" {
            self.project.load_jdf(&abs_path, None)
        } else if ext == "rsn" {
            self.project.load_rsn(&abs_path)
        } else if is_bruker_file || is_dir || abs_path.join("fid").is_file() || abs_path.join("1/fid").is_file() {
            self.project.load_bruker(&abs_path, None)
        } else {
            self.status_message = format!("Unsupported file format: {}", abs_path.display());
            return;
        };

        match res {
            Ok(_) => {
                // ウィンドウタイトル用の current_file_path の決定:
                // 生データ Bruker の場合はディレクトリパスを保持
                let current_path = if is_bruker_file {
                    let parent = abs_path.parent().unwrap_or(&abs_path);
                    let parent_name = parent.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if !parent_name.is_empty() && parent_name.chars().all(|c| c.is_ascii_digit()) {
                        parent.parent().unwrap_or(parent).to_path_buf()
                    } else {
                        parent.to_path_buf()
                    }
                } else if is_dir {
                    let dir_name = abs_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if !dir_name.is_empty() && dir_name.chars().all(|c| c.is_ascii_digit()) {
                        abs_path.parent().unwrap_or(&abs_path).to_path_buf()
                    } else {
                        abs_path.clone()
                    }
                } else {
                    abs_path.clone()
                };
                self.current_file_path = Some(current_path.clone());

                // 保存先初期ディレクトリ current_directory:
                // Bruker の場合は「データディレクトリの親ディレクトリ」
                // JEOL や RSN の場合は「ファイルの親ディレクトリ」
                if is_bruker_file || is_dir {
                    self.current_directory = current_path.parent().map(|p| p.to_path_buf());
                } else if let Some(parent) = abs_path.parent() {
                    self.current_directory = Some(parent.to_path_buf());
                }
                self.save_app_settings();

                let is_13c = self.project.metadata.nucleus.contains("13C") || self.project.metadata.nucleus.contains("C13");
                self.action_state.ref_target_ppm = if is_13c { 77.16 } else { 7.26 };
                self.action_state.ref_solvent_idx = 0;
                self.action_state.ref_set_active = false;
                self.sync_action_bar_from_project();
                self.zoom_history.clear();
                self.reset_zoom();

                if ext == "rsn" {
                    let ds = &self.project.state.display_settings;
                    if let (Some(x_min), Some(x_max)) = (ds.x_min, ds.x_max) {
                        if let Some(ref mut t) = self.transform {
                            t.ppm_min = x_min;
                            t.ppm_max = x_max;
                        }
                    }
                    if let Some(y_min) = ds.y_min_scale {
                        self.y_min_scale = y_min;
                    }
                    if let Some(y_max) = ds.y_max_scale {
                        self.y_max_scale = y_max;
                    }
                    if let Some(ref mut t) = self.transform {
                        if let Some(spec) = &self.project.spectrum_real {
                            let max_intensity = spec.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(1e-6);
                            let top_pct = self.y_max_scale.max(1.0);
                            t.y_max = max_intensity * (100.0 / top_pct);
                            t.y_min = -max_intensity * (self.y_min_scale / 100.0);
                            self.last_transform_y = Some((t.y_min, t.y_max));
                        }
                    }
                }
                self.is_dirty = false;
                self.status_message = format!("Loaded {}", abs_path.display());

                // 生データ読み込み時はFTダイアログを開き、パラメータを選べるようにする
                if is_raw_data {
                    self.ft_dialog_state.settings = self.project.state.ft_settings.clone();
                    self.ft_dialog_state.reset_preview();
                    self.ft_dialog_state.open = true;
                }
            }
            Err(e) => {
                self.status_message = format!("Error loading file {}", e);
            }
        }
    }

    /// プロジェクトの状態 (ベースライン補正等) をアクションバー状態に同期
    pub fn sync_action_bar_from_project(&mut self) {
        match self.project.state.baseline_method {
            crate::core::baseline::BaselineMethod::AirPLS { log_lambda, .. } => {
                self.action_state.baseline_method_kind = 0;
                self.action_state.baseline_log_lambda = log_lambda;
                self.action_state.baseline_applied = true;
            }
            crate::core::baseline::BaselineMethod::Polynomial { order, .. } => {
                self.action_state.baseline_method_kind = 1;
                self.action_state.baseline_poly_order = order;
                self.action_state.baseline_applied = true;
            }
            crate::core::baseline::BaselineMethod::None => {
                self.action_state.baseline_applied = false;
            }
        }
        if let Some(th) = self.project.state.peak_threshold {
            self.action_state.peak_threshold = th;
        }
        if let Some(target) = self.project.state.reference_point {
            self.action_state.ref_target_ppm = target;
        }
    }

    /// ファイル保存 (.rsn)
    pub fn save_file<P: AsRef<Path>>(&mut self, path: P) -> bool {
        let p = path.as_ref();
        let abs_path = if p.is_relative() {
            std::env::current_dir().unwrap_or_default().join(p)
        } else {
            p.to_path_buf()
        };

        if let Some(ref t) = self.transform {
            self.project.state.display_settings.x_min = Some(t.ppm_min);
            self.project.state.display_settings.x_max = Some(t.ppm_max);
            self.project.state.display_settings.y_min_scale = Some(self.y_min_scale);
            self.project.state.display_settings.y_max_scale = Some(self.y_max_scale);
        }

        match self.project.save_rsn(&abs_path) {
            Ok(_) => {
                // 一回でも保存した状態は rsn ファイルのフルパスに切り替える
                self.current_file_path = Some(abs_path.clone());
                if let Some(parent) = abs_path.parent() {
                    self.current_directory = Some(parent.to_path_buf());
                }
                self.is_dirty = false;
                self.save_app_settings();
                self.status_message = format!("Saved project {}", abs_path.display());
                true
            }
            Err(e) => {
                eprintln!("Error saving project {}", e);
                self.status_message = format!("Error saving project {} ({})", abs_path.display(), e);
                false
            }
        }
    }

    /// ズームリセット (全体表示)
    pub fn reset_zoom(&mut self) {
        if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
            if ppm.is_empty() || spec.is_empty() {
                return;
            }
            let (p_min, p_max) = (ppm[ppm.len() - 1], ppm[0]); // ppmは通常降順
            let mut y_min = f64::MAX;
            let mut y_max = f64::MIN;
            for &y in spec.iter() {
                if y < y_min { y_min = y; }
                if y > y_max { y_max = y; }
            }
            if y_min >= y_max {
                y_min = 0.0;
                y_max = 1.0;
            }
            let y_span = y_max - y_min;
            let eff_y_min = y_min - y_span * 0.05;
            let eff_y_max = y_max + y_span * 0.1;

            if let Some(ref mut t) = self.transform {
                t.ppm_min = p_min.min(p_max);
                t.ppm_max = p_min.max(p_max);
                t.y_min = eff_y_min;
                t.y_max = eff_y_max;

                let span = (t.ppm_max - t.ppm_min).abs();
                let is_13c = self.project.metadata.nucleus.to_uppercase().contains("13C")
                    || self.project.metadata.nucleus.to_uppercase().contains("C13");
                let default_step = if is_13c || (span >= 30.0 && span <= 300.0) {
                    10.0
                } else if span < 5.0 {
                    0.5
                } else if span < 30.0 {
                    1.0
                } else if span <= 600.0 {
                    20.0
                } else {
                    50.0
                };
                self.plot_style.tick_major = default_step;
            }
        }
    }

    /// 既存の拡大図について、枠全体の縦の大きさ（h）を一番左上にあるものに揃え、
    /// アスペクト比（w/h）を維持して拡大・縮小する (内部用: 履歴pushなし)
    pub fn adjust_y_multiviews_internal(&mut self) -> Option<f32> {
        if self.project.state.multiviews.is_empty() {
            return None;
        }

        // 一番左上にあるインセット（y 最小、近接行なら x 最小）
        let target_mv = self.project.state.multiviews.iter().min_by(|a, b| {
            if (a.geometry.y - b.geometry.y).abs() < 10.0 {
                a.geometry.x.partial_cmp(&b.geometry.x).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.geometry.y.partial_cmp(&b.geometry.y).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        let target_h = target_mv.map(|mv| mv.geometry.h).unwrap_or(0.0);
        if target_h <= 0.0 {
            return None;
        }

        for mv in &mut self.project.state.multiviews {
            if mv.geometry.h > 0.0 {
                mv.geometry.h = target_h;
            }
        }

        Some(target_h)
    }

    /// 既存の拡大図について、枠全体の縦の大きさ（h）を一番左上のものに揃え、
    /// アスペクト比（w/h）を維持して拡大・縮小する (Adjust-Y)
    pub fn adjust_y_multiviews(&mut self) {
        if let Some(target_h) = self.adjust_y_multiviews_internal() {
            self.push_history();
            self.status_message = format!("Adjusted multiview heights to top-left {:.0}px", target_h);
        }
    }

    /// 既存の Multiview を最大化学シフト降順にソートし、画面上部に下揃えで整列配置 (内部用: 履歴pushなし)
    pub fn align_multiviews_internal(&mut self, plot_rect: Rect) {
        if self.project.state.multiviews.is_empty() {
            return;
        }

        // 最大化学シフト降順にソート (ezNMR準拠)
        self.project.state.multiviews.sort_by(|a, b| {
            let max_a = a.src_x_min.max(a.src_x_max);
            let max_b = b.src_x_min.max(b.src_x_max);
            max_b.partial_cmp(&max_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        let start_x = plot_rect.min.x + 15.0;
        let start_y = plot_rect.min.y + 15.0;
        let max_w = (plot_rect.width() - 30.0).max(200.0);
        let mut rows: Vec<Vec<usize>> = Vec::new();
        let mut current_row: Vec<usize> = Vec::new();
        let mut current_offset_x = 0.0_f32;

        for (i, mv) in self.project.state.multiviews.iter().enumerate() {
            let w = mv.geometry.w;
            if current_offset_x + w > max_w && !current_row.is_empty() {
                rows.push(current_row);
                current_row = Vec::new();
                current_offset_x = 0.0;
            }
            current_row.push(i);
            current_offset_x += w + 15.0;
        }
        if !current_row.is_empty() {
            rows.push(current_row);
        }

        let mut current_y = start_y;
        for row in rows {
            let row_h = row
                .iter()
                .map(|&idx| self.project.state.multiviews[idx].geometry.h)
                .fold(0.0_f32, f32::max);

            let mut cur_x = start_x;
            for &idx in &row {
                let h = self.project.state.multiviews[idx].geometry.h;
                let w = self.project.state.multiviews[idx].geometry.w;
                // 下揃え: y = current_y + (row_h - h)
                let y = current_y + (row_h - h);
                self.project.state.multiviews[idx].geometry.x = cur_x;
                self.project.state.multiviews[idx].geometry.y = y;
                cur_x += w + 15.0;
            }
            current_y += row_h + 15.0;
        }
    }

    /// 既存の Multiview を最大化学シフト降順にソートし、画面上部に下揃えで整列配置 (ezNMR準拠)
    pub fn align_multiviews(&mut self, plot_rect: Rect) {
        if self.project.state.multiviews.is_empty() {
            return;
        }
        self.align_multiviews_internal(plot_rect);
        self.push_history();
        self.status_message = "Aligned multiview insets".to_string();
    }

    /// 既存の Multiview を最大化学シフト降順にソートし、画面上部に1行横並びで下揃え整列配置
    /// 全幅がプロット画面幅を超える場合は画面内に収まるよう均等縮小
    pub fn align_multiviews_one_row(&mut self, plot_rect: Rect) {
        if self.project.state.multiviews.is_empty() {
            return;
        }

        // 最大化学シフト降順にソート (ezNMR準拠)
        self.project.state.multiviews.sort_by(|a, b| {
            let max_a = a.src_x_min.max(a.src_x_max);
            let max_b = b.src_x_min.max(b.src_x_max);
            max_b.partial_cmp(&max_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        let start_x = plot_rect.min.x + 15.0;
        let start_y = plot_rect.min.y + 15.0;
        let available_w = (plot_rect.width() - 30.0).max(100.0);
        let count = self.project.state.multiviews.len();
        let gap = 15.0_f32;
        let total_gap = if count > 1 { (count - 1) as f32 * gap } else { 0.0 };
        let sum_w: f32 = self.project.state.multiviews.iter().map(|mv| mv.geometry.w).sum();
        let total_needed = sum_w + total_gap;

        // 画面幅を超える場合は、各拡大図の相対的な幅を維持して縮小 (Rx 倍)。高さは維持
        if total_needed > available_w && sum_w > 0.0 {
            let available_for_insets = (available_w - total_gap).max(count as f32 * 30.0);
            let rx = (available_for_insets / sum_w).clamp(0.05, 1.0);
            for mv in &mut self.project.state.multiviews {
                mv.geometry.w *= rx;
            }
        }

        let max_h = self
            .project
            .state
            .multiviews
            .iter()
            .map(|mv| mv.geometry.h)
            .fold(0.0_f32, f32::max);

        let mut cur_x = start_x;
        for mv in &mut self.project.state.multiviews {
            let h = mv.geometry.h;
            let w = mv.geometry.w;
            // 下揃え: y = start_y + (max_h - h)
            let y = start_y + (max_h - h);
            mv.geometry.x = cur_x;
            mv.geometry.y = y;
            cur_x += w + gap;
        }

        self.push_history();
        self.status_message = "Aligned multiview insets in 1 row".to_string();
    }

    /// 既存の積分区間から Multiview インセットを自動生成 (ezNMR lines 508-535準拠 + 感度フィルタ)
    pub fn auto_create_multiview_from_integrations(&mut self, plot_rect: Rect, view_ppm_span: f64) {
        if self.project.state.integrations.is_empty() {
            self.status_message = "No integrations to create multiviews from".to_string();
            return;
        }

        // 各積分区間の面積を計算して、最大面積を算出 (感度に応じた微小除外)
        let min_ratio = self.project.state.auto_sensitivity.multiview_min_area_ratio();
        let target_integrations: Vec<_> = if min_ratio > 0.0 {
            if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                let mut max_area = 0.0_f64;
                let mut areas = Vec::with_capacity(self.project.state.integrations.len());
                for integ in &self.project.state.integrations {
                    let a = compute_integral(spec, ppm, integ, 1.0, 1.0, 0.0)
                        .map(|r| r.total_area)
                        .unwrap_or(0.0);
                    if a > max_area {
                        max_area = a;
                    }
                    areas.push(a);
                }
                self.project.state.integrations.iter().enumerate()
                    .filter(|(idx, _)| areas[*idx] >= max_area * min_ratio)
                    .map(|(_, item)| item.clone())
                    .collect()
            } else {
                self.project.state.integrations.clone()
            }
        } else {
            self.project.state.integrations.clone()
        };

        if target_integrations.is_empty() {
            self.status_message = "No significant integrations to create multiviews from".to_string();
            return;
        }

        self.project.state.multiviews.clear();
        let ratio = self.action_state.multiview_ratio;
        let plot_width = plot_rect.width();

        for (i, integ) in target_integrations.iter().enumerate() {
            let x1 = integ.min_ppm();
            let x2 = integ.max_ppm();
            let pad = ((x2 - x1) * 0.1).max(0.02);

            let src_min = x1 - pad;
            let src_max = x2 + pad;

            let dx = (src_max - src_min).abs();
            let w_main = (dx / view_ppm_span.max(1e-6)) * (plot_width as f64);
            let raw_w = (w_main * ratio) as f32;
            let w = raw_w.clamp(70.0, (plot_width * 0.9).max(100.0));
            let h = (plot_rect.height() * 0.25).clamp(100.0, 250.0);

            let geom = RectF {
                x: plot_rect.min.x + 20.0 + (i as f32) * 20.0,
                y: plot_rect.min.y + 15.0 + (i as f32) * 20.0,
                w,
                h,
            };

            let m_id = format!("mv-{}", i + 1);
            self.project.state.multiviews.push(MultiviewItem {
                id: m_id,
                src_x_min: src_min,
                src_x_max: src_max,
                src_y_min: None,
                src_y_max: None,
                ratio,
                geometry: geom,
            });
        }

        // Auto で追加したときは必ず Adjust-Y + Align を実行 (履歴は 1-step)
        self.adjust_y_multiviews_internal();
        self.align_multiviews_internal(plot_rect);
        self.push_history();
        self.status_message = format!(
            "Auto-created {} multiview insets from integrals (sensitivity={})",
            self.project.state.multiviews.len(),
            self.project.state.auto_sensitivity.label(),
        );
    }

    /// Peak List ダイアログを開く (低磁場順・ppm降順で半角カンマ+スペース区切り)
    pub fn open_peak_list_dialog(&mut self) {
        let mut sorted_peaks = self.project.state.peaks.clone();
        // 低磁場 (Downfield) から順: ppm の値が大きい順 (降順)
        sorted_peaks.sort_by(|a, b| b.ppm.partial_cmp(&a.ppm).unwrap_or(std::cmp::Ordering::Equal));

        let decimals = self.plot_style.ppm_decimals;
        let formatted_list: Vec<String> = sorted_peaks
            .iter()
            .map(|pk| format!("{:.1$}", pk.ppm, decimals))
            .collect();

        self.peak_list_dialog_state.peak_count = sorted_peaks.len();
        self.peak_list_dialog_state.text = formatted_list.join(", ");
        self.peak_list_dialog_state.copied = false;
        self.peak_list_dialog_state.open = true;
    }

    /// Display Settings ダイアログを開く (相対パーセント、目盛り設定などを初期化)
    pub fn open_display_dialog(&mut self) {
        let max_intensity = self.project.spectrum_real.as_ref()
            .map(|s| s.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(1.0))
            .unwrap_or(1.0);
        self.display_dialog_state.max_peak_intensity = max_intensity;

        if let Some(ref t) = self.transform {
            self.display_dialog_state.ppm_min = t.ppm_min;
            self.display_dialog_state.ppm_max = t.ppm_max;
            let top_pct = if t.y_max > 1e-6 {
                (100.0 * max_intensity / t.y_max).clamp(1.0, 10000.0)
            } else {
                80.0
            };
            let min_pct = if max_intensity > 1e-6 {
                (100.0 * (-t.y_min) / max_intensity).clamp(0.0, 10000.0)
            } else {
                10.0
            };
            self.display_dialog_state.y_max_scale = top_pct;
            self.display_dialog_state.y_min_scale = min_pct;

            let span = (t.ppm_max - t.ppm_min).abs();
            if self.plot_style.tick_major <= 0.0 {
                let is_13c = self.project.metadata.nucleus.to_uppercase().contains("13C")
                    || self.project.metadata.nucleus.to_uppercase().contains("C13");
                let default_step = if is_13c || (span >= 30.0 && span <= 300.0) {
                    10.0
                } else if span < 5.0 {
                    0.5
                } else if span < 30.0 {
                    1.0
                } else if span <= 600.0 {
                    20.0
                } else {
                    50.0
                };
                self.plot_style.tick_major = default_step;
            }
        } else {
            self.display_dialog_state.y_min_scale = 10.0;
            self.display_dialog_state.y_max_scale = 80.0;
        }

        self.display_dialog_state.auto_ticks = self.plot_style.auto_ticks;
        self.display_dialog_state.tick_major = self.plot_style.tick_major;
        self.display_dialog_state.tick_minor = self.plot_style.tick_minor;
        self.display_dialog_state.ppm_decimals = self.plot_style.ppm_decimals;
        self.display_dialog_state.integral_decimals = self.plot_style.integral_decimals;
        self.display_dialog_state.open = true;
    }

    /// 外部ダイアログ経由でデータファイルを開く (Open Data)
    /// 外部ダイアログ経由でデータファイルを開く (Open Data) - バックグラウンドスレッドで実行
    pub fn open_file_dialog(&mut self, ctx: &Context) {
        if self.is_file_dialog_open.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.is_file_dialog_open.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut dialog = rfd::FileDialog::new()
            // All: JEOL (*.jdf), Resona (*.rsn), および Bruker (fid, acqus, acqu, ser) に厳密に限定
            .add_filter("All", &["jdf;*.rsn;fid;acqus;acqu;ser"])
            .add_filter("JEOL Raw FID (*.jdf)", &["jdf"])
            .add_filter("Resona Project (*.rsn)", &["rsn"])
            .add_filter("Bruker Raw FID (fid, acqus)", &["bruker;fid;acqus;acqu;ser"]);

        if let Some(ref dir) = self.current_directory {
            let abs_dir = if dir.is_relative() {
                std::env::current_dir().unwrap_or_default().join(dir)
            } else {
                dir.clone()
            };
            dialog = dialog.set_directory(abs_dir);
        }

        let is_open_flag = std::sync::Arc::clone(&self.is_file_dialog_open);
        let result_slot = std::sync::Arc::clone(&self.file_dialog_result);
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = dialog.pick_file();
            is_open_flag.store(false, std::sync::atomic::Ordering::SeqCst);
            if let Some(path) = res {
                if let Ok(mut lock) = result_slot.lock() {
                    *lock = Some(FileDialogResult::OpenFile(path));
                }
                ctx_clone.request_repaint();
            }
        });
    }

    /// 外部ダイアログ経由でフォルダを開く (Brukerデータセット等) - バックグラウンドスレッドで実行
    pub fn open_folder_dialog(&mut self, ctx: &Context) {
        if self.is_file_dialog_open.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.is_file_dialog_open.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut dialog = rfd::FileDialog::new();

        if let Some(ref dir) = self.current_directory {
            let abs_dir = if dir.is_relative() {
                std::env::current_dir().unwrap_or_default().join(dir)
            } else {
                dir.clone()
            };
            dialog = dialog.set_directory(abs_dir);
        }

        let is_open_flag = std::sync::Arc::clone(&self.is_file_dialog_open);
        let result_slot = std::sync::Arc::clone(&self.file_dialog_result);
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = dialog.pick_folder();
            is_open_flag.store(false, std::sync::atomic::Ordering::SeqCst);
            if let Some(path) = res {
                if let Ok(mut lock) = result_slot.lock() {
                    *lock = Some(FileDialogResult::OpenFile(path));
                }
                ctx_clone.request_repaint();
            }
        });
    }

    /// プロジェクトを保存する (常に Save As 挙動でダイアログを開き、確認なし上書きを防ぐ)
    pub fn handle_save(&mut self, ctx: &Context) {
        self.save_rsn_dialog(ctx);
    }

    /// 外部ダイアログ経由でプロジェクトを保存する (Save as rsn) - バックグラウンドスレッドで実行
    pub fn save_rsn_dialog(&mut self, ctx: &Context) {
        if self.is_file_dialog_open.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.is_file_dialog_open.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut dialog = rfd::FileDialog::new()
            .set_title("Save as RSN")
            .add_filter("Resona Project (*.rsn)", &["rsn"]);

        if let Some(ref dir) = self.current_directory {
            let abs_dir = if dir.is_relative() {
                std::env::current_dir().unwrap_or_default().join(dir)
            } else {
                dir.clone()
            };
            dialog = dialog.set_directory(abs_dir);
        }

        // ファイル名の初期値: 元データと同じ名前 (Brukerの名前はディレクトリ名)
        if let Some(ref cur) = self.current_file_path {
            let base_name = if cur.is_dir() {
                cur.file_name().and_then(|s| s.to_str()).unwrap_or("spectrum")
            } else {
                cur.file_stem().and_then(|s| s.to_str()).unwrap_or("spectrum")
            };
            dialog = dialog.set_file_name(format!("{}.rsn", base_name));
        }

        let is_open_flag = std::sync::Arc::clone(&self.is_file_dialog_open);
        let result_slot = std::sync::Arc::clone(&self.file_dialog_result);
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = dialog.save_file();
            is_open_flag.store(false, std::sync::atomic::Ordering::SeqCst);
            if let Some(mut path) = res {
                if path.extension().is_none() {
                    path.set_extension("rsn");
                }
                if let Ok(mut lock) = result_slot.lock() {
                    *lock = Some(FileDialogResult::SaveFile(path));
                }
            } else if let Ok(mut lock) = result_slot.lock() {
                *lock = Some(FileDialogResult::SaveCancelled);
            }
            ctx_clone.request_repaint();
        });
    }

    /// ファイル D&D の処理
    pub(crate) fn handle_drag_and_drop(&mut self, ctx: &Context) {
        if self.has_open_dialog() {
            return;
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                self.open_file(path);
                break;
            }
        }
    }

    /// アクションバーからのイベント処理
    pub(crate) fn handle_action_event(&mut self, _ctx: &Context, action_event: ActionEvent) {
        match action_event {
            ActionEvent::None => {}
            ActionEvent::ResetZoom => self.reset_zoom(),
            ActionEvent::UndoZoom => {
                if let Some((p_min, p_max, y_min, y_max)) = self.zoom_history.pop() {
                    if let Some(ref mut t) = self.transform {
                        t.ppm_min = p_min;
                        t.ppm_max = p_max;
                        t.y_min = y_min;
                        t.y_max = y_max;
                    }
                }
            }
            ActionEvent::AutoPhase => {
                let (new_p0, new_p1) = self.project.auto_phase();
                self.push_history();
                self.status_message = format!(
                    "ACME Autophase applied with Pivot at {:.3} ppm (P0={:.2}°, P1={:.2}°)",
                    self.project.pivot_ppm(), new_p0, new_p1
                );
            }
            ActionEvent::AutoPivot => {
                if let Some(ppm) = self.project.auto_pivot() {
                    self.push_history();
                    self.status_message = format!("Auto Pivot set to {:.3} ppm", ppm);
                }
            }
            ActionEvent::ApplyBaseline { method } => {
                self.project.apply_baseline(method);
                self.push_history();
                self.action_state.baseline_applied = true;
                let method_desc = match method {
                    crate::core::baseline::BaselineMethod::AirPLS { log_lambda, .. } => {
                        format!("airPLS (log10 λ = {:.1})", log_lambda)
                    }
                    crate::core::baseline::BaselineMethod::Polynomial { order, .. } => {
                        format!("Polynomial (order = {})", order)
                    }
                    crate::core::baseline::BaselineMethod::None => "None".to_string(),
                };
                self.status_message = format!("Baseline corrected: {}", method_desc);
            }
            ActionEvent::ClearBaseline => {
                self.project.clear_baseline();
                self.push_history();
                self.action_state.baseline_applied = false;
                self.status_message = "Baseline correction cleared".to_string();
            }
            ActionEvent::AutoReference => {
                let target = self.action_state.ref_target_ppm;
                let nuc = &self.project.metadata.nucleus;
                let is_1h = nuc.contains("1H") || nuc.is_empty() || nuc.contains("H1");
                let delta = if is_1h { 0.10 } else { 1.00 };
                let search_min = target - delta;
                let search_max = target + delta;

                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                    let mut best_p = None;
                    let mut max_val = f64::NEG_INFINITY;
                    for i in 0..ppm.len().min(spec.len()) {
                        let p = ppm[i];
                        if p >= search_min && p <= search_max {
                            let v = spec[i];
                            if v > max_val {
                                max_val = v;
                                best_p = Some(p);
                            }
                        }
                    }

                    let noise = self.project.noise_level();
                    if let Some(peak_ppm) = best_p {
                        if max_val > noise * 2.0 {
                            self.apply_shift_reference(peak_ppm, target);
                            self.push_history();
                            self.status_message = format!(
                                "Auto referenced: {:.3} ppm -> {:.3} ppm (Δ = {:+.3} ppm)",
                                peak_ppm,
                                target,
                                target - peak_ppm
                            );
                        } else {
                            self.status_message = format!(
                                "Auto reference failed: No significant peak found within ±{:.2} ppm of {:.3} ppm (intensity={:.1}, noise={:.1})",
                                delta, target, max_val, noise
                            );
                        }
                    } else {
                        self.status_message = format!(
                            "Auto reference failed: No data points within ±{:.2} ppm of {:.3} ppm",
                            delta, target
                        );
                    }
                }
            }
            ActionEvent::ApplyShiftReference { peak_ppm, target_ppm } => {
                self.apply_shift_reference(peak_ppm, target_ppm);
                self.push_history();
                self.status_message = format!("Referenced peak at {:.3} ppm -> {:.3} ppm", peak_ppm, target_ppm);
            }
            ActionEvent::AutoPeak => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    let noise = self.project.noise_level();
                    let factor = self.project.state.auto_sensitivity.peak_noise_factor();
                    let thresh = noise * factor;
                    self.action_state.peak_threshold = thresh;
                    self.project.state.peak_threshold = Some(thresh);
                    self.project.state.peaks = pick_peaks(spec, ppm, thresh, &self.project.state.peaks);
                    self.push_history();
                    self.status_message = format!(
                        "Auto detected {} peaks (thresh={:.1}, sensitivity={})",
                        self.project.state.peaks.len(),
                        thresh,
                        self.project.state.auto_sensitivity.label(),
                    );
                }
            }
            ActionEvent::PickPeaks { threshold } => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    self.action_state.peak_threshold = threshold;
                    self.project.state.peak_threshold = Some(threshold);
                    self.project.state.peaks = pick_peaks(spec, ppm, threshold, &self.project.state.peaks);
                    self.push_history();
                    self.status_message = format!("Picked {} peaks with threshold {:.1}", self.project.state.peaks.len(), threshold);
                }
            }
            ActionEvent::ClearPeaks => {
                self.project.state.peaks.clear();
                self.push_history();
                self.status_message = "All peaks cleared".to_string();
            }
            ActionEvent::AutoIntegrate => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    self.project.state.integrations = auto_detect_integrations(spec, ppm, self.project.state.auto_sensitivity);
                    let mut max_area = 0.0_f64;
                    for intg in &self.project.state.integrations {
                        if let Some(res) = compute_integral(spec, ppm, intg, 1.0, 1.0, 0.03) {
                            if res.total_area > max_area {
                                max_area = res.total_area;
                            }
                        }
                    }
                    let max_spec = self.project.max_intensity();
                    if max_area > 1e-12 && max_spec > 0.0 {
                        self.project.state.integration_scale = (max_spec * 0.35) / max_area;
                        self.project.state.integration_ref_area = max_area;
                        self.project.state.integration_ref_value = 1.0;
                    }
                    self.push_history();
                    self.status_message = format!(
                        "Auto detected {} integration regions (sensitivity={})",
                        self.project.state.integrations.len(),
                        self.project.state.auto_sensitivity.label(),
                    );
                }
            }
            ActionEvent::ClearIntegrations => {
                self.project.state.integrations.clear();
                self.push_history();
                self.status_message = "All integrations cleared".to_string();
            }
            ActionEvent::AutoMultiview => {
                let plot_rect = self.transform.as_ref().map(|t| t.screen_rect).unwrap_or(Rect::from_min_size(Pos2::new(100.0, 100.0), egui::vec2(800.0, 600.0)));
                let ppm_span = self.transform.as_ref().map(|t| (t.ppm_max - t.ppm_min).abs()).unwrap_or(10.0);
                self.auto_create_multiview_from_integrations(plot_rect, ppm_span);
            }
            ActionEvent::AdjustYMultiview => {
                self.adjust_y_multiviews();
            }
            ActionEvent::AlignMultiview => {
                let plot_rect = self.transform.as_ref().map(|t| t.screen_rect).unwrap_or(Rect::from_min_size(Pos2::new(100.0, 100.0), egui::vec2(800.0, 600.0)));
                self.align_multiviews(plot_rect);
            }
            ActionEvent::AlignMultiviewsOneRow => {
                let plot_rect = self.transform.as_ref().map(|t| t.screen_rect).unwrap_or(Rect::from_min_size(Pos2::new(100.0, 100.0), egui::vec2(800.0, 600.0)));
                self.align_multiviews_one_row(plot_rect);
            }
            ActionEvent::ResetMultiview => {
                self.project.state.multiviews.clear();
                self.selected_multiview_ids.clear();
                self.push_history();
                self.status_message = "All multiview insets cleared".to_string();
            }
            ActionEvent::ClearJCoupling => {
                self.project.state.j_couplings.clear();
                self.selected_j_idx = None;
                self.push_history();
                self.status_message = "All J-coupling results cleared".to_string();
            }
            ActionEvent::CloseMode => {
                self.mode = None;
                self.action_state.clear_submodes();
                self.status_message = "Mode closed".to_string();
            }
        }
    }
}
