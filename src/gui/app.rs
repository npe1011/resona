use std::path::{Path, PathBuf};
use egui::{CentralPanel, Color32, Context, Key, Margin, Pos2, Rect, RichText, Stroke, TopBottomPanel, SidePanel};
use crate::core::{
    analyze_multiplet, auto_detect_integrations, compute_integral, pick_peaks,
    add_peak_in_range, IntegrationItem, JCouplingResultItem, MultiviewItem, Project, RectF,
};

use crate::gui::dialogs::{
    show_display_dialog, show_ft_dialog, show_jcoupling_dialog, show_print_dialog,
    DisplayDialogState, FtDialogState, JCouplingDialogState, PrintDialogState,
};
use crate::gui::mode::{AppMode, IntegrateSubMode, MultiviewSubMode, PeakSubMode, ZoomTool};
use crate::gui::panels::{
    show_action_bar, show_mode_bar, show_side_panel, ActionEvent, ActionBarState, ModeBarEvent,
};
use crate::gui::plot::{paint_spectrum, PlotStyle, PlotTransform};

/// Integrate Edit モードでのドラッグ対象
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntegrateDragTarget {
    StartHandle(usize),
    EndHandle(usize),
    Scale { start_scale: f64, start_y: f32 },
    Offset { start_offset: f64, start_y: f32 },
}

/// Multiview Edit モードでのドラッグ対象
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MultiviewDragMode {
    Move,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone)]
pub struct MultiviewDragState {
    pub item_id: String,
    pub mode: MultiviewDragMode,
    pub start_rect: RectF,
    pub start_pointer: Pos2,
}

/// Multiview インセット枠のマウス位置からドラッグモードを正確に判定する
/// 外側 8px から内側 14px までの枠線ゾーンを検出し、4隅・4辺のハンドルを優先判定
fn detect_multiview_drag_mode(pos: Pos2, geom: &RectF) -> Option<MultiviewDragMode> {
    let rect = Rect::from_min_size(Pos2::new(geom.x, geom.y), egui::vec2(geom.w, geom.h));
    // 外側 8px まで許容
    if !rect.expand(8.0).contains(pos) {
        return None;
    }

    // 4隅の優先判定 (各角から半径 14px)
    let d_tl = (pos - rect.left_top()).length();
    let d_tr = (pos - rect.right_top()).length();
    let d_bl = (pos - rect.left_bottom()).length();
    let d_br = (pos - rect.right_bottom()).length();
    let corner_radius = 14.0_f32;

    if d_tl < corner_radius { return Some(MultiviewDragMode::TopLeft); }
    if d_tr < corner_radius { return Some(MultiviewDragMode::TopRight); }
    if d_bl < corner_radius { return Some(MultiviewDragMode::BottomLeft); }
    if d_br < corner_radius { return Some(MultiviewDragMode::BottomRight); }

    // 4辺の判定 (外側 8px 〜 内側 12px)
    let left_zone = pos.x <= rect.min.x + 12.0;
    let right_zone = pos.x >= rect.max.x - 12.0;
    let top_zone = pos.y <= rect.min.y + 12.0;
    let bottom_zone = pos.y >= rect.max.y - 12.0;

    if left_zone && top_zone { Some(MultiviewDragMode::TopLeft) }
    else if right_zone && top_zone { Some(MultiviewDragMode::TopRight) }
    else if left_zone && bottom_zone { Some(MultiviewDragMode::BottomLeft) }
    else if right_zone && bottom_zone { Some(MultiviewDragMode::BottomRight) }
    else if left_zone { Some(MultiviewDragMode::Left) }
    else if right_zone { Some(MultiviewDragMode::Right) }
    else if top_zone { Some(MultiviewDragMode::Top) }
    else if bottom_zone { Some(MultiviewDragMode::Bottom) }
    else { Some(MultiviewDragMode::Move) }
}

/// PPM 範囲とプロット幅、および Ratio 倍率に基づいてインセットの推奨ピクセルサイズ (w, h) を算出
/// Ratio = 5.0 を標準とし、Ratio にダイレクトに正比例して拡大・縮小
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

pub struct ResonaApp {
    pub project: Project,
    pub current_file_path: Option<PathBuf>,
    pub current_directory: Option<PathBuf>,

    pub mode: Option<AppMode>,
    pub active_zoom: Option<ZoomTool>,
    pub action_state: ActionBarState,
    pub ft_dialog_state: FtDialogState,
    pub display_dialog_state: DisplayDialogState,
    pub jcoupling_dialog_state: JCouplingDialogState,
    pub print_dialog_state: PrintDialogState,

    pub plot_style: PlotStyle,
    pub transform: Option<PlotTransform>,
    pub zoom_history: Vec<(f64, f64, f64, f64)>, // (ppm_min, ppm_max, y_min, y_max)

    // ドラッグ状態
    pub drag_start: Option<Pos2>,
    pub drag_current: Option<Pos2>,
    pub is_dragging_threshold: bool,
    pub integrate_drag: Option<IntegrateDragTarget>,
    pub multiview_drag: Option<MultiviewDragState>,

    // 選択状態
    pub selected_multiview_id: Option<String>,
    pub hovered_multiview_id: Option<String>,
    pub selected_j_idx: Option<usize>,

    pub last_multiview_ratio: f64,
    pub status_message: String,
}

impl Default for ResonaApp {
    fn default() -> Self {
        Self {
            project: Project::new(),
            current_file_path: None,
            current_directory: None,
            mode: None,
            active_zoom: None,
            action_state: ActionBarState::default(),
            ft_dialog_state: FtDialogState::default(),
            display_dialog_state: DisplayDialogState::default(),
            jcoupling_dialog_state: JCouplingDialogState::default(),
            print_dialog_state: PrintDialogState::default(),
            plot_style: PlotStyle::default(),
            transform: None,
            zoom_history: Vec::new(),
            drag_start: None,
            drag_current: None,
            is_dragging_threshold: false,
            integrate_drag: None,
            multiview_drag: None,
            selected_multiview_id: None,
            hovered_multiview_id: None,
            selected_j_idx: None,
            last_multiview_ratio: 5.0,
            status_message: "Ready. Drag & drop .jdf or .rsn file here.".to_string(),
        }
    }
}

impl ResonaApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    /// ファイルを開く
    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) {
        let p = path.as_ref();
        let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

        let is_raw_data = ext == "jdf";

        let res = match ext.as_str() {
            "jdf" => self.project.load_jdf(p, None),
            "rsn" | "ez" => self.project.load_rsn(p),
            _ => {
                self.status_message = format!("Unsupported file extension {}", ext);
                return;
            }
        };

        match res {
            Ok(_) => {
                self.current_file_path = Some(p.to_path_buf());
                if let Some(parent) = p.parent() {
                    let abs_parent = if parent.is_relative() {
                        std::env::current_dir().unwrap_or_default().join(parent)
                    } else {
                        parent.to_path_buf()
                    };
                    self.current_directory = Some(abs_parent);
                }
                let is_13c = self.project.metadata.nucleus.contains("13C") || self.project.metadata.nucleus.contains("C13");
                self.action_state.ref_target_ppm = if is_13c { 77.16 } else { 7.26 };
                self.action_state.ref_solvent_idx = 0;
                self.action_state.ref_set_active = false;
                self.zoom_history.clear();
                self.reset_zoom();
                self.status_message = format!("Loaded {}", p.display());

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

    /// ファイル保存 (.rsn)
    pub fn save_file<P: AsRef<Path>>(&mut self, path: P) {
        let p = path.as_ref();
        match self.project.save_rsn(p) {
            Ok(_) => {
                self.current_file_path = Some(p.to_path_buf());
                self.status_message = format!("Saved project {}", p.display());
            }
            Err(e) => {
                eprintln!("Error saving project {}", e);
                self.status_message = format!("Error saving project {} ({})", p.display(), e);
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

    /// 既存の拡大図について、枠全体の縦の大きさ（h）を最大のものに揃え、
    /// アスペクト比（w/h）を維持して拡大・縮小する (Adjust-Y)
    pub fn adjust_y_multiviews(&mut self) {
        if self.project.state.multiviews.is_empty() {
            return;
        }

        let max_h = self
            .project
            .state
            .multiviews
            .iter()
            .map(|mv| mv.geometry.h)
            .fold(0.0_f32, f32::max);

        if max_h <= 0.0 {
            return;
        }

        for mv in &mut self.project.state.multiviews {
            if mv.geometry.h > 0.0 {
                let scale = max_h / mv.geometry.h;
                mv.geometry.w *= scale;
                mv.geometry.h = max_h;
            }
        }

        self.project.push_history();
        self.status_message = format!("Adjusted multiview heights to {:.0}px", max_h);
    }

    /// 既存の Multiview を最大化学シフト降順にソートし、画面上部に下揃えで整列配置 (ezNMR準拠)
    pub fn align_multiviews(&mut self, plot_rect: Rect) {
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

        self.project.push_history();
        self.status_message = "Aligned multiview insets".to_string();
    }

    /// 既存の積分区間から Multiview インセットを自動生成 (ezNMR lines 508-535準拠)
    pub fn auto_create_multiview_from_integrations(&mut self, plot_rect: Rect, view_ppm_span: f64) {
        if self.project.state.integrations.is_empty() {
            self.status_message = "No integrations to create multiviews from".to_string();
            return;
        }

        self.project.state.multiviews.clear();
        let ratio = self.action_state.multiview_ratio;
        let plot_width = plot_rect.width();

        for (i, integ) in self.project.state.integrations.iter().enumerate() {
            let x1 = integ.min_ppm();
            let x2 = integ.max_ppm();
            let pad = ((x2 - x1) * 0.1).max(0.02);

            let src_min = x1 - pad;
            let src_max = x2 + pad;

            // Auto 実行時に Ratio を全体反映 (Ratio = 5.0 を基準幅約 220px とし、Ratio に比例してスケール)
            let scale = ((ratio / 5.0) as f32).max(0.2);
            let dx = (src_max - src_min).abs();
            let w_main = (dx / view_ppm_span.max(1e-6)) * (plot_width as f64);
            let w = ((200.0 + (w_main as f32) * 4.0) * scale).clamp(90.0, 800.0);
            let h = (w * 0.70).clamp(65.0, 560.0);

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

        self.align_multiviews(plot_rect);
        self.status_message = format!("Auto-created {} multiview insets from integrals", self.project.state.multiviews.len());
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
            self.display_dialog_state.y_min_scale = (t.y_min / max_intensity) * 100.0;
            self.display_dialog_state.y_max_scale = (t.y_max / max_intensity) * 100.0;

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
            self.display_dialog_state.y_min_scale = -10.0;
            self.display_dialog_state.y_max_scale = 110.0;
        }

        self.display_dialog_state.auto_ticks = self.plot_style.auto_ticks;
        self.display_dialog_state.tick_major = self.plot_style.tick_major;
        self.display_dialog_state.tick_minor = self.plot_style.tick_minor;
        self.display_dialog_state.ppm_decimals = self.plot_style.ppm_decimals;
        self.display_dialog_state.integral_decimals = self.plot_style.integral_decimals;
        self.display_dialog_state.open = true;
    }

    /// 外部ダイアログ経由でファイルを開く
    pub fn open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("NMR Data (*.jdf, *.rsn, *.ez)", &["jdf", "rsn", "ez"])
            .add_filter("JEOL Raw FID (*.jdf)", &["jdf"])
            .add_filter("Resona Project (*.rsn)", &["rsn", "ez"]);

        if let Some(ref dir) = self.current_directory {
            let abs_dir = if dir.is_relative() {
                std::env::current_dir().unwrap_or_default().join(dir)
            } else {
                dir.clone()
            };
            dialog = dialog.set_directory(abs_dir);
        }

        if let Some(path) = dialog.pick_file() {
            self.open_file(path);
        }
    }

    /// クイック保存 (現在開いているファイルと同じディレクトリに直接 .rsn として保存)
    pub fn quick_save(&mut self) {
        if let Some(ref cur) = self.current_file_path.clone() {
            let rsn_path = if cur.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("rsn")).unwrap_or(false) {
                cur.clone()
            } else {
                cur.with_extension("rsn")
            };
            self.save_file(&rsn_path);
        } else {
            self.save_rsn_dialog();
        }
    }

    /// 外部ダイアログ経由でプロジェクトを保存する (Save As)
    pub fn save_rsn_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Resona Project (*.rsn)", &["rsn"]);

        if let Some(ref dir) = self.current_directory {
            let abs_dir = if dir.is_relative() {
                std::env::current_dir().unwrap_or_default().join(dir)
            } else {
                dir.clone()
            };
            dialog = dialog.set_directory(abs_dir);
        }

        // ファイル名の自動プリセット (拡張子なしで渡す)
        if let Some(ref cur) = self.current_file_path {
            if let Some(stem) = cur.file_stem().and_then(|s| s.to_str()) {
                dialog = dialog.set_file_name(stem);
            }
        }

        if let Some(mut path) = dialog.save_file() {
            if path.extension().is_none() {
                path.set_extension("rsn");
            }
            self.save_file(path);
        }
    }

    /// キーボードショートカットの処理
    fn handle_shortcuts(&mut self, ctx: &Context) {
        let input = ctx.input(|i| i.clone());

        let ctrl_or_cmd = input.modifiers.command || input.modifiers.ctrl;

        // Ctrl + O: Open
        if ctrl_or_cmd && !input.modifiers.shift && input.key_pressed(Key::O) {
            self.open_file_dialog();
        }

        // Ctrl + Shift + S: Save As
        if ctrl_or_cmd && input.modifiers.shift && input.key_pressed(Key::S) {
            self.save_rsn_dialog();
        } else if ctrl_or_cmd && input.key_pressed(Key::S) {
            // Ctrl + S: Quick Save (ダイアログを出さずに直接保存)
            self.quick_save();
        }

        // Ctrl + Z: Undo
        if ctrl_or_cmd && !input.modifiers.shift && input.key_pressed(Key::Z) {
            if self.project.undo() {
                self.status_message = "Undo performed".to_string();
            }
        }

        // Ctrl + Y or Ctrl + Shift + Z: Redo
        if (ctrl_or_cmd && input.key_pressed(Key::Y))
            || (ctrl_or_cmd && input.modifiers.shift && input.key_pressed(Key::Z))
        {
            if self.project.redo() {
                self.status_message = "Redo performed".to_string();
            }
        }

        // Escape: 現在のモードおよびズーム・サブモードを解除
        if input.key_pressed(Key::Escape) {
            self.mode = None;
            self.active_zoom = None;
            self.action_state.clear_submodes();
            self.status_message = "Mode cleared".to_string();
        }

        // Home: Reset Zoom (常に動作)
        if input.key_pressed(Key::Home) {
            self.reset_zoom();
        }

        // Delete / Backspace: 選択されたMultiviewまたはJ-Coupling行の削除
        if input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace) {
            if let Some(mode) = self.mode {
                if mode == AppMode::Multiview {
                    if let Some(ref sel_id) = self.selected_multiview_id {
                        self.project.state.multiviews.retain(|mv| mv.id != *sel_id);
                        self.selected_multiview_id = None;
                        self.project.push_history();
                        self.status_message = "Deleted selected multiview inset".to_string();
                    }
                } else if mode == AppMode::JCoupling {
                    if let Some(idx) = self.selected_j_idx {
                        if idx < self.project.state.j_couplings.len() {
                            self.project.state.j_couplings.remove(idx);
                            self.selected_j_idx = None;
                            self.project.push_history();
                            self.status_message = "Deleted selected J-coupling result".to_string();
                        }
                    }
                }
            }
        }
    }

    /// ファイル D&D の処理
    fn handle_drag_and_drop(&mut self, ctx: &Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            if let Some(path) = file.path {
                self.open_file(path);
                break;
            }
        }
    }
}

impl eframe::App for ResonaApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // ezNMR / 科学NMR標準の洗練されたライトテーマを設定
        let mut visuals = egui::Visuals::light();
        visuals.window_fill = Color32::WHITE;
        visuals.panel_fill = Color32::from_rgb(248, 249, 250); // #f8f9fa
        ctx.set_visuals(visuals);

        self.handle_shortcuts(ctx);
        self.handle_drag_and_drop(ctx);

        // 1. トップメニューバー
        TopBottomPanel::top("top_menu")
            .frame(egui::Frame::none()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Open... (Ctrl+O)").clicked() {
                            self.open_file_dialog();
                            ui.close_menu();
                        }
                        if ui.button("Save (Ctrl+S)").clicked() {
                            self.quick_save();
                            ui.close_menu();
                        }
                        if ui.button("Save As... (Ctrl+Shift+S)").clicked() {
                            self.save_rsn_dialog();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button("Edit", |ui| {
                        if ui.button("Undo (Ctrl+Z)").clicked() {
                            self.project.undo();
                            ui.close_menu();
                        }
                        if ui.button("Redo (Ctrl+Y)").clicked() {
                            self.project.redo();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Fourier Transform Settings...").clicked() {
                            self.ft_dialog_state.settings = self.project.state.ft_settings.clone();
                            self.ft_dialog_state.reset_preview();
                            self.ft_dialog_state.open = true;
                            ui.close_menu();
                        }
                        if ui.button("Display Settings...").clicked() {
                            self.open_display_dialog();
                            ui.close_menu();
                        }
                    });
                });
            });

        // 2. モード切替ツールバー (1行目: ezNMR完全準拠のライトテーマバー)
        TopBottomPanel::top("mode_toolbar")
            .frame(egui::Frame::none()
                .fill(Color32::from_rgb(248, 249, 250))
                .inner_margin(Margin::symmetric(8.0, 5.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                match show_mode_bar(ui, &mut self.mode) {
                    ModeBarEvent::None => {}
                    ModeBarEvent::OpenReFt => {
                        self.ft_dialog_state.settings = self.project.state.ft_settings.clone();
                        self.ft_dialog_state.reset_preview();
                        self.ft_dialog_state.open = true;
                    }
                    ModeBarEvent::OpenDisplay => {
                        self.open_display_dialog();
                    }
                    ModeBarEvent::OpenPrint => {
                        self.print_dialog_state.open();
                    }
                }
            });

        // 3. コンテキスト専用アクションバー (2行目: 左 ZOOM常駐フレーム + 右 コンテキストフレーム)
        let mut p0 = self.project.state.p0;
        let mut p1 = self.project.state.p1;
        let mut int_scale = self.project.state.integration_scale;

        let noise_level = self.project.noise_level();

        if self.mode == Some(AppMode::Peak) && self.action_state.peak_threshold <= 0.0 {
            let initial_thresh = self.project.state.peak_threshold.unwrap_or(noise_level * 10.0);
            self.action_state.peak_threshold = initial_thresh;
            self.project.state.peak_threshold = Some(initial_thresh);
        }

        let action_event = TopBottomPanel::top("action_bar")
            .frame(egui::Frame::none()
                .fill(Color32::from_rgb(248, 249, 250))
                .inner_margin(Margin::symmetric(8.0, 4.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                show_action_bar(
                    ui,
                    self.mode,
                    &mut self.active_zoom,
                    &mut self.action_state,
                    &mut p0,
                    &mut p1,
                    &mut int_scale,
                    &self.project.metadata.nucleus,
                    noise_level,
                )
            }).inner;

        if (p0 - self.project.state.p0).abs() > 1e-4 || (p1 - self.project.state.p1).abs() > 1e-4 {
            self.project.update_phase(p0, p1);
        }
        self.project.state.integration_scale = int_scale;

        // アクションイベントの実行
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
                self.project.push_history();
                self.status_message = format!("ACME Autophase applied (P0={:.2}°, P1={:.2}°)", new_p0, new_p1);
            }
            ActionEvent::ResetPhase => {
                self.project.update_phase(0.0, 0.0);
                self.project.push_history();
            }
            ActionEvent::ApplyBaseline { log_lambda, p } => {
                let lam = 10.0_f64.powf(log_lambda);
                self.project.auto_baseline(lam, p);
                self.project.push_history();
                self.status_message = format!("ALS baseline corrected (λ=1e{:.1}, p={:.4})", log_lambda, p);
            }
            ActionEvent::ClearBaseline => {
                self.project.clear_baseline();
                self.project.push_history();
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
                            self.project.set_shift_reference(peak_ppm, target);
                            self.project.push_history();
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
                self.project.set_shift_reference(peak_ppm, target_ppm);
                self.project.push_history();
                self.status_message = format!("Referenced peak at {:.3} ppm -> {:.3} ppm", peak_ppm, target_ppm);
            }
            ActionEvent::AutoPeak => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    let noise = self.project.noise_level();
                    let thresh = noise * 10.0;
                    self.action_state.peak_threshold = thresh;
                    self.project.state.peak_threshold = Some(thresh);
                    self.project.state.peaks = pick_peaks(spec, ppm, thresh, &self.project.state.peaks);
                    self.project.push_history();
                    self.status_message = format!("Auto detected {} peaks (thresh={:.1}, noise={:.2})", self.project.state.peaks.len(), thresh, noise);
                }
            }
            ActionEvent::PickPeaks { threshold } => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    self.action_state.peak_threshold = threshold;
                    self.project.state.peak_threshold = Some(threshold);
                    self.project.state.peaks = pick_peaks(spec, ppm, threshold, &self.project.state.peaks);
                    self.project.push_history();
                    self.status_message = format!("Picked {} peaks with threshold {:.1}", self.project.state.peaks.len(), threshold);
                }
            }
            ActionEvent::ClearPeaks => {
                self.project.state.peaks.clear();
                self.project.push_history();
                self.status_message = "All peaks cleared".to_string();
            }
            ActionEvent::AutoIntegrate => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    self.project.state.integrations = auto_detect_integrations(spec, ppm);
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
                    self.project.push_history();
                    self.status_message = format!("Auto detected {} integration regions", self.project.state.integrations.len());
                }
            }
            ActionEvent::ClearIntegrations => {
                self.project.state.integrations.clear();
                self.project.push_history();
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
            ActionEvent::ResetMultiview => {
                self.project.state.multiviews.clear();
                self.selected_multiview_id = None;
                self.project.push_history();
                self.status_message = "All multiview insets cleared".to_string();
            }
            ActionEvent::ClearJCoupling => {
                self.project.state.j_couplings.clear();
                self.selected_j_idx = None;
                self.project.push_history();
                self.status_message = "All J-coupling results cleared".to_string();
            }
            ActionEvent::CloseMode => {
                self.mode = None;
                self.action_state.clear_submodes();
                self.status_message = "Mode closed".to_string();
            }
        }

        // 4. 右サイドパネル (ezNMR完全準拠: Metadata, FT Settings, J-Coupling)
        SidePanel::right("side_panel")
            .resizable(true)
            .default_width(250.0)
            .frame(egui::Frame::none()
                .fill(Color32::WHITE)
                .inner_margin(Margin::symmetric(8.0, 8.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                show_side_panel(
                    ui,
                    &self.project.metadata,
                    &self.project.state.ft_settings,
                    &mut self.project.state.j_couplings,
                    &mut self.selected_j_idx,
                );
            });

        // 5. ステータスバー (下部)
        TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none()
                .fill(Color32::from_rgb(248, 249, 250))
                .inner_margin(Margin::symmetric(8.0, 3.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status_message).size(11.0).color(Color32::from_rgb(108, 117, 125)));
                });
            });

        // 6. メインプロット領域 (純白背景、下部80pxピークラベル領域)
        CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::WHITE))
            .show(ctx, |ui| {
                let available_rect = ui.available_rect_before_wrap();
                let plot_rect = Rect::from_min_max(
                    available_rect.min,
                    Pos2::new(available_rect.max.x, available_rect.max.y - 4.0),
                );

                // 初期 transform のセットアップ
                if self.transform.is_none() {
                    self.transform = Some(PlotTransform::new(
                        plot_rect,
                        -0.5,
                        10.5,
                        -100.0,
                        1000.0,
                    ));
                    self.reset_zoom();
                }

                let mut do_reset_zoom = false;
                if let Some(ref mut t) = self.transform {
                    t.screen_rect = plot_rect;

                    // プロット描画
                    if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                        let ref_factor = self.project.state.integration_ref_value
                            / self.project.state.integration_ref_area.max(1e-12);
                        let is_peak_mode = self.mode == Some(AppMode::Peak);
                        let ref_drag_range = if self.mode == Some(AppMode::Reference) && self.action_state.ref_set_active {
                            if let (Some(s), Some(c)) = (self.drag_start, self.drag_current) {
                                let (p1, _) = t.screen_to_data(s);
                                let (p2, _) = t.screen_to_data(c);
                                Some((p1, p2))
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let is_thresh_submode = is_peak_mode && self.action_state.peak_submode == PeakSubMode::Threshold;
                        let is_integrate_edit_mode = self.mode == Some(AppMode::Integrate)
                            && self.action_state.integrate_submode == IntegrateSubMode::Edit;
                        let is_multiview_edit_mode = self.mode == Some(AppMode::Multiview)
                            && self.action_state.multiview_submode == MultiviewSubMode::Edit;

                        // 積分スケールが初期値 (1.0) のままの場合、自動で見やすい高さ (主ピークの約35%) にスケーリング
                        if self.project.state.integration_scale == 1.0 && !self.project.state.integrations.is_empty() {
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
                            }
                        }

                        // Multiview モード時のみ拡大図を表示 (モードを閉じたら非表示、データは保持)
                        let active_multiviews: &[MultiviewItem] = if self.mode == Some(AppMode::Multiview) {
                            &self.project.state.multiviews
                        } else {
                            &[]
                        };

                        // Reference モード時のみ基準ピークマーカーを表示
                        let ref_marker = if self.mode == Some(AppMode::Reference) {
                            self.project.state.reference_point
                        } else {
                            None
                        };

                        paint_spectrum(
                            ui,
                            t,
                            ppm,
                            spec,
                            &self.project.state.peaks,
                            &self.project.state.integrations,
                            self.project.state.integration_scale,
                            self.project.state.integration_offset,
                            ref_factor,
                            active_multiviews,
                            self.action_state.multiview_ratio,
                            self.selected_multiview_id.as_deref(),
                            self.hovered_multiview_id.as_deref(),
                            is_multiview_edit_mode,
                            Some(self.action_state.peak_threshold),
                            is_peak_mode,
                            is_thresh_submode,
                            is_integrate_edit_mode,
                            ref_drag_range,
                            ref_marker,
                            &self.plot_style,
                        );
                    } else {
                        let painter = ui.painter_at(plot_rect);
                        painter.rect_filled(plot_rect, 0.0, Color32::WHITE);
                        painter.text(
                            plot_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "No NMR data loaded.\nDrop a .jdf or .rsn file here, or use File -> Open...",
                            egui::FontId::proportional(15.0),
                            Color32::from_gray(140),
                        );
                    }

                    // マウスインタラクション (クリック & ドラッグ)
                    // ※ ホイールズーム・中クリック・右ドラッグは廃止
                    let response = ui.allocate_rect(plot_rect, egui::Sense::click_and_drag());
                    let pointer_pos = response.hover_pos().or_else(|| ctx.input(|i| i.pointer.latest_pos()));
                    let axis_y = t.axis_y();
                    // ダブルクリックで拡大を1つ前に戻す (Zoom モード時)
                    if response.double_clicked() && self.active_zoom.is_some() {
                        if let Some((p_min, p_max, y_min, y_max)) = self.zoom_history.pop() {
                            t.ppm_min = p_min;
                            t.ppm_max = p_max;
                            t.y_min = y_min;
                            t.y_max = y_max;
                            self.status_message = "Zoom undone (double-click)".to_string();
                        } else {
                            do_reset_zoom = true;
                            self.status_message = "Zoom reset (double-click)".to_string();
                        }
                    }

                    // スレッショルドバーのホバー／ドラッグ判定 (Peak pick モード時)
                    let is_peak_mode = self.mode == Some(AppMode::Peak) && self.active_zoom.is_none();
                    let is_thresh_submode = is_peak_mode && self.action_state.peak_submode == PeakSubMode::Threshold;
                    let is_integrate_edit_mode = self.mode == Some(AppMode::Integrate)
                        && self.action_state.integrate_submode == IntegrateSubMode::Edit
                        && self.active_zoom.is_none();
                    let is_multiview_mode = self.mode == Some(AppMode::Multiview) && self.active_zoom.is_none();
                    let is_multiview_edit_mode = is_multiview_mode && self.action_state.multiview_submode == MultiviewSubMode::Edit;
                    let is_multiview_delete_mode = is_multiview_mode && self.action_state.multiview_submode == MultiviewSubMode::Delete;
                    let thresh_val = self.action_state.peak_threshold;
                    let mut near_threshold = false;

                    // Multiview ホバー判定 (外側8px枠線ゾーンまで検知)
                    self.hovered_multiview_id = None;
                    if is_multiview_mode && (is_multiview_edit_mode || is_multiview_delete_mode) {
                        if let Some(pos) = pointer_pos {
                            for mv in self.project.state.multiviews.iter().rev() {
                                if is_multiview_delete_mode {
                                    let rect = Rect::from_min_size(Pos2::new(mv.geometry.x, mv.geometry.y), egui::vec2(mv.geometry.w, mv.geometry.h));
                                    if rect.expand(6.0).contains(pos) {
                                        self.hovered_multiview_id = Some(mv.id.clone());
                                        ctx.set_cursor_icon(egui::CursorIcon::NoDrop);
                                        break;
                                    }
                                } else if is_multiview_edit_mode {
                                    if let Some(mode) = detect_multiview_drag_mode(pos, &mv.geometry) {
                                        self.hovered_multiview_id = Some(mv.id.clone());
                                        let cursor = match mode {
                                            MultiviewDragMode::TopLeft | MultiviewDragMode::BottomRight => egui::CursorIcon::ResizeNwSe,
                                            MultiviewDragMode::TopRight | MultiviewDragMode::BottomLeft => egui::CursorIcon::ResizeNeSw,
                                            MultiviewDragMode::Left | MultiviewDragMode::Right => egui::CursorIcon::ResizeHorizontal,
                                            MultiviewDragMode::Top | MultiviewDragMode::Bottom => egui::CursorIcon::ResizeVertical,
                                            MultiviewDragMode::Move => egui::CursorIcon::Move,
                                        };
                                        ctx.set_cursor_icon(cursor);
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if is_peak_mode {
                        if let Some(pos) = pointer_pos {
                            let in_plot = plot_rect.contains(pos) && pos.y <= axis_y && response.hovered();
                            if is_thresh_submode {
                                if in_plot {
                                    near_threshold = true;
                                    ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                }
                            } else if thresh_val > 0.0 && in_plot {
                                let sy_pos = t.y_to_screen_y(thresh_val);
                                let sy_neg = t.y_to_screen_y(-thresh_val);
                                if (pos.y - sy_pos).abs() <= 7.0 || (pos.y - sy_neg).abs() <= 7.0 {
                                    near_threshold = true;
                                    ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                }
                            }
                        }
                        if self.is_dragging_threshold {
                            ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        }
                    }

                    if response.clicked_by(egui::PointerButton::Primary) {
                        if let Some(pos) = pointer_pos {
                            if plot_rect.contains(pos) && pos.y <= axis_y {
                                let (click_ppm, _) = t.screen_to_data(pos);
                                if is_thresh_submode {
                                    let new_thresh = t.screen_y_to_y(pos.y).abs();
                                    self.action_state.peak_threshold = new_thresh;
                                    self.project.state.peak_threshold = Some(new_thresh);
                                } else if is_multiview_mode {
                                    if is_multiview_delete_mode {
                                        if let Some(ref hid) = self.hovered_multiview_id.clone() {
                                            self.project.state.multiviews.retain(|m| &m.id != hid);
                                            if self.selected_multiview_id.as_ref() == Some(hid) {
                                                self.selected_multiview_id = None;
                                            }
                                            self.project.push_history();
                                            self.status_message = "Deleted multiview inset".to_string();
                                        }
                                    } else if is_multiview_edit_mode {
                                        if let Some(ref hid) = self.hovered_multiview_id {
                                            self.selected_multiview_id = Some(hid.clone());
                                        } else {
                                            self.selected_multiview_id = None;
                                        }
                                    }
                                } else if self.mode == Some(AppMode::Peak) && self.active_zoom.is_none() {
                                    if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                        let dt = if ppm.len() > 1 { (ppm[0] - ppm[ppm.len() - 1]).abs() / (ppm.len() - 1) as f64 } else { 0.001 };
                                        match self.action_state.peak_submode {
                                            PeakSubMode::Add => {
                                                let p_low = click_ppm - dt * 5.0;
                                                let p_high = click_ppm + dt * 5.0;
                                                let before_count = self.project.state.peaks.len();
                                                self.project.state.peaks = add_peak_in_range(spec, ppm, p_low, p_high, &self.project.state.peaks);
                                                if self.project.state.peaks.len() > before_count {
                                                    self.project.push_history();
                                                    self.status_message = format!("Added peak near {:.3} ppm", click_ppm);
                                                }
                                            }
                                            PeakSubMode::Delete => {
                                                let tol = dt * 5.0;
                                                let before_count = self.project.state.peaks.len();
                                                self.project.state.peaks.retain(|pk| (pk.ppm - click_ppm).abs() > tol);
                                                if self.project.state.peaks.len() < before_count {
                                                    self.project.push_history();
                                                    self.status_message = format!("Deleted peak near {:.3} ppm", click_ppm);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                } else if self.mode == Some(AppMode::Integrate) && self.active_zoom.is_none() {
                                    match self.action_state.integrate_submode {
                                        IntegrateSubMode::Delete => {
                                            let before_count = self.project.state.integrations.len();
                                            self.project.state.integrations.retain(|item| !item.contains_ppm(click_ppm));
                                            if self.project.state.integrations.len() < before_count {
                                                self.project.push_history();
                                                self.status_message = "Deleted integration".to_string();
                                            }
                                        }
                                        IntegrateSubMode::Split => {
                                            let mut split_idx = None;
                                            for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                                if item.contains_ppm(click_ppm) {
                                                    split_idx = Some(idx);
                                                    break;
                                                }
                                            }
                                            if let Some(idx) = split_idx {
                                                let item = self.project.state.integrations.remove(idx);
                                                let (x1, x2) = (item.start_ppm, item.end_ppm);
                                                let (y1, y2) = (item.y_start, item.y_end);
                                                let y_split = item.baseline_y_at(click_ppm);
                                                let d1 = IntegrationItem {
                                                    id: format!("intg-{}", self.project.state.integrations.len() + 1),
                                                    start_ppm: x1,
                                                    end_ppm: click_ppm,
                                                    y_start: y1,
                                                    y_end: y_split,
                                                };
                                                let d2 = IntegrationItem {
                                                    id: format!("intg-{}", self.project.state.integrations.len() + 2),
                                                    start_ppm: click_ppm,
                                                    end_ppm: x2,
                                                    y_start: y_split,
                                                    y_end: y2,
                                                };
                                                self.project.state.integrations.insert(idx, d2);
                                                self.project.state.integrations.insert(idx, d1);
                                                self.project.push_history();
                                                self.status_message = format!("Split integration at {:.3} ppm", click_ppm);
                                            }
                                        }
                                        IntegrateSubMode::Reference => {
                                            let mut target_idx = None;
                                            for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                                if item.contains_ppm(click_ppm) {
                                                    target_idx = Some(idx);
                                                    break;
                                                }
                                            }
                                            if let Some(idx) = target_idx {
                                                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                    let item = &self.project.state.integrations[idx];
                                                    if let Some(res) = compute_integral(spec, ppm, item, 1.0, 1.0, 0.03) {
                                                        if res.total_area > 1e-12 {
                                                            let target_val = self.action_state.integration_ref_val;
                                                            self.project.state.integration_ref_area = res.total_area;
                                                            self.project.state.integration_ref_value = target_val;
                                                            self.project.push_history();
                                                            self.status_message = format!("Set reference integral to {:.2}", target_val);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }

                    // Multiview 選択中の Delete キー削除 (ezNMR lines 401-407準拠)
                    if (ctx.input(|i| i.key_pressed(Key::Delete)) || ctx.input(|i| i.key_pressed(Key::Backspace)))
                        && is_multiview_mode
                        && is_multiview_edit_mode
                    {
                        if let Some(ref sel_id) = self.selected_multiview_id.clone() {
                            self.project.state.multiviews.retain(|m| &m.id != sel_id);
                            self.selected_multiview_id = None;
                            self.project.push_history();
                            self.status_message = "Deleted multiview inset (Delete key)".to_string();
                        }
                    }

                    if let Some(pos) = pointer_pos {
                        let (cur_ppm, cur_y) = t.screen_to_data(pos);
                        self.status_message = format!("PPM {:.3}   Intensity {:.1}", cur_ppm, cur_y);
                    }

                    // 非ドラッグ時: Split サブモードではマウス位置に縦の青色ガイド線を表示
                    if self.mode == Some(AppMode::Integrate) && self.action_state.integrate_submode == IntegrateSubMode::Split && self.drag_start.is_none() {
                        if let Some(pos) = pointer_pos {
                            if plot_rect.contains(pos) && pos.y <= axis_y {
                                let painter = ui.painter_at(plot_rect);
                                painter.line_segment(
                                    [Pos2::new(pos.x, plot_rect.min.y), Pos2::new(pos.x, axis_y)],
                                    Stroke::new(1.5_f32, Color32::from_rgb(13, 110, 253)),
                                );
                            }
                        }
                    }

                    // ドラッグ開始 (押下した瞬間の正確な原点座標を取得して遅れを解消)
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        if near_threshold {
                            self.is_dragging_threshold = true;
                            if let Some(pos) = pointer_pos {
                                let new_thresh = t.screen_y_to_y(pos.y).abs();
                                self.action_state.peak_threshold = new_thresh;
                                self.project.state.peak_threshold = Some(new_thresh);
                            }
                        } else if is_multiview_edit_mode {
                            self.is_dragging_threshold = false;
                            let origin = ctx.input(|i| i.pointer.press_origin()).or(pointer_pos);
                            if let Some(pos) = origin {
                                let mut hit_mv = None;
                                for mv in self.project.state.multiviews.iter().rev() {
                                    if let Some(mode) = detect_multiview_drag_mode(pos, &mv.geometry) {
                                        hit_mv = Some((mv.id.clone(), mode, mv.geometry));
                                        break;
                                    }
                                }
                                if let Some((m_id, mode, geom)) = hit_mv {
                                    self.selected_multiview_id = Some(m_id.clone());
                                    self.multiview_drag = Some(MultiviewDragState {
                                        item_id: m_id,
                                        mode,
                                        start_rect: geom,
                                        start_pointer: pos,
                                    });
                                }
                            }
                        } else if is_integrate_edit_mode {
                            self.is_dragging_threshold = false;
                            if let Some(pos) = pointer_pos {
                                let mut hit_handle = None;
                                for (idx, intg) in self.project.state.integrations.iter().enumerate() {
                                    let s_pos = t.data_to_screen(intg.start_ppm, intg.y_start);
                                    let e_pos = t.data_to_screen(intg.end_ppm, intg.y_end);
                                    if (pos - s_pos).length() < 12.0 {
                                        hit_handle = Some(IntegrateDragTarget::StartHandle(idx));
                                        break;
                                    } else if (pos - e_pos).length() < 12.0 {
                                        hit_handle = Some(IntegrateDragTarget::EndHandle(idx));
                                        break;
                                    }
                                }
                                if let Some(target) = hit_handle {
                                    self.integrate_drag = Some(target);
                                } else {
                                    let (ppm, _) = t.screen_to_data(pos);
                                    let mut in_intg = false;
                                    for intg in &self.project.state.integrations {
                                        if intg.contains_ppm(ppm) {
                                            in_intg = true;
                                            break;
                                        }
                                    }
                                    if in_intg {
                                        let mid_y = (plot_rect.min.y + axis_y) * 0.5;
                                        if pos.y < mid_y {
                                            self.integrate_drag = Some(IntegrateDragTarget::Scale {
                                                start_scale: self.project.state.integration_scale,
                                                start_y: pos.y,
                                            });
                                        } else {
                                            self.integrate_drag = Some(IntegrateDragTarget::Offset {
                                                start_offset: self.project.state.integration_offset,
                                                start_y: pos.y,
                                            });
                                        }
                                    }
                                }
                            }
                        } else {
                            self.is_dragging_threshold = false;
                            let origin = ctx.input(|i| i.pointer.press_origin()).or(pointer_pos);
                            self.drag_start = origin;
                            self.drag_current = pointer_pos;
                        }
                    }

                    // ドラッグ中
                    if response.dragged_by(egui::PointerButton::Primary) {
                        if self.is_dragging_threshold {
                            ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                            if let Some(pos) = pointer_pos {
                                let new_thresh = t.screen_y_to_y(pos.y).abs();
                                self.action_state.peak_threshold = new_thresh;
                                self.project.state.peak_threshold = Some(new_thresh);
                            }
                        } else if let Some(ref drag) = self.multiview_drag {
                            if let Some(pos) = pointer_pos {
                                let delta = pos - drag.start_pointer;
                                if let Some(mv) = self.project.state.multiviews.iter_mut().find(|m| m.id == drag.item_id) {
                                    let mut r = drag.start_rect;
                                    match drag.mode {
                                        MultiviewDragMode::Move => {
                                            r.x += delta.x;
                                            r.y += delta.y;
                                        }
                                        MultiviewDragMode::Left => {
                                            let new_w = (r.w - delta.x).max(60.0);
                                            r.x += r.w - new_w;
                                            r.w = new_w;
                                        }
                                        MultiviewDragMode::Right => {
                                            r.w = (r.w + delta.x).max(60.0);
                                        }
                                        MultiviewDragMode::Top => {
                                            let new_h = (r.h - delta.y).max(60.0);
                                            r.y += r.h - new_h;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::Bottom => {
                                            r.h = (r.h + delta.y).max(60.0);
                                        }
                                        MultiviewDragMode::TopLeft => {
                                            let new_w = (r.w - delta.x).max(60.0);
                                            let new_h = (r.h - delta.y).max(60.0);
                                            r.x += r.w - new_w;
                                            r.y += r.h - new_h;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::TopRight => {
                                            let new_w = (r.w + delta.x).max(60.0);
                                            let new_h = (r.h - delta.y).max(60.0);
                                            r.y += r.h - new_h;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::BottomLeft => {
                                            let new_w = (r.w - delta.x).max(60.0);
                                            let new_h = (r.h + delta.y).max(60.0);
                                            r.x += r.w - new_w;
                                            r.w = new_w;
                                            r.h = new_h;
                                        }
                                        MultiviewDragMode::BottomRight => {
                                            r.w = (r.w + delta.x).max(60.0);
                                            r.h = (r.h + delta.y).max(60.0);
                                        }
                                    }
                                    mv.geometry = r;
                                }
                            }
                        } else if let Some(target) = self.integrate_drag {
                            if let Some(pos) = pointer_pos {
                                match target {
                                    IntegrateDragTarget::StartHandle(idx) => {
                                        if idx < self.project.state.integrations.len() {
                                            let (p, y) = t.screen_to_data(pos);
                                            self.project.state.integrations[idx].start_ppm = p;
                                            self.project.state.integrations[idx].y_start = y;
                                        }
                                    }
                                    IntegrateDragTarget::EndHandle(idx) => {
                                        if idx < self.project.state.integrations.len() {
                                            let (p, y) = t.screen_to_data(pos);
                                            self.project.state.integrations[idx].end_ppm = p;
                                            self.project.state.integrations[idx].y_end = y;
                                        }
                                    }
                                    IntegrateDragTarget::Scale { start_scale, start_y } => {
                                        ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                        let dy = start_y - pos.y;
                                        let factor = ((dy / (plot_rect.height() * 0.25)) as f64).exp();
                                        self.project.state.integration_scale = (start_scale * factor).max(1e-12);
                                        self.status_message = format!("Scale {:.2e}", self.project.state.integration_scale);
                                    }
                                    IntegrateDragTarget::Offset { start_offset, start_y } => {
                                        ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                        let dy = start_y - pos.y;
                                        let d_offset = (dy / plot_rect.height()) as f64 * 0.5;
                                        self.project.state.integration_offset = start_offset + d_offset;
                                        self.status_message = format!("Offset {:.3}", self.project.state.integration_offset);
                                    }
                                }
                            }
                        } else {
                            self.drag_current = pointer_pos;
                        }
                    }

                    // ラバーバンド描画 (ドラッグ中、スレッショルドドラッグでない場合)
                    if !self.is_dragging_threshold && self.integrate_drag.is_none() && self.multiview_drag.is_none() {
                        if let (Some(start), Some(curr)) = (self.drag_start, self.drag_current) {
                            let painter = ui.painter_at(plot_rect);
                        let axis_y = t.axis_y();

                        // Zoom ツールがアクティブな場合は最優先で Zoom ラバーバンドを表示
                        if let Some(tool) = self.active_zoom {
                            let band_rect = match tool {
                                ZoomTool::Rect => Rect::from_two_pos(start, curr),
                                ZoomTool::X => Rect::from_min_max(
                                    Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                    Pos2::new(start.x.max(curr.x), axis_y),
                                ),
                                ZoomTool::Y => Rect::from_min_max(
                                    Pos2::new(plot_rect.min.x, start.y.min(curr.y)),
                                    Pos2::new(plot_rect.max.x, start.y.max(curr.y)),
                                ),
                            };
                            painter.rect_filled(band_rect, 0.0, self.plot_style.rubberband_color);
                            painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(13, 110, 253)));
                        } else if let Some(mode) = self.mode {
                            match mode {
                                AppMode::Reference => {
                                    // paint_spectrum の ref_drag_range で既に半透明矩形を描画
                                }
                                AppMode::Peak => {
                                    let band_rect = Rect::from_min_max(
                                        Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                        Pos2::new(start.x.max(curr.x), axis_y),
                                    );
                                    painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(0, 180, 80, 25));
                                }
                                AppMode::Integrate => {
                                    let min_x = start.x.min(curr.x);
                                    let max_x = start.x.max(curr.x);
                                    let band_rect = Rect::from_min_max(
                                        Pos2::new(min_x, plot_rect.min.y),
                                        Pos2::new(max_x, axis_y),
                                    );
                                    match self.action_state.integrate_submode {
                                        IntegrateSubMode::Add => {
                                            // 1. 半透明の濃いめハイライト (赤/ピンク)
                                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(225, 29, 72, 45));
                                            painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(225, 29, 72)));
                                            // 2. 開始位置と現在位置の両端に明瞭な縦線 (上からX軸まで届く赤線)
                                            let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(225, 29, 72));
                                            painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);

                                            // 3. 上端に選択範囲の PPM 情報ガイド
                                            let (p1, _) = t.screen_to_data(start);
                                            let (p2, _) = t.screen_to_data(curr);
                                            let p_s = p1.max(p2);
                                            let p_e = p1.min(p2);
                                            let label_txt = format!("{:.3} ~ {:.3} ppm (Δ={:.3})", p_s, p_e, p_s - p_e);
                                            painter.text(
                                                Pos2::new((min_x + max_x) * 0.5, plot_rect.min.y + 12.0),
                                                egui::Align2::CENTER_CENTER,
                                                label_txt,
                                                egui::FontId::proportional(11.5),
                                                Color32::from_rgb(225, 29, 72),
                                            );
                                        }
                                        IntegrateSubMode::Delete => {
                                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(220, 38, 38, 50));
                                            let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(220, 38, 38));
                                            painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);
                                        }
                                        IntegrateSubMode::Split => {
                                            let stroke_v = Stroke::new(2.0_f32, Color32::from_rgb(13, 110, 253));
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);
                                        }
                                        IntegrateSubMode::Reference => {
                                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(13, 110, 253, 45));
                                            let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(13, 110, 253));
                                            painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);
                                        }
                                        _ => {}
                                    }
                                }
                                AppMode::Multiview => {
                                    let p1 = t.screen_to_data(start).0;
                                    let p2 = t.screen_to_data(curr).0;
                                    let p_high = p1.max(p2);
                                    let p_low = p1.min(p2);
                                    let delta_p = p_high - p_low;

                                    match self.action_state.multiview_submode {
                                        MultiviewSubMode::Add => {
                                            let min_x = start.x.min(curr.x);
                                            let max_x = start.x.max(curr.x);
                                            let band_rect = Rect::from_min_max(
                                                Pos2::new(min_x, plot_rect.min.y),
                                                Pos2::new(max_x, axis_y),
                                            );
                                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(147, 51, 234, 45));
                                            painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(147, 51, 234)));
                                            let stroke_v = Stroke::new(1.5_f32, Color32::from_rgb(147, 51, 234));
                                            painter.line_segment([Pos2::new(start.x, plot_rect.min.y), Pos2::new(start.x, axis_y)], stroke_v);
                                            painter.line_segment([Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, axis_y)], stroke_v);

                                            let label_txt = format!("{:.3} ~ {:.3} ppm (Δ={:.3})", p_high, p_low, delta_p);
                                            painter.text(
                                                Pos2::new((min_x + max_x) * 0.5, plot_rect.min.y + 12.0),
                                                egui::Align2::CENTER_CENTER,
                                                label_txt,
                                                egui::FontId::proportional(11.5),
                                                Color32::from_rgb(147, 51, 234),
                                            );
                                        }
                                        _ => {}
                                    }
                                }
                                AppMode::JCoupling => {
                                    if self.action_state.jcoupling_add_active {
                                        let band_rect = Rect::from_min_max(
                                            Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                            Pos2::new(start.x.max(curr.x), axis_y),
                                        );
                                        painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(13, 110, 253, 30));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                    // ドラッグ終了 (解放) 時の処理
                    if response.drag_stopped_by(egui::PointerButton::Primary) {
                        if self.multiview_drag.is_some() {
                            self.multiview_drag = None;
                            self.project.push_history();
                        } else if self.integrate_drag.is_some() {
                            self.integrate_drag = None;
                            self.project.push_history();
                        } else if self.is_dragging_threshold {
                            self.is_dragging_threshold = false;
                            self.status_message = format!("Threshold set to {:.2}", self.action_state.peak_threshold);
                        } else if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
                            let (p_start, y_start) = t.screen_to_data(start);
                            let (p_end, y_end) = t.screen_to_data(end);

                            if let Some(tool) = self.active_zoom {
                                // ズーム前の範囲を履歴に保存
                                self.zoom_history.push((t.ppm_min, t.ppm_max, t.y_min, t.y_max));

                                let dx = (start.x - end.x).abs();
                                let dy = (start.y - end.y).abs();
                                if dx > 5.0 || dy > 5.0 {
                                    match tool {
                                        ZoomTool::Rect => {
                                            t.ppm_min = p_start.min(p_end);
                                            t.ppm_max = p_start.max(p_end);
                                            t.y_min = y_start.min(y_end);
                                            t.y_max = y_start.max(y_end);
                                        }
                                        ZoomTool::X => {
                                            t.ppm_min = p_start.min(p_end);
                                            t.ppm_max = p_start.max(p_end);
                                        }
                                        ZoomTool::Y => {
                                            t.y_min = y_start.min(y_end);
                                            t.y_max = y_start.max(y_end);
                                        }
                                    }
                                }
                            } else if let Some(mode) = self.mode {
                                match mode {
                                    AppMode::Reference => {
                                        if self.action_state.ref_set_active && (start.x - end.x).abs() > 3.0 {
                                            if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                                let p_low = p_start.min(p_end);
                                                let p_high = p_start.max(p_end);
                                                let mut best_p = None;
                                                let mut max_abs = -1.0_f64;
                                                for i in 0..ppm.len().min(spec.len()) {
                                                    let p = ppm[i];
                                                    if p >= p_low && p <= p_high {
                                                        let abs_val = spec[i].abs();
                                                        if abs_val > max_abs {
                                                            max_abs = abs_val;
                                                            best_p = Some(p);
                                                        }
                                                    }
                                                }
                                                if let Some(peak_ppm) = best_p {
                                                    let target = self.action_state.ref_target_ppm;
                                                    self.project.set_shift_reference(peak_ppm, target);
                                                    self.project.push_history();
                                                    self.status_message = format!("Referenced peak at {:.3} ppm -> {:.3} ppm", peak_ppm, target);
                                                }
                                            }
                                        }
                                    }
                                    AppMode::Peak => {
                                        if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                            let p_low = p_start.min(p_end);
                                            let p_high = p_start.max(p_end);
                                            let dx = (start.x - end.x).abs();
                                            if dx > 3.0 {
                                                if self.action_state.peak_submode == PeakSubMode::Add {
                                                    let before_count = self.project.state.peaks.len();
                                                    self.project.state.peaks = add_peak_in_range(spec, ppm, p_low, p_high, &self.project.state.peaks);
                                                    if self.project.state.peaks.len() > before_count {
                                                        self.project.push_history();
                                                        self.status_message = format!("Added peak in range [{:.3}, {:.3}] ppm", p_low, p_high);
                                                    } else {
                                                        self.status_message = "Peak already exists in selected range".to_string();
                                                    }
                                                } else if self.action_state.peak_submode == PeakSubMode::Delete {
                                                    let before_count = self.project.state.peaks.len();
                                                    self.project.state.peaks.retain(|pk| pk.ppm < p_low || pk.ppm > p_high);
                                                    if self.project.state.peaks.len() < before_count {
                                                        self.project.push_history();
                                                        self.status_message = "Deleted peaks in selected range".to_string();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    AppMode::Integrate => {
                                        let p_low = p_start.min(p_end);
                                        let p_high = p_start.max(p_end);
                                        let dx = (start.x - end.x).abs();
                                        match self.action_state.integrate_submode {
                                            IntegrateSubMode::Add => {
                                                if dx > 5.0 {
                                                    let s_ppm = p_start.max(p_end);
                                                    let e_ppm = p_start.min(p_end);
                                                    let med = self.project.spectrum_real.as_ref().map(|s| {
                                                        let mut sorted = s.to_vec();
                                                        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                                                        sorted[sorted.len() / 2]
                                                    }).unwrap_or(0.0);

                                                    let new_item = IntegrationItem {
                                                        id: format!("intg-{}", self.project.state.integrations.len() + 1),
                                                        start_ppm: s_ppm,
                                                        end_ppm: e_ppm,
                                                        y_start: med,
                                                        y_end: med,
                                                    };

                                                    if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                        if self.project.state.integrations.is_empty() || self.project.state.integration_scale == 1.0 {
                                                            if let Some(res) = compute_integral(spec, ppm, &new_item, 1.0, 1.0, 0.03) {
                                                                if res.total_area > 1e-12 {
                                                                    self.project.state.integration_ref_area = res.total_area;
                                                                    self.project.state.integration_ref_value = 1.0;
                                                                    let max_spec = self.project.max_intensity();
                                                                    if max_spec > 0.0 {
                                                                        self.project.state.integration_scale = (max_spec * 0.35) / res.total_area;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    self.project.state.integrations.push(new_item);
                                                    self.project.push_history();
                                                    self.status_message = format!("Added integration {:.3} ~ {:.3} ppm", s_ppm, e_ppm);
                                                }
                                            }
                                            IntegrateSubMode::Delete => {
                                                if dx > 3.0 {
                                                    let before_len = self.project.state.integrations.len();
                                                    self.project.state.integrations.retain(|item| {
                                                        let item_low = item.min_ppm();
                                                        let item_high = item.max_ppm();
                                                        !(p_low.max(item_low) <= p_high.min(item_high))
                                                    });
                                                    if self.project.state.integrations.len() != before_len {
                                                        self.project.push_history();
                                                        self.status_message = "Deleted integrations in range".to_string();
                                                    }
                                                }
                                            }
                                            IntegrateSubMode::Split => {
                                                let split_ppm = p_end;
                                                let mut split_idx = None;
                                                for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                                    if item.contains_ppm(split_ppm) {
                                                        split_idx = Some(idx);
                                                        break;
                                                    }
                                                }
                                                if let Some(idx) = split_idx {
                                                    let item = self.project.state.integrations.remove(idx);
                                                    let (x1, x2) = (item.start_ppm, item.end_ppm);
                                                    let (y1, y2) = (item.y_start, item.y_end);
                                                    let y_split = item.baseline_y_at(split_ppm);
                                                    let d1 = IntegrationItem {
                                                        id: format!("intg-{}", self.project.state.integrations.len() + 1),
                                                        start_ppm: x1,
                                                        end_ppm: split_ppm,
                                                        y_start: y1,
                                                        y_end: y_split,
                                                    };
                                                    let d2 = IntegrationItem {
                                                        id: format!("intg-{}", self.project.state.integrations.len() + 2),
                                                        start_ppm: split_ppm,
                                                        end_ppm: x2,
                                                        y_start: y_split,
                                                        y_end: y2,
                                                    };
                                                    self.project.state.integrations.insert(idx, d2);
                                                    self.project.state.integrations.insert(idx, d1);
                                                    self.project.push_history();
                                                    self.status_message = format!("Split integration at {:.3} ppm", split_ppm);
                                                }
                                            }
                                            IntegrateSubMode::Reference => {
                                                let ref_ppm = p_end;
                                                let mut target_idx = None;
                                                for (idx, item) in self.project.state.integrations.iter().enumerate() {
                                                    if item.contains_ppm(ref_ppm) || (p_low.max(item.min_ppm()) <= p_high.min(item.max_ppm())) {
                                                        target_idx = Some(idx);
                                                        break;
                                                    }
                                                }
                                                if let Some(idx) = target_idx {
                                                    if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                        let item = &self.project.state.integrations[idx];
                                                        if let Some(res) = compute_integral(spec, ppm, item, 1.0, 1.0, 0.03) {
                                                            if res.total_area > 1e-12 {
                                                                let target_val = self.action_state.integration_ref_val;
                                                                self.project.state.integration_ref_area = res.total_area;
                                                                self.project.state.integration_ref_value = target_val;
                                                                self.project.push_history();
                                                                self.status_message = format!("Set reference integral to {:.2}", target_val);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    AppMode::Multiview => {
                                        let p_low = p_start.min(p_end);
                                        let p_high = p_start.max(p_end);
                                        let dx = (start.x - end.x).abs();
                                        let ratio = self.action_state.multiview_ratio;

                                        match self.action_state.multiview_submode {
                                            MultiviewSubMode::Add => {
                                                if dx > 8.0 {
                                                    let view_ppm_span = (t.ppm_max - t.ppm_min).abs().max(1e-6);
                                                    let px_w = plot_rect.width();
                                                    let w_main = ((p_high - p_low) / view_ppm_span) * (px_w as f64);
                                                    let w = (240.0 + (w_main * ratio) as f32 * 0.5).clamp(240.0, 600.0);
                                                    let h = (w * 0.70).clamp(160.0, 420.0);
                                                    // 既存の拡大図に被らないように左上から順に空き位置を探索
                                                    let pos = find_non_overlapping_multiview_pos(&self.project.state.multiviews, plot_rect, w, h);
                                                    let geom = RectF {
                                                        x: pos.x,
                                                        y: pos.y,
                                                        w,
                                                        h,
                                                    };
                                                    let mv_id = format!("mv-{}", self.project.state.multiviews.len() + 1);
                                                    self.project.state.multiviews.push(MultiviewItem {
                                                        id: mv_id.clone(),
                                                        src_x_min: p_low,
                                                        src_x_max: p_high,
                                                        src_y_min: None,
                                                        src_y_max: None,
                                                        ratio,
                                                        geometry: geom,
                                                    });
                                                    self.selected_multiview_id = Some(mv_id);
                                                    self.project.push_history();
                                                    self.status_message = format!("Added multiview inset {:.3} ~ {:.3} ppm", p_high, p_low);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    AppMode::JCoupling => {
                                        if self.action_state.jcoupling_add_active && (start.x - end.x).abs() > 5.0 {
                                            let p_low = p_start.min(p_end);
                                            let p_high = p_start.max(p_end);
                                            let freq_mhz = self.project.metadata.obs_freq_mhz;

                                            let mut peaks_in_range = Vec::new();
                                            let mut peaks_hz = Vec::new();
                                            let mut intensities = Vec::new();
                                            for pk in &self.project.state.peaks {
                                                if pk.ppm >= p_low && pk.ppm <= p_high {
                                                    peaks_in_range.push(pk);
                                                    peaks_hz.push(pk.ppm * freq_mhz);
                                                    intensities.push(pk.intensity);
                                                }
                                            }

                                            if !peaks_hz.is_empty() {
                                                // 1. ピーク強度加重平均による化学シフト重心の計算
                                                let total_int: f64 = intensities.iter().sum();
                                                let center_ppm = if total_int > 0.0 {
                                                    peaks_in_range.iter().map(|p| p.ppm * p.intensity).sum::<f64>() / total_int
                                                } else {
                                                    (p_low + p_high) * 0.5
                                                };

                                                let shift_str = format!("{:.2}", center_ppm);
                                                let shift_str_m = format!("{:.2}-{:.2}", p_high, p_low);

                                                // 2. プロトン数 (積分値) の算出
                                                let mut raw_protons = 1.0_f64;
                                                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                    let ref_factor = self.project.state.integration_ref_value
                                                        / self.project.state.integration_ref_area.max(1e-12);

                                                    // 重なる既存の積分区間を探索
                                                    let mut matched_intg = None;
                                                    for intg in &self.project.state.integrations {
                                                        let i_min = intg.min_ppm();
                                                        let i_max = intg.max_ppm();
                                                        let overlap_min = p_low.max(i_min);
                                                        let overlap_max = p_high.min(i_max);
                                                        if overlap_max > overlap_min {
                                                            let overlap_len = overlap_max - overlap_min;
                                                            if overlap_len > 0.4 * (i_max - i_min) || overlap_len > 0.4 * (p_high - p_low) {
                                                                if let Some(res) = compute_integral(spec, ppm, intg, 1.0, ref_factor, 0.0) {
                                                                    matched_intg = Some(res.normalized_value);
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }

                                                    if let Some(val) = matched_intg {
                                                        raw_protons = val;
                                                    } else if !self.project.state.integrations.is_empty() || self.project.state.integration_ref_area != 1.0 {
                                                        // 選択範囲の直接台形積分
                                                        let n_pts = ppm.len().min(spec.len());
                                                        let mut in_range_pts: Vec<(f64, f64)> = Vec::new();
                                                        for i in 0..n_pts {
                                                            let p = ppm[i];
                                                            if p >= p_low && p <= p_high {
                                                                in_range_pts.push((p, spec[i]));
                                                            }
                                                        }
                                                        if in_range_pts.len() >= 2 {
                                                            in_range_pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                                                            let mut area = 0.0_f64;
                                                            for i in 0..in_range_pts.len() - 1 {
                                                                let dx = (in_range_pts[i + 1].0 - in_range_pts[i].0).abs();
                                                                let avg_y = (in_range_pts[i].1 + in_range_pts[i + 1].1) * 0.5;
                                                                area += avg_y * dx;
                                                            }
                                                            raw_protons = (area.abs() * ref_factor).max(0.01);
                                                        }
                                                    }
                                                }

                                                // 3. Allow Non-Integer の適用
                                                let proton_str = if self.action_state.non_integer_protons {
                                                    format!("{:.2}H", raw_protons)
                                                } else {
                                                    format!("{}H", raw_protons.round().max(1.0) as i64)
                                                };

                                                let candidates = analyze_multiplet(
                                                    peaks_hz,
                                                    intensities,
                                                    &shift_str,
                                                    &shift_str_m,
                                                    &proton_str,
                                                    1.0,
                                                );

                                                if !candidates.is_empty() {
                                                    self.jcoupling_dialog_state.candidates = candidates;
                                                    self.jcoupling_dialog_state.selected_idx = 0;
                                                    self.jcoupling_dialog_state.edited_text = self.jcoupling_dialog_state.candidates[0].text.clone();
                                                    self.jcoupling_dialog_state.center_ppm = center_ppm;
                                                    self.jcoupling_dialog_state.open = true;
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        self.drag_start = None;
                        self.drag_current = None;
                        self.is_dragging_threshold = false;
                    }
                }

                if do_reset_zoom {
                    self.reset_zoom();
                }
            });

        // 7. ダイアログの表示と処理
        if let Some(ft_settings) = show_ft_dialog(
            ctx,
            &mut self.ft_dialog_state,
            self.project.fid_raw.as_ref(),
            Some(&self.project.metadata),
            self.project.metadata.digital_filter_delay,
            self.project.state.p0,
            self.project.state.p1,
        ) {
            if let Some(ref fid_raw) = self.project.fid_raw {
                let raw_fid = crate::core::RawFid {
                    data: fid_raw.clone(),
                    metadata: self.project.metadata.clone(),
                    group_delay: self.project.metadata.digital_filter_delay,
                };
                match crate::core::process_raw_fid(&raw_fid, &ft_settings, self.project.state.p0, self.project.state.p1) {
                    Ok(processed) => {
                        self.project.ppm = Some(processed.ppm);
                        self.project.spectrum_real = Some(processed.spectrum_real);
                        self.project.complex_spectrum_unphased = Some(processed.complex_spectrum_unphased);
                        self.project.invalidate_cache();
                        self.project.state.ft_settings = ft_settings;
                        if self.project.state.ft_settings.auto_phase {
                            self.project.auto_phase();
                        }
                        self.project.push_history();
                        self.reset_zoom();
                        self.status_message = "Fourier Transform applied with updated settings".to_string();
                    }
                    Err(e) => {
                        self.status_message = format!("FT error {}", e);
                    }
                }
            }
        }

        if let Some(res) = show_display_dialog(ctx, &mut self.display_dialog_state) {
            if let Some(ref mut t) = self.transform {
                t.ppm_min = res.ppm_min;
                t.ppm_max = res.ppm_max;
                t.y_min = res.y_min;
                t.y_max = res.y_max;
            }
            self.plot_style.ppm_decimals = res.ppm_decimals;
            self.plot_style.integral_decimals = res.integral_decimals;
            self.plot_style.auto_ticks = res.auto_ticks;
            self.plot_style.tick_major = res.tick_major;
            self.plot_style.tick_minor = res.tick_minor;
        }

        if let Some((text, center_ppm)) = show_jcoupling_dialog(ctx, &mut self.jcoupling_dialog_state) {
            self.project.state.j_couplings.push(JCouplingResultItem {
                text,
                ppm: center_ppm,
            });
            self.project.push_history();
            self.status_message = "Added J-coupling multiplet to results".to_string();
        }

        let ref_factor = if self.project.state.integration_ref_area > 0.0 {
            self.project.state.integration_ref_value / self.project.state.integration_ref_area
        } else {
            1.0
        };

        // 印刷ダイアログへ現在のスタイル設定（桁数・目盛り設定）を同期
        self.print_dialog_state.settings.ppm_decimals = self.plot_style.ppm_decimals;
        self.print_dialog_state.settings.integral_decimals = self.plot_style.integral_decimals;
        self.print_dialog_state.settings.auto_ticks = self.plot_style.auto_ticks;
        self.print_dialog_state.settings.tick_major = self.plot_style.tick_major;
        self.print_dialog_state.settings.tick_minor = self.plot_style.tick_minor;

        show_print_dialog(
            ctx,
            &mut self.print_dialog_state,
            self.transform.as_ref(),
            self.project.ppm.as_ref(),
            self.project.spectrum_real.as_ref(),
            &self.project.state.peaks,
            &self.project.state.integrations,
            self.project.state.integration_scale,
            self.project.state.integration_offset,
            ref_factor,
            &self.project.state.multiviews,
            &self.project.metadata,
            &self.project.state.ft_settings,
            &self.project.state.j_couplings,
            self.current_file_path.as_deref(),
        );
    }
}
