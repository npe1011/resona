use std::collections::HashSet;
use std::path::{Path, PathBuf};
use egui::{CentralPanel, Color32, Context, Key, Margin, Pos2, Rect, RichText, Stroke, TopBottomPanel, SidePanel};
use crate::core::{
    add_peak_in_range, analyze_multiplet, auto_detect_integrations, compute_integral,
    parse_jcoupling_sort_ppm, pick_peaks, IntegrationItem, JCouplingResultItem, MultiviewItem,
    Project, RectF,
};

use crate::gui::dialogs::{
    show_display_dialog, show_ft_dialog, show_full_auto_dialog, show_jcoupling_dialog,
    show_multiview_yscale_dialog, show_print_dialog, DisplayDialogState, FtDialogState,
    FullAutoBaselineChoice, FullAutoDialogState, JCouplingDialogState,
    MultiviewYScaleDialogState, PrintDialogState,
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

/// Integrate Edit モードでの曲線の操作種別
#[derive(Debug, Clone, Copy, PartialEq)]
enum IntgEditTarget {
    Scale,
    Offset,
}

/// 点 p から線分 ab への画面上での最短距離
fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq <= 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).length()
}

/// 積分曲線のホバー／ドラッグ対象判定
/// - 積分曲線の上端付近: Scale (拡大率変更)
/// - 積分曲線の線上付近: Offset (平行移動)
/// - 線分への最短距離ベースで判定し、急峻な立ち上がり部や端点付近でも余裕を持って掴めるようにする
fn detect_integrate_edit_target(
    pos: Pos2,
    ppm: f64,
    intg: &IntegrationItem,
    t: &PlotTransform,
    spec: &ndarray::Array1<f64>,
    ppm_arr: &ndarray::Array1<f64>,
    scale: f64,
    offset: f64,
    ref_factor: f64,
) -> Option<IntgEditTarget> {
    // 画面 X 座標での範囲判定 (左右に 16px の余裕マージンを持たせる)
    let s_x = t.ppm_to_screen_x(intg.start_ppm);
    let e_x = t.ppm_to_screen_x(intg.end_ppm);
    let min_x = s_x.min(e_x) - 16.0;
    let max_x = s_x.max(e_x) + 16.0;
    if pos.x < min_x || pos.x > max_x {
        return None;
    }

    let res = compute_integral(spec, ppm_arr, intg, scale, ref_factor, offset)?;
    if res.ppm.len() < 2 || res.curve_y.len() < 2 {
        return None;
    }

    // 曲線の画面座標点列
    let curve_pts: Vec<Pos2> = res.ppm
        .iter()
        .zip(res.curve_y.iter())
        .map(|(&p, &y)| t.data_to_screen(p, y))
        .collect();

    // 曲線全体への最短距離 & 頂点画面位置
    let mut min_dist = f32::INFINITY;
    let mut min_curve_screen_y = f32::INFINITY;
    let mut top_pos = curve_pts[0];

    for i in 0..curve_pts.len() {
        let pt = curve_pts[i];
        if pt.y < min_curve_screen_y {
            min_curve_screen_y = pt.y;
            top_pos = pt;
        }
        if i + 1 < curve_pts.len() {
            let d = dist_to_segment(pos, curve_pts[i], curve_pts[i + 1]);
            if d < min_dist {
                min_dist = d;
            }
        }
    }

    let bl_screen_y = t.data_to_screen(ppm, intg.baseline_y_at(ppm)).y;

    // 1. ベースラインより大幅に下（18px以上下）は判定外（誤操作防止）
    if pos.y > bl_screen_y + 18.0 {
        return None;
    }

    // 2. 曲線より遥か上空（28px以上上）も判定外
    if pos.y < min_curve_screen_y - 28.0 {
        return None;
    }

    // 3. 掴む許容距離 (22px: 従来の14pxから大幅に拡大し掴みやすくする)
    let hit_tolerance = 22.0_f32;
    if min_dist > hit_tolerance && (pos - top_pos).length() > hit_tolerance {
        return None;
    }

    // 4. 積分曲線の上端判定 (頂点付近、または上部近傍) -> Scale
    if (pos - top_pos).length() <= hit_tolerance + 4.0
        || (pos.y <= min_curve_screen_y + 16.0 && min_dist <= hit_tolerance)
    {
        Some(IntgEditTarget::Scale)
    } else {
        // 5. それ以外の積分曲線付近 -> Offset
        Some(IntgEditTarget::Offset)
    }
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
pub struct MultiviewDragItemState {
    pub id: String,
    pub start_rect: RectF,
}

#[derive(Debug, Clone)]
pub struct MultiviewDragState {
    pub item_id: String,
    pub mode: MultiviewDragMode,
    pub start_rect: RectF,
    pub start_pointer: Pos2,
    pub items: Vec<MultiviewDragItemState>,
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
    pub full_auto_dialog_state: FullAutoDialogState,
    pub display_dialog_state: DisplayDialogState,
    pub jcoupling_dialog_state: JCouplingDialogState,
    pub multiview_yscale_dialog_state: MultiviewYScaleDialogState,
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
    pub selected_multiview_ids: HashSet<String>,
    pub hovered_multiview_id: Option<String>,
    pub selected_j_idx: Option<usize>,

    pub last_multiview_ratio: f64,
    pub status_message: String,

    pub y_max_scale: f64,
    pub y_min_scale: f64,
    last_transform_y: Option<(f64, f64)>,
    pub arrow_key_hold_time: f64,
    pub temp_zoom_saved: Option<Option<ZoomTool>>,
}

impl Default for ResonaApp {
    fn default() -> Self {
        let settings = crate::gui::config::AppSettings::load();

        let mut display_dialog_state = DisplayDialogState::default();
        display_dialog_state.ppm_decimals = settings.ppm_decimals;
        display_dialog_state.integral_decimals = settings.integral_decimals;

        let mut plot_style = PlotStyle::default();
        plot_style.ppm_decimals = settings.ppm_decimals;
        plot_style.integral_decimals = settings.integral_decimals;

        let mut action_state = ActionBarState::default();
        action_state.multiview_ratio = settings.multiview_ratio;
        action_state.multiview_auto_align = settings.multiview_auto_align;

        let mut print_dialog_state = PrintDialogState::default();
        print_dialog_state.settings = settings.print_settings;

        let mut full_auto_dialog_state = FullAutoDialogState::default();
        full_auto_dialog_state.baseline_choice = settings.full_auto_baseline_choice;
        full_auto_dialog_state.airpls_log_lambda = settings.full_auto_airpls_lambda;
        full_auto_dialog_state.poly_order = settings.full_auto_poly_order;
        full_auto_dialog_state.enable_integration = settings.full_auto_integration;

        Self {
            project: Project::new(),
            current_file_path: None,
            current_directory: settings.current_directory,
            mode: None,
            active_zoom: None,
            action_state,
            ft_dialog_state: FtDialogState::default(),
            full_auto_dialog_state,
            display_dialog_state,
            jcoupling_dialog_state: JCouplingDialogState::default(),
            multiview_yscale_dialog_state: MultiviewYScaleDialogState::default(),
            print_dialog_state,
            plot_style,
            transform: None,
            zoom_history: Vec::new(),
            drag_start: None,
            drag_current: None,
            is_dragging_threshold: false,
            integrate_drag: None,
            multiview_drag: None,
            selected_multiview_ids: HashSet::new(),
            hovered_multiview_id: None,
            selected_j_idx: None,
            last_multiview_ratio: settings.multiview_ratio,
            status_message: "Ready. Drag & drop .jdf or .rsn file here.".to_string(),
            y_max_scale: 80.0,
            y_min_scale: 10.0,
            last_transform_y: None,
            arrow_key_hold_time: 0.0,
            temp_zoom_saved: None,
        }
    }
}

impl ResonaApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

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

    /// いずれかのモーダルダイアログが開いているか判定
    pub fn has_open_dialog(&self) -> bool {
        self.ft_dialog_state.open
            || self.full_auto_dialog_state.open
            || self.display_dialog_state.open
            || self.jcoupling_dialog_state.open
            || self.multiview_yscale_dialog_state.open
            || self.print_dialog_state.is_open
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
    pub fn save_file<P: AsRef<Path>>(&mut self, path: P) {
        let p = path.as_ref();
        let abs_path = if p.is_relative() {
            std::env::current_dir().unwrap_or_default().join(p)
        } else {
            p.to_path_buf()
        };
        match self.project.save_rsn(&abs_path) {
            Ok(_) => {
                // 一回でも保存した状態は rsn ファイルのフルパスに切り替える
                self.current_file_path = Some(abs_path.clone());
                if let Some(parent) = abs_path.parent() {
                    self.current_directory = Some(parent.to_path_buf());
                }
                self.save_app_settings();
                self.status_message = format!("Saved project {}", abs_path.display());
            }
            Err(e) => {
                eprintln!("Error saving project {}", e);
                self.status_message = format!("Error saving project {} ({})", abs_path.display(), e);
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
                let scale = target_h / mv.geometry.h;
                mv.geometry.w *= scale;
                mv.geometry.h = target_h;
            }
        }

        Some(target_h)
    }

    /// 既存の拡大図について、枠全体の縦の大きさ（h）を一番左上のものに揃え、
    /// アスペクト比（w/h）を維持して拡大・縮小する (Adjust-Y)
    pub fn adjust_y_multiviews(&mut self) {
        if let Some(target_h) = self.adjust_y_multiviews_internal() {
            self.project.push_history();
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
        self.project.push_history();
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

        // 画面幅を超える場合は、アスペクト比を維持して均等縮小
        if total_needed > available_w && sum_w > 0.0 {
            let available_for_insets = (available_w - total_gap).max(count as f32 * 30.0);
            let scale = (available_for_insets / sum_w).clamp(0.05, 1.0);
            for mv in &mut self.project.state.multiviews {
                mv.geometry.w *= scale;
                mv.geometry.h *= scale;
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

        self.project.push_history();
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

        // Auto で追加したときは必ず Adjust-Y + Align を実行 (履歴は 1-step)
        self.adjust_y_multiviews_internal();
        self.align_multiviews_internal(plot_rect);
        self.project.push_history();
        self.status_message = format!(
            "Auto-created {} multiview insets from integrals (sensitivity={})",
            self.project.state.multiviews.len(),
            self.project.state.auto_sensitivity.label(),
        );
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
    pub fn open_file_dialog(&mut self) {
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

        if let Some(path) = dialog.pick_file() {
            self.open_file(path);
        }
    }

    /// 外部ダイアログ経由でフォルダを開く (Brukerデータセット等)
    pub fn open_folder_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new();

        if let Some(ref dir) = self.current_directory {
            let abs_dir = if dir.is_relative() {
                std::env::current_dir().unwrap_or_default().join(dir)
            } else {
                dir.clone()
            };
            dialog = dialog.set_directory(abs_dir);
        }

        if let Some(path) = dialog.pick_folder() {
            self.open_file(path);
        }
    }

    /// 直上書きや確認なしセーブは行わず、常に Save as rsn ダイアログを呼ぶ
    pub fn quick_save(&mut self) {
        self.save_rsn_dialog();
    }

    /// 外部ダイアログ経由でプロジェクトを保存する (Save as rsn)
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

        // ファイル名の初期値: 元データと同じ名前 (Brukerの名前はディレクトリ名)
        if let Some(ref cur) = self.current_file_path {
            let base_name = if cur.is_dir() {
                cur.file_name().and_then(|s| s.to_str()).unwrap_or("spectrum")
            } else {
                cur.file_stem().and_then(|s| s.to_str()).unwrap_or("spectrum")
            };
            dialog = dialog.set_file_name(format!("{}.rsn", base_name));
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
        if self.has_open_dialog() {
            return;
        }

        let input = ctx.input(|i| i.clone());

        let ctrl_or_cmd = input.modifiers.command || input.modifiers.ctrl;

        // Ctrl + O: Open Data
        if ctrl_or_cmd && !input.modifiers.shift && input.key_pressed(Key::O) {
            self.open_file_dialog();
        }

        // Ctrl + S: Save as rsn (常にダイアログ要求)
        if ctrl_or_cmd && input.key_pressed(Key::S) {
            self.save_rsn_dialog();
        }

        // Ctrl + Z: Undo
        if ctrl_or_cmd && !input.modifiers.shift && input.key_pressed(Key::Z) {
            if self.project.undo() {
                self.sync_action_bar_from_project();
                self.status_message = "Undo performed".to_string();
            }
        }

        // Ctrl + Y or Ctrl + Shift + Z: Redo
        if (ctrl_or_cmd && input.key_pressed(Key::Y))
            || (ctrl_or_cmd && input.modifiers.shift && input.key_pressed(Key::Z))
        {
            if self.project.redo() {
                self.sync_action_bar_from_project();
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
                    if !self.selected_multiview_ids.is_empty() {
                        let count = self.selected_multiview_ids.len();
                        self.project.state.multiviews.retain(|mv| !self.selected_multiview_ids.contains(&mv.id));
                        self.selected_multiview_ids.clear();
                        self.project.push_history();
                        self.status_message = format!("Deleted {} selected multiview inset(s)", count);
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
}

impl eframe::App for ResonaApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let prev_print_settings = self.print_dialog_state.settings.clone();
        let prev_multiview_ratio = self.action_state.multiview_ratio;

        // ezNMR / 科学NMR標準の洗練されたライトテーマを設定
        let mut visuals = egui::Visuals::light();
        visuals.window_fill = Color32::WHITE;
        visuals.panel_fill = Color32::from_rgb(248, 249, 250); // #f8f9fa
        ctx.set_visuals(visuals);

        // ウィンドウタイトルの更新: 未ロード時は "Resona"、ロード時は "Resona - (フルパス)"
        let window_title = match &self.current_file_path {
            Some(path) => format!("Resona - {}", path.display()),
            None => "Resona".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(window_title));

        self.handle_shortcuts(ctx);
        self.handle_drag_and_drop(ctx);

        let is_modal_active = self.has_open_dialog();

        // 1. トップメニューバー
        TopBottomPanel::top("top_menu")
            .frame(egui::Frame::none()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                if is_modal_active {
                    ui.disable();
                }
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Open Data... (Ctrl+O)").clicked() {
                            self.open_file_dialog();
                            ui.close_menu();
                        }
                        if ui.button("Save as rsn... (Ctrl+S)").clicked() {
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
                            if self.project.undo() {
                                self.sync_action_bar_from_project();
                            }
                            ui.close_menu();
                        }
                        if ui.button("Redo (Ctrl+Y)").clicked() {
                            if self.project.redo() {
                                self.sync_action_bar_from_project();
                            }
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Settings", |ui| {
                        if ui.button("Default").clicked() {
                            self.reset_settings_to_default();
                            ui.close_menu();
                        }
                    });
                });
            });

        // 2. モード切替ツールバー (1行目: ezNMR完全準拠のライトテーマバー + 右端 Sensitivity)
        TopBottomPanel::top("mode_toolbar")
            .frame(egui::Frame::none()
                .fill(Color32::from_rgb(248, 249, 250))
                .inner_margin(Margin::symmetric(8.0, 5.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                if is_modal_active {
                    ui.disable();
                }
                match show_mode_bar(
                    ui,
                    &mut self.mode,
                    &mut self.project.state.auto_sensitivity,
                ) {
                    ModeBarEvent::None => {}
                    ModeBarEvent::OpenFullAuto => {
                        self.full_auto_dialog_state.sensitivity = self.project.state.auto_sensitivity;
                        self.full_auto_dialog_state.baseline_choice = FullAutoBaselineChoice::None;
                        self.full_auto_dialog_state.open = true;
                    }
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

        if self.mode == Some(AppMode::Integrate) && self.action_state.integrate_submode == IntegrateSubMode::None {
            self.action_state.integrate_submode = IntegrateSubMode::Add;
        }

        // 3. コンテキスト専用アクションバー (2行目: 左 ZOOM常駐フレーム + 中央 サブツールバー + 右 Y-Axis (scale)常駐フレーム)
        let has_spectrum = self.project.spectrum_real.is_some();
        let max_intensity = self.project.spectrum_real.as_ref()
            .map(|s| s.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(1e-6))
            .unwrap_or(1.0);

        // プロット側（ズームやダイアログ適用など）で transform が変更されていたらツールバーのスケールを逆算同期
        if let Some(ref t) = self.transform {
            let current_y = (t.y_min, t.y_max);
            if self.last_transform_y != Some(current_y) {
                if t.y_max > 1e-6 {
                    self.y_max_scale = (100.0 * max_intensity / t.y_max).clamp(1.0, 10000.0);
                    self.y_min_scale = (100.0 * (-t.y_min) / max_intensity).clamp(0.0, 10000.0);
                }
                self.last_transform_y = Some(current_y);
            }
        }

        let old_max_scale = self.y_max_scale;
        let old_min_scale = self.y_min_scale;

        let mut p0 = self.project.state.p0;
        let mut p1 = self.project.state.p1;
        let mut int_scale = self.project.state.integration_scale;

        let noise_level = self.project.noise_level();

        if self.mode == Some(AppMode::Peak) && self.action_state.peak_threshold <= 0.0 {
            let factor = self.project.state.auto_sensitivity.peak_noise_factor();
            let initial_thresh = self.project.state.peak_threshold.unwrap_or(noise_level * factor);
            self.action_state.peak_threshold = initial_thresh;
            self.project.state.peak_threshold = Some(initial_thresh);
        }

        let action_event = TopBottomPanel::top("action_bar")
            .frame(egui::Frame::none()
                .fill(Color32::from_rgb(248, 249, 250))
                .inner_margin(Margin::symmetric(8.0, 4.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                if is_modal_active {
                    ui.disable();
                }
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
                    self.project.state.baseline_method,
                    &mut self.y_max_scale,
                    &mut self.y_min_scale,
                    has_spectrum,
                )
            }).inner;

        // ツールバーの Y-Axis (scale) が操作された場合、transform を更新
        if (self.y_max_scale - old_max_scale).abs() > 1e-6 || (self.y_min_scale - old_min_scale).abs() > 1e-6 {
            if let Some(ref mut t) = self.transform {
                let base_y = max_intensity;
                let top_pct = self.y_max_scale.max(1.0);
                t.y_max = base_y * (100.0 / top_pct);
                t.y_min = -base_y * (self.y_min_scale / 100.0);
                self.last_transform_y = Some((t.y_min, t.y_max));
            }
        }

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
            ActionEvent::ApplyBaseline { method } => {
                self.project.apply_baseline(method);
                self.project.push_history();
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
                self.project.push_history();
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
                    let factor = self.project.state.auto_sensitivity.peak_noise_factor();
                    let thresh = noise * factor;
                    self.action_state.peak_threshold = thresh;
                    self.project.state.peak_threshold = Some(thresh);
                    self.project.state.peaks = pick_peaks(spec, ppm, thresh, &self.project.state.peaks);
                    self.project.push_history();
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
                    self.project.push_history();
                    self.status_message = format!(
                        "Auto detected {} integration regions (sensitivity={})",
                        self.project.state.integrations.len(),
                        self.project.state.auto_sensitivity.label(),
                    );
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
            ActionEvent::AlignMultiviewsOneRow => {
                let plot_rect = self.transform.as_ref().map(|t| t.screen_rect).unwrap_or(Rect::from_min_size(Pos2::new(100.0, 100.0), egui::vec2(800.0, 600.0)));
                self.align_multiviews_one_row(plot_rect);
            }
            ActionEvent::ResetMultiview => {
                self.project.state.multiviews.clear();
                self.selected_multiview_ids.clear();
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
                if is_modal_active {
                    ui.disable();
                }
                let has_data = self.project.spectrum_real.is_some() || self.project.fid_raw.is_some();
                show_side_panel(
                    ui,
                    &self.project.metadata,
                    &self.project.state.ft_settings,
                    &mut self.project.state.j_couplings,
                    &mut self.selected_j_idx,
                    has_data,
                );
            });

        // 5. ステータスバー (下部)
        TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none()
                .fill(Color32::from_rgb(248, 249, 250))
                .inner_margin(Margin::symmetric(8.0, 3.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                if is_modal_active {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status_message).size(11.0).color(Color32::from_rgb(108, 117, 125)));
                });
            });

        // 6. メインプロット領域 (純白背景、下部80pxピークラベル領域)
        CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::WHITE))
            .show(ctx, |ui| {
                if is_modal_active {
                    ui.disable();
                }
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
                            &self.selected_multiview_ids,
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

                    if is_modal_active {
                        self.drag_start = None;
                        self.drag_current = None;
                        self.integrate_drag = None;
                        self.multiview_drag = None;
                        self.is_dragging_threshold = false;
                    } else {
                        // ダブルクリックで拡大を1つ前に戻す (Zoom モード時)
                        if response.double_clicked() && self.active_zoom.is_some() {
                            if let Some((p_min, p_max, y_min, y_max)) = self.zoom_history.pop() {
                                t.ppm_min = p_min;
                                t.ppm_max = p_max;
                                t.y_min = y_min;
                                t.y_max = y_max;
                                self.last_transform_y = Some((t.y_min, t.y_max));
                                self.status_message = "Zoom undone (double-click)".to_string();
                            } else {
                                do_reset_zoom = true;
                                self.status_message = "Zoom reset (double-click)".to_string();
                            }
                        }

                        // --- キーボード＆マウスホイール操作 ---
                        let wants_kbd = ctx.wants_keyboard_input();
                        if !wants_kbd {
                            // 1. Backspace で Zoom を 1 つ戻す
                            if ctx.input(|i| i.key_pressed(Key::Backspace)) {
                                if let Some((p_min, p_max, y_min, y_max)) = self.zoom_history.pop() {
                                    t.ppm_min = p_min;
                                    t.ppm_max = p_max;
                                    t.y_min = y_min;
                                    t.y_max = y_max;
                                    self.last_transform_y = Some((t.y_min, t.y_max));
                                    self.status_message = "Zoom undone (Backspace)".to_string();
                                } else {
                                    do_reset_zoom = true;
                                    self.status_message = "Zoom reset (Backspace)".to_string();
                                }
                            }

                            // 2. Z / X / S で一時 Zoom (押している間だけアクティブ、離すと復帰、他キーや修飾キーなし時限定)
                            let has_modifiers = ctx.input(|i| i.modifiers.any());
                            if !has_modifiers {
                                let z_down = ctx.input(|i| i.key_down(Key::Z));
                                let x_down = ctx.input(|i| i.key_down(Key::X));
                                let s_down = ctx.input(|i| i.key_down(Key::S));

                                if z_down || x_down || s_down {
                                    if self.temp_zoom_saved.is_none() {
                                        self.temp_zoom_saved = Some(self.active_zoom);
                                    }
                                    let target_tool = if z_down {
                                        ZoomTool::Rect
                                    } else if x_down {
                                        ZoomTool::X
                                    } else {
                                        ZoomTool::Y
                                    };
                                    if self.active_zoom != Some(target_tool) {
                                        self.active_zoom = Some(target_tool);
                                        self.action_state.clear_submodes();
                                    }
                                } else if let Some(saved) = self.temp_zoom_saved.take() {
                                    self.active_zoom = saved;
                                }
                            } else if let Some(saved) = self.temp_zoom_saved.take() {
                                self.active_zoom = saved;
                            }

                            // 3. 矢印キー（← / →）によるデータ範囲内スクロール、および（↑ / ↓）による Y-Scale (%) 調整
                            let is_left = ctx.input(|i| i.key_down(Key::ArrowLeft));
                            let is_right = ctx.input(|i| i.key_down(Key::ArrowRight));
                            let is_up = ctx.input(|i| i.key_down(Key::ArrowUp));
                            let is_down = ctx.input(|i| i.key_down(Key::ArrowDown));

                            let pressed_left = ctx.input(|i| i.key_pressed(Key::ArrowLeft));
                            let pressed_right = ctx.input(|i| i.key_pressed(Key::ArrowRight));
                            let pressed_up = ctx.input(|i| i.key_pressed(Key::ArrowUp));
                            let pressed_down = ctx.input(|i| i.key_pressed(Key::ArrowDown));

                            let any_arrow_down = is_left || is_right || is_up || is_down;
                            let dt = (ctx.input(|i| i.stable_dt) as f64).min(0.1);

                            if any_arrow_down {
                                ctx.request_repaint();
                                self.arrow_key_hold_time += dt;

                                let is_shift = ctx.input(|i| i.modifiers.shift);
                                let shift_mult = if is_shift { 3.0 } else { 1.0 };

                                // 長押し時の加速 (0.15秒後から最大3倍まで加速)
                                let accel_mult = if self.arrow_key_hold_time > 0.15 {
                                    let progress = ((self.arrow_key_hold_time - 0.15) / 0.8).clamp(0.0, 1.0);
                                    1.0 + (progress * progress) * 2.0
                                } else {
                                    1.0
                                };

                                // 3a. 左右キー: データ範囲 (PPM端) を超えないようにスクロール & クランプ
                                if is_left || is_right {
                                    let x_span = (t.ppm_max - t.ppm_min).abs();
                                    let continuous_rate = 0.6 * shift_mult * accel_mult * dt;
                                    let step_x_cont = x_span * continuous_rate;
                                    let discrete_rate = 0.05 * shift_mult;
                                    let step_x_disc = x_span * discrete_rate;

                                    let dx = if is_left {
                                        if pressed_left { step_x_disc } else { step_x_cont }
                                    } else {
                                        -(if pressed_right { step_x_disc } else { step_x_cont })
                                    };

                                    t.ppm_min += dx;
                                    t.ppm_max += dx;

                                    // データ範囲でクランプ
                                    let (data_min_ppm, data_max_ppm) = match self.project.ppm {
                                        Some(ref p) if !p.is_empty() => {
                                            let p0 = p[0];
                                            let p1 = p[p.len() - 1];
                                            (p0.min(p1), p0.max(p1))
                                        }
                                        _ => (-10.0, 200.0),
                                    };

                                    let cur_min = t.ppm_min.min(t.ppm_max);
                                    let cur_max = t.ppm_min.max(t.ppm_max);
                                    let width = cur_max - cur_min;
                                    let data_width = data_max_ppm - data_min_ppm;

                                    if width < data_width {
                                        let is_desc = t.ppm_min > t.ppm_max;
                                        if cur_max > data_max_ppm {
                                            let new_max = data_max_ppm;
                                            let new_min = data_max_ppm - width;
                                            if is_desc {
                                                t.ppm_min = new_max;
                                                t.ppm_max = new_min;
                                            } else {
                                                t.ppm_min = new_min;
                                                t.ppm_max = new_max;
                                            }
                                        } else if cur_min < data_min_ppm {
                                            let new_min = data_min_ppm;
                                            let new_max = data_min_ppm + width;
                                            if is_desc {
                                                t.ppm_min = new_max;
                                                t.ppm_max = new_min;
                                            } else {
                                                t.ppm_min = new_min;
                                                t.ppm_max = new_max;
                                            }
                                        }
                                    }
                                }

                                // 3b. 上下キー: Y-Scale (%) Max / Min 調整
                                if is_up || is_down {
                                    let max_intensity = self.project.max_intensity().max(1e-6);
                                    if is_shift {
                                        // Shift + 上下: Y-scale Min(%) を加速調整
                                        let step = if pressed_up || pressed_down { 2.0 } else { 25.0 * accel_mult * dt };
                                        if is_up {
                                            self.y_min_scale = (self.y_min_scale + step).clamp(0.0, 10000.0);
                                        } else {
                                            self.y_min_scale = (self.y_min_scale - step).clamp(0.0, 10000.0);
                                        }
                                    } else {
                                        // 上下: Y-scale Max(%) を加速調整
                                        let step = if pressed_up || pressed_down { 2.5 } else { 35.0 * accel_mult * dt };
                                        if is_up {
                                            self.y_max_scale = (self.y_max_scale + step).clamp(1.0, 10000.0);
                                        } else {
                                            self.y_max_scale = (self.y_max_scale - step).clamp(1.0, 10000.0);
                                        }
                                    }
                                    let top_pct = self.y_max_scale.max(1.0);
                                    t.y_max = max_intensity * (100.0 / top_pct);
                                    t.y_min = -max_intensity * (self.y_min_scale / 100.0);
                                    self.last_transform_y = Some((t.y_min, t.y_max));
                                }
                            } else {
                                self.arrow_key_hold_time = 0.0;
                            }
                        }

                        // 4. マウスホイール (通常: Y-scale Max, Shift: Y-scale Min)
                        if response.hovered() {
                            let scroll_y = ctx.input(|i| i.raw_scroll_delta.y) as f64;
                            if scroll_y.abs() > 0.1 {
                                let is_shift = ctx.input(|i| i.modifiers.shift);
                                let max_intensity = self.project.max_intensity().max(1e-6);
                                if is_shift {
                                    // Shift + ホイール: Min (%) 調整
                                    let delta = (scroll_y / 30.0).clamp(-10.0, 10.0) * 2.0;
                                    let new_min = (self.y_min_scale + delta).clamp(0.0, 10000.0);
                                    if (new_min - self.y_min_scale).abs() > 1e-4 {
                                        self.y_min_scale = new_min;
                                        let top_pct = self.y_max_scale.max(1.0);
                                        t.y_max = max_intensity * (100.0 / top_pct);
                                        t.y_min = -max_intensity * (self.y_min_scale / 100.0);
                                        self.last_transform_y = Some((t.y_min, t.y_max));
                                    }
                                } else {
                                    // 通常ホイール: Max (%) 調整
                                    let factor = 1.10_f64.powf(scroll_y / 50.0);
                                    let new_max_scale = (self.y_max_scale * factor).clamp(1.0, 10000.0);
                                    if (new_max_scale - self.y_max_scale).abs() > 1e-4 {
                                        self.y_max_scale = new_max_scale;
                                        let top_pct = self.y_max_scale.max(1.0);
                                        t.y_max = max_intensity * (100.0 / top_pct);
                                        t.y_min = -max_intensity * (self.y_min_scale / 100.0);
                                        self.last_transform_y = Some((t.y_min, t.y_max));
                                    }
                                }
                            }
                        }

                        // スレッショルドバーのホバー／ドラッグ判定 (Peak pick モード時)
                        let is_peak_mode = self.mode == Some(AppMode::Peak) && self.active_zoom.is_none();
                        let is_thresh_submode = is_peak_mode && self.action_state.peak_submode == PeakSubMode::Threshold;
                        let is_integrate_edit_mode = self.mode == Some(AppMode::Integrate)
                            && self.action_state.integrate_submode == IntegrateSubMode::Edit
                            && self.active_zoom.is_none();
                        let is_integrate_delete_mode = self.mode == Some(AppMode::Integrate)
                            && self.action_state.integrate_submode == IntegrateSubMode::Delete
                            && self.active_zoom.is_none();
                        let is_multiview_mode = self.mode == Some(AppMode::Multiview) && self.active_zoom.is_none();
                        let is_multiview_delete_mode = is_multiview_mode && self.action_state.multiview_submode == MultiviewSubMode::Delete;
                        let thresh_val = self.action_state.peak_threshold;
                        let mut near_threshold = false;

                    // Multiview ホバー判定 (外側8px枠線ゾーンまで検知)
                    self.hovered_multiview_id = None;
                    if is_multiview_mode {
                        if let Some(pos) = pointer_pos {
                            for mv in self.project.state.multiviews.iter().rev() {
                                if is_multiview_delete_mode {
                                    let rect = Rect::from_min_size(Pos2::new(mv.geometry.x, mv.geometry.y), egui::vec2(mv.geometry.w, mv.geometry.h));
                                    if rect.expand(6.0).contains(pos) {
                                        self.hovered_multiview_id = Some(mv.id.clone());
                                        ctx.set_cursor_icon(egui::CursorIcon::Default);
                                        break;
                                    }
                                } else {
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

                    if response.double_clicked() {
                        if let Some(pos) = pointer_pos {
                            if is_multiview_mode && !is_multiview_delete_mode {
                                for mv in self.project.state.multiviews.iter().rev() {
                                    let rect = Rect::from_min_size(Pos2::new(mv.geometry.x, mv.geometry.y), egui::vec2(mv.geometry.w, mv.geometry.h));
                                    if rect.contains(pos) {
                                        let p_min = mv.src_x_min.min(mv.src_x_max);
                                        let p_max = mv.src_x_min.max(mv.src_x_max);
                                        let inset_max = if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                            let mut mx = 0.0_f64;
                                            for i in 0..ppm.len().min(spec.len()) {
                                                let p = ppm[i];
                                                if p >= p_min && p <= p_max {
                                                    if spec[i] > mx {
                                                        mx = spec[i];
                                                    }
                                                }
                                            }
                                            if mx > 1e-6 { mx } else { self.project.max_intensity().max(1.0) }
                                        } else {
                                            1.0
                                        };

                                        let (y_max_scale, y_min_scale) = if let (Some(cur_ymax), Some(cur_ymin)) = (mv.src_y_max, mv.src_y_min) {
                                            let top = if cur_ymax > 1e-6 { (100.0 * inset_max / cur_ymax).clamp(1.0, 10000.0) } else { 80.0 };
                                            let min = (100.0 * (-cur_ymin) / inset_max).clamp(0.0, 10000.0);
                                            (top, min)
                                        } else {
                                            (80.0, 10.0)
                                        };

                                        self.multiview_yscale_dialog_state = MultiviewYScaleDialogState {
                                            open: true,
                                            target_id: Some(mv.id.clone()),
                                            target_label: format!("Inset: {:.3} ~ {:.3} ppm", mv.src_x_max, mv.src_x_min),
                                            auto_y: mv.src_y_max.is_none(),
                                            max_peak_intensity: inset_max,
                                            y_max_scale,
                                            y_min_scale,
                                        };
                                        break;
                                    }
                                }
                            }
                        }
                    } else if response.clicked_by(egui::PointerButton::Primary) {
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
                                            self.selected_multiview_ids.remove(hid);
                                            self.project.push_history();
                                            self.status_message = "Deleted multiview inset".to_string();
                                        }
                                    } else {
                                        let is_shift = ctx.input(|i| i.modifiers.shift);
                                        if let Some(ref hid) = self.hovered_multiview_id {
                                            if is_shift {
                                                // Shift+クリック: 複数選択のトグル
                                                if self.selected_multiview_ids.contains(hid) {
                                                    self.selected_multiview_ids.remove(hid);
                                                } else {
                                                    self.selected_multiview_ids.insert(hid.clone());
                                                }
                                            } else {
                                                // 通常クリック: 単一選択
                                                self.selected_multiview_ids.clear();
                                                self.selected_multiview_ids.insert(hid.clone());
                                            }
                                        } else if !is_shift {
                                            // 何もないところを通常クリックした場合は選択全解除
                                            self.selected_multiview_ids.clear();
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
                    if ctx.input(|i| i.key_pressed(Key::Delete))
                        && is_multiview_mode
                        && !self.selected_multiview_ids.is_empty()
                    {
                        let count = self.selected_multiview_ids.len();
                        self.project.state.multiviews.retain(|m| !self.selected_multiview_ids.contains(&m.id));
                        self.selected_multiview_ids.clear();
                        self.project.push_history();
                        self.status_message = format!("Deleted {} multiview inset(s) (Delete key)", count);
                    }

                    if let Some(pos) = pointer_pos {
                        let (cur_ppm, cur_y) = t.screen_to_data(pos);
                        self.status_message = format!("PPM {:.3}   Intensity {:.1}", cur_ppm, cur_y);

                        // Delete サブモード時のホバー判定
                        if is_integrate_delete_mode && plot_rect.contains(pos) && pos.y <= axis_y {
                            if let Some(item) = self.project.state.integrations.iter().find(|it| it.contains_ppm(cur_ppm)) {
                                ctx.set_cursor_icon(egui::CursorIcon::Default);
                                let p_min = item.start_ppm.min(item.end_ppm);
                                let p_max = item.start_ppm.max(item.end_ppm);
                                self.status_message = format!("Click to delete integration [{:.2} ~ {:.2} ppm]", p_min, p_max);
                            }
                        }

                        // Edit サブモード時のホバー判定
                        if is_integrate_edit_mode && !self.is_dragging_threshold && self.integrate_drag.is_none() && plot_rect.contains(pos) && pos.y <= axis_y {
                            let mut hit_handle = false;
                            for intg in &self.project.state.integrations {
                                let s_pos = t.data_to_screen(intg.start_ppm, intg.y_start);
                                let e_pos = t.data_to_screen(intg.end_ppm, intg.y_end);
                                let hit_s = (pos - s_pos).length() <= 18.0 || ((pos.x - s_pos.x).abs() <= 16.0 && (pos.y - s_pos.y).abs() <= 20.0);
                                let hit_e = (pos - e_pos).length() <= 18.0 || ((pos.x - e_pos.x).abs() <= 16.0 && (pos.y - e_pos.y).abs() <= 20.0);
                                if hit_s || hit_e {
                                    hit_handle = true;
                                    break;
                                }
                            }
                            if hit_handle {
                                ctx.set_cursor_icon(egui::CursorIcon::Grab);
                                self.status_message = "Drag to adjust baseline handle position".to_string();
                            } else if let (Some(spec), Some(ppm_arr)) = (&self.project.spectrum_real, &self.project.ppm) {
                                let ref_factor = self.project.state.integration_ref_value / self.project.state.integration_ref_area.max(1e-12);
                                let mut detected = None;
                                for intg in &self.project.state.integrations {
                                    if let Some(target) = detect_integrate_edit_target(
                                        pos,
                                        cur_ppm,
                                        intg,
                                        t,
                                        spec,
                                        ppm_arr,
                                        self.project.state.integration_scale,
                                        self.project.state.integration_offset,
                                        ref_factor,
                                    ) {
                                        detected = Some(target);
                                        break;
                                    }
                                }
                                match detected {
                                    Some(IntgEditTarget::Scale) => {
                                        ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                        self.status_message = "Drag up/down to adjust integration scale (height)".to_string();
                                    }
                                    Some(IntgEditTarget::Offset) => {
                                        ctx.set_cursor_icon(egui::CursorIcon::Move);
                                        self.status_message = "Drag up/down to adjust integration vertical offset".to_string();
                                    }
                                    None => {}
                                }
                            }
                        }
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

                    // 非ドラッグ時: Delete サブモードではマウス位置に縦の赤色ガイド線を表示
                    if is_integrate_delete_mode && self.drag_start.is_none() {
                        if let Some(pos) = pointer_pos {
                            if plot_rect.contains(pos) && pos.y <= axis_y {
                                let painter = ui.painter_at(plot_rect);
                                painter.line_segment(
                                    [Pos2::new(pos.x, plot_rect.min.y), Pos2::new(pos.x, axis_y)],
                                    Stroke::new(1.2_f32, Color32::from_rgb(239, 68, 68)),
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
                        } else if is_multiview_mode && !is_multiview_delete_mode {
                            self.is_dragging_threshold = false;
                            let origin = ctx.input(|i| i.pointer.press_origin()).or(pointer_pos);
                            let mut hit_mv = None;
                            if let Some(pos) = origin {
                                for mv in self.project.state.multiviews.iter().rev() {
                                    if let Some(mode) = detect_multiview_drag_mode(pos, &mv.geometry) {
                                        hit_mv = Some((mv.id.clone(), mode, mv.geometry));
                                        break;
                                    }
                                }
                            }
                            if let (Some(pos), Some((m_id, mode, geom))) = (origin, hit_mv) {
                                let is_shift = ctx.input(|i| i.modifiers.shift);
                                if !self.selected_multiview_ids.contains(&m_id) {
                                    if !is_shift {
                                        self.selected_multiview_ids.clear();
                                    }
                                    self.selected_multiview_ids.insert(m_id.clone());
                                }

                                // 移動モードの場合は選択中の全アイテムを初期位置とともに保持
                                let items = if mode == MultiviewDragMode::Move {
                                    self.project
                                        .state
                                        .multiviews
                                        .iter()
                                        .filter(|m| self.selected_multiview_ids.contains(&m.id))
                                        .map(|m| MultiviewDragItemState {
                                            id: m.id.clone(),
                                            start_rect: m.geometry,
                                        })
                                        .collect()
                                } else {
                                    vec![MultiviewDragItemState {
                                        id: m_id.clone(),
                                        start_rect: geom,
                                    }]
                                };

                                self.multiview_drag = Some(MultiviewDragState {
                                    item_id: m_id,
                                    mode,
                                    start_rect: geom,
                                    start_pointer: pos,
                                    items,
                                });
                            } else {
                                self.drag_start = origin;
                                self.drag_current = pointer_pos;
                            }
                        } else if is_integrate_edit_mode {
                            self.is_dragging_threshold = false;
                            if let Some(pos) = pointer_pos {
                                let mut hit_handle = None;
                                for (idx, intg) in self.project.state.integrations.iter().enumerate() {
                                    let s_pos = t.data_to_screen(intg.start_ppm, intg.y_start);
                                    let e_pos = t.data_to_screen(intg.end_ppm, intg.y_end);
                                    let hit_s = (pos - s_pos).length() <= 18.0 || ((pos.x - s_pos.x).abs() <= 16.0 && (pos.y - s_pos.y).abs() <= 20.0);
                                    let hit_e = (pos - e_pos).length() <= 18.0 || ((pos.x - e_pos.x).abs() <= 16.0 && (pos.y - e_pos.y).abs() <= 20.0);
                                    if hit_s {
                                        hit_handle = Some(IntegrateDragTarget::StartHandle(idx));
                                        break;
                                    } else if hit_e {
                                        hit_handle = Some(IntegrateDragTarget::EndHandle(idx));
                                        break;
                                    }
                                }
                                if let Some(target) = hit_handle {
                                    self.integrate_drag = Some(target);
                                } else if let (Some(spec), Some(ppm_arr)) = (&self.project.spectrum_real, &self.project.ppm) {
                                    let (ppm, _) = t.screen_to_data(pos);
                                    let ref_factor = self.project.state.integration_ref_value / self.project.state.integration_ref_area.max(1e-12);
                                    let mut detected = None;
                                    for intg in &self.project.state.integrations {
                                        if let Some(target) = detect_integrate_edit_target(
                                            pos,
                                            ppm,
                                            intg,
                                            t,
                                            spec,
                                            ppm_arr,
                                            self.project.state.integration_scale,
                                            self.project.state.integration_offset,
                                            ref_factor,
                                        ) {
                                            detected = Some(target);
                                            break;
                                        }
                                    }
                                    match detected {
                                        Some(IntgEditTarget::Scale) => {
                                            self.integrate_drag = Some(IntegrateDragTarget::Scale {
                                                start_scale: self.project.state.integration_scale,
                                                start_y: pos.y,
                                            });
                                        }
                                        Some(IntgEditTarget::Offset) => {
                                            self.integrate_drag = Some(IntegrateDragTarget::Offset {
                                                start_offset: self.project.state.integration_offset,
                                                start_y: pos.y,
                                            });
                                        }
                                        None => {
                                            self.integrate_drag = None;
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
                                let is_shift = ctx.input(|i| i.modifiers.shift);

                                if drag.mode == MultiviewDragMode::Move {
                                    // 拡大図の直角移動 (Shift を押しながら)
                                    let effective_delta = if is_shift {
                                        if delta.x.abs() >= delta.y.abs() {
                                            egui::vec2(delta.x, 0.0) // 水平移動
                                        } else {
                                            egui::vec2(0.0, delta.y) // 垂直移動
                                        }
                                    } else {
                                        delta
                                    };

                                    // 複数選択されたすべての拡大図を同時に移動
                                    for item in &drag.items {
                                        if let Some(mv) = self.project.state.multiviews.iter_mut().find(|m| m.id == item.id) {
                                            mv.geometry.x = item.start_rect.x + effective_delta.x;
                                            mv.geometry.y = item.start_rect.y + effective_delta.y;
                                        }
                                    }
                                } else if let Some(mv) = self.project.state.multiviews.iter_mut().find(|m| m.id == drag.item_id) {
                                    let mut r = drag.start_rect;
                                    if is_shift {
                                        // アスペクト比維持の拡大縮小
                                        let aspect = (drag.start_rect.w / drag.start_rect.h.max(1e-3)) as f32;
                                        let orig_w = drag.start_rect.w;
                                        let orig_h = drag.start_rect.h;

                                        match drag.mode {
                                            MultiviewDragMode::Move => unreachable!(),
                                            MultiviewDragMode::Right => {
                                                let new_w = (orig_w + delta.x).max(60.0);
                                                let new_h = (new_w / aspect).max(60.0);
                                                let actual_w = new_h * aspect;
                                                r.x = drag.start_rect.x;
                                                r.y = drag.start_rect.y + (orig_h - new_h) * 0.5;
                                                r.w = actual_w;
                                                r.h = new_h;
                                            }
                                            MultiviewDragMode::Left => {
                                                let new_w = (orig_w - delta.x).max(60.0);
                                                let new_h = (new_w / aspect).max(60.0);
                                                let actual_w = new_h * aspect;
                                                r.x = drag.start_rect.x + orig_w - actual_w;
                                                r.y = drag.start_rect.y + (orig_h - new_h) * 0.5;
                                                r.w = actual_w;
                                                r.h = new_h;
                                            }
                                            MultiviewDragMode::Bottom => {
                                                let new_h = (orig_h + delta.y).max(60.0);
                                                let new_w = (new_h * aspect).max(60.0);
                                                let actual_h = new_w / aspect;
                                                r.y = drag.start_rect.y;
                                                r.x = drag.start_rect.x + (orig_w - new_w) * 0.5;
                                                r.w = new_w;
                                                r.h = actual_h;
                                            }
                                            MultiviewDragMode::Top => {
                                                let new_h = (orig_h - delta.y).max(60.0);
                                                let new_w = (new_h * aspect).max(60.0);
                                                let actual_h = new_w / aspect;
                                                r.y = drag.start_rect.y + orig_h - actual_h;
                                                r.x = drag.start_rect.x + (orig_w - new_w) * 0.5;
                                                r.w = new_w;
                                                r.h = actual_h;
                                            }
                                            MultiviewDragMode::TopLeft => {
                                                let dx = -delta.x;
                                                let dy = -delta.y;
                                                let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                                let mut new_w = (orig_w + d).max(60.0);
                                                let mut new_h = new_w / aspect;
                                                if new_h < 60.0 {
                                                    new_h = 60.0;
                                                    new_w = new_h * aspect;
                                                }
                                                r.x = drag.start_rect.x + orig_w - new_w;
                                                r.y = drag.start_rect.y + orig_h - new_h;
                                                r.w = new_w;
                                                r.h = new_h;
                                            }
                                            MultiviewDragMode::TopRight => {
                                                let dx = delta.x;
                                                let dy = -delta.y;
                                                let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                                let mut new_w = (orig_w + d).max(60.0);
                                                let mut new_h = new_w / aspect;
                                                if new_h < 60.0 {
                                                    new_h = 60.0;
                                                    new_w = new_h * aspect;
                                                }
                                                r.x = drag.start_rect.x;
                                                r.y = drag.start_rect.y + orig_h - new_h;
                                                r.w = new_w;
                                                r.h = new_h;
                                            }
                                            MultiviewDragMode::BottomLeft => {
                                                let dx = -delta.x;
                                                let dy = delta.y;
                                                let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                                let mut new_w = (orig_w + d).max(60.0);
                                                let mut new_h = new_w / aspect;
                                                if new_h < 60.0 {
                                                    new_h = 60.0;
                                                    new_w = new_h * aspect;
                                                }
                                                r.x = drag.start_rect.x + orig_w - new_w;
                                                r.y = drag.start_rect.y;
                                                r.w = new_w;
                                                r.h = new_h;
                                            }
                                            MultiviewDragMode::BottomRight => {
                                                let dx = delta.x;
                                                let dy = delta.y;
                                                let d = if dx.abs() >= dy.abs() * aspect { dx } else { dy * aspect };
                                                let mut new_w = (orig_w + d).max(60.0);
                                                let mut new_h = new_w / aspect;
                                                if new_h < 60.0 {
                                                    new_h = 60.0;
                                                    new_w = new_h * aspect;
                                                }
                                                r.x = drag.start_rect.x;
                                                r.y = drag.start_rect.y;
                                                r.w = new_w;
                                                r.h = new_h;
                                            }
                                        }
                                    } else {
                                        match drag.mode {
                                            MultiviewDragMode::Move => unreachable!(),
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
                                    }
                                    mv.geometry = r;
                                }
                            }
                        } else if let Some(target) = self.integrate_drag {
                            if let Some(pos) = pointer_pos {
                                match target {
                                    IntegrateDragTarget::StartHandle(idx) => {
                                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                        if idx < self.project.state.integrations.len() {
                                            let (p, y) = t.screen_to_data(pos);
                                            self.project.state.integrations[idx].start_ppm = p;
                                            self.project.state.integrations[idx].y_start = y;
                                            self.status_message = format!("Adjusting start handle: {:.3} ppm", p);
                                        }
                                    }
                                    IntegrateDragTarget::EndHandle(idx) => {
                                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                        if idx < self.project.state.integrations.len() {
                                            let (p, y) = t.screen_to_data(pos);
                                            self.project.state.integrations[idx].end_ppm = p;
                                            self.project.state.integrations[idx].y_end = y;
                                            self.status_message = format!("Adjusting end handle: {:.3} ppm", p);
                                        }
                                    }
                                    IntegrateDragTarget::Scale { start_scale, start_y } => {
                                        ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                                        let dy = start_y - pos.y;
                                        let factor = ((dy / (plot_rect.height() * 0.25)) as f64).exp();
                                        self.project.state.integration_scale = (start_scale * factor).max(1e-12);
                                        self.status_message = format!("Integration Scale: {:.2e}", self.project.state.integration_scale);
                                    }
                                    IntegrateDragTarget::Offset { start_offset, start_y } => {
                                        ctx.set_cursor_icon(egui::CursorIcon::Move);
                                        let dy = start_y - pos.y;
                                        let d_offset = (dy / plot_rect.height()) as f64 * 0.5;
                                        self.project.state.integration_offset = start_offset + d_offset;
                                        self.status_message = format!("Integration Offset: {:.3}", self.project.state.integration_offset);
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

                                            // 4. ドラッグ中のリアルタイム積分曲線描画
                                            if (start.x - curr.x).abs() > 4.0 {
                                                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                                                    let temp_item = IntegrationItem {
                                                        id: "preview".to_string(),
                                                        start_ppm: p_s,
                                                        end_ppm: p_e,
                                                        y_start: 0.0,
                                                        y_end: 0.0,
                                                    };
                                                    let scale = if self.project.state.integrations.is_empty() || self.project.state.integration_scale == 1.0 {
                                                        if let Some(r0) = compute_integral(spec, ppm, &temp_item, 1.0, 1.0, 0.03) {
                                                            let max_spec = self.project.max_intensity();
                                                            if r0.total_area > 1e-12 && max_spec > 0.0 {
                                                                (max_spec * 0.35) / r0.total_area
                                                            } else {
                                                                1.0
                                                            }
                                                        } else {
                                                            1.0
                                                        }
                                                    } else {
                                                        self.project.state.integration_scale
                                                    };
                                                    let ref_factor = if self.project.state.integration_ref_area > 1e-12 {
                                                        self.project.state.integration_ref_value / self.project.state.integration_ref_area
                                                    } else {
                                                        1.0
                                                    };
                                                    if let Some(res) = compute_integral(spec, ppm, &temp_item, scale, ref_factor, 0.03) {
                                                        if res.ppm.len() > 1 && res.ppm.len() == res.curve_y.len() {
                                                            let mut pts: Vec<Pos2> = Vec::with_capacity(res.ppm.len());
                                                            for i in 0..res.ppm.len() {
                                                                let pos = t.data_to_screen(res.ppm[i], res.curve_y[i]);
                                                                let clamped_pos = Pos2::new(pos.x, pos.y.clamp(plot_rect.min.y, plot_rect.max.y));
                                                                pts.push(clamped_pos);
                                                            }
                                                            if pts.len() > 1 {
                                                                painter.add(egui::epaint::PathShape::line(
                                                                    pts.clone(),
                                                                    Stroke::new(2.0_f32, self.plot_style.integral_color),
                                                                ));

                                                                // 5. ドラッグ中のリアルタイム積分数値ラベル (縦書き90度回転)
                                                                let mid_x = (start.x + curr.x) * 0.5;
                                                                let val_text = format!("{:.1$}", res.normalized_value, self.plot_style.integral_decimals);
                                                                let font_intg = egui::FontId::proportional(11.0);
                                                                let galley = painter.layout_no_wrap(val_text, font_intg, self.plot_style.integral_color);
                                                                let text_len = galley.size().x;
                                                                let text_h = galley.size().y;
                                                                let min_screen_y = pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                                                                let start_y = (min_screen_y - 4.0 - text_len).max(plot_rect.min.y + 4.0);
                                                                let text_pos = Pos2::new(mid_x + text_h * 0.5, start_y);
                                                                let ts = egui::epaint::TextShape::new(text_pos, galley, self.plot_style.integral_color)
                                                                    .with_angle(std::f32::consts::FRAC_PI_2);
                                                                painter.add(ts);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
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
                                    if self.action_state.multiview_submode != MultiviewSubMode::Delete {
                                        let p1 = t.screen_to_data(start).0;
                                        let p2 = t.screen_to_data(curr).0;
                                        let p_high = p1.max(p2);
                                        let p_low = p1.min(p2);
                                        let delta_p = p_high - p_low;

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

                                        if self.action_state.multiview_submode != MultiviewSubMode::Delete {
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
                                                self.selected_multiview_ids.clear();
                                                self.selected_multiview_ids.insert(mv_id);
                                                if self.action_state.multiview_auto_align {
                                                    self.adjust_y_multiviews_internal();
                                                    self.align_multiviews_internal(plot_rect);
                                                }
                                                self.project.push_history();
                                                self.status_message = format!("Added multiview inset {:.3} ~ {:.3} ppm", p_high, p_low);
                                            }
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
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("No Data").size(24.0).strong().color(Color32::from_gray(160)));
                });
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
        ) {
            if let Some(ref fid_raw) = self.project.fid_raw {
                let raw_fid = crate::core::RawFid {
                    data: fid_raw.clone(),
                    metadata: self.project.metadata.clone(),
                    group_delay: self.project.metadata.digital_filter_delay,
                };
                match crate::core::process_raw_fid(&raw_fid, &ft_settings, 0.0, 0.0) {
                    Ok(processed) => {
                        // Re-FT 時はすべての既存解析 (Peak, Integrate, Multiview, J-Coupling, Refシフト, Baseline) をリセット
                        self.project.clear_all_processing();
                        self.action_state.clear_submodes();

                        self.project.ppm = Some(processed.ppm);
                        self.project.complex_spectrum_unphased = Some(processed.complex_spectrum_unphased);
                        self.project.state.ft_settings = ft_settings;
                        self.project.baseline_array = None;
                        self.project.state.baseline_method = crate::core::baseline::BaselineMethod::None;
                        self.sync_action_bar_from_project();

                        // フロント側から自動位相補正を自動実行
                        let (p0, p1) = self.project.auto_phase();
                        self.project.push_history();
                        self.reset_zoom();
                        self.status_message = format!("Fourier Transform applied (All analyses reset; Autophased: P0={:.2}°, P1={:.2}°)", p0, p1);
                    }
                    Err(e) => {
                        self.status_message = format!("FT error {}", e);
                    }
                }
            }
        }

        if let Some(full_auto_res) = show_full_auto_dialog(ctx, &mut self.full_auto_dialog_state) {
            // 1. Sensitivity を全体設定に反映
            self.project.state.auto_sensitivity = full_auto_res.sensitivity;

            // 2. Full Auto パイプラインを実行 (Phase -> Baseline -> Reference -> Peak -> Integrate[optional])
            let report = self.project.execute_full_auto(
                full_auto_res.baseline_method,
                full_auto_res.enable_integration,
            );

            // 3. アクションバー状態をプロジェクトに合わせて同期
            self.sync_action_bar_from_project();
            self.action_state.clear_submodes();

            // 4. Undo 履歴にコミット
            self.project.push_history();

            // 5. 設定保存
            self.save_app_settings();

            // 6. ステータスメッセージを更新
            self.status_message = format!(
                "Full Auto completed: Phase (P0={:.1}°, P1={:.1}°), Baseline ({}), Ref ({}), Peaks ({}), Integrations ({})",
                report.p0,
                report.p1,
                report.baseline_desc,
                report.reference_desc,
                report.peaks_count,
                report.integrations_count,
            );
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
            self.save_app_settings();
        }

        if let Some((text, center_ppm)) = show_jcoupling_dialog(ctx, &mut self.jcoupling_dialog_state) {
            let sort_ppm = parse_jcoupling_sort_ppm(&text).unwrap_or(center_ppm);
            self.project.state.j_couplings.push(JCouplingResultItem {
                text,
                ppm: sort_ppm,
            });
            self.project.state.j_couplings.sort_by(|a, b| {
                let ppm_a = parse_jcoupling_sort_ppm(&a.text).unwrap_or(a.ppm);
                let ppm_b = parse_jcoupling_sort_ppm(&b.text).unwrap_or(b.ppm);
                ppm_b.partial_cmp(&ppm_a).unwrap_or(std::cmp::Ordering::Equal)
            });
            self.project.push_history();
            self.status_message = "Added J-coupling multiplet to results".to_string();
        }

        if let Some(res) = show_multiview_yscale_dialog(ctx, &mut self.multiview_yscale_dialog_state) {
            if let Some(mv) = self.project.state.multiviews.iter_mut().find(|m| m.id == res.target_id) {
                mv.src_y_min = res.y_min;
                mv.src_y_max = res.y_max;
                self.project.push_history();
                self.status_message = format!("Updated Y-scale for inset {}", res.target_id);
            }
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

        if self.print_dialog_state.settings != prev_print_settings
            || (self.action_state.multiview_ratio - prev_multiview_ratio).abs() > 1e-6
        {
            self.save_app_settings();
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_app_settings();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiview_selection_toggle() {
        let mut app = ResonaApp::default();
        assert!(app.selected_multiview_ids.is_empty());

        // 単一選択
        app.selected_multiview_ids.insert("mv-1".to_string());
        assert_eq!(app.selected_multiview_ids.len(), 1);
        assert!(app.selected_multiview_ids.contains("mv-1"));

        // Shift追加
        app.selected_multiview_ids.insert("mv-2".to_string());
        assert_eq!(app.selected_multiview_ids.len(), 2);
        assert!(app.selected_multiview_ids.contains("mv-2"));

        // Shiftトグル (削除)
        app.selected_multiview_ids.remove("mv-1");
        assert_eq!(app.selected_multiview_ids.len(), 1);
        assert!(!app.selected_multiview_ids.contains("mv-1"));
        assert!(app.selected_multiview_ids.contains("mv-2"));
    }

    #[test]
    fn test_multiview_orthogonal_movement() {
        // 水平優位
        let delta = egui::vec2(100.0, 30.0);
        let eff_delta = if delta.x.abs() >= delta.y.abs() {
            egui::vec2(delta.x, 0.0)
        } else {
            egui::vec2(0.0, delta.y)
        };
        assert_eq!(eff_delta.x, 100.0);
        assert_eq!(eff_delta.y, 0.0);

        // 垂直優位
        let delta2 = egui::vec2(20.0, -80.0);
        let eff_delta2 = if delta2.x.abs() >= delta2.y.abs() {
            egui::vec2(delta2.x, 0.0)
        } else {
            egui::vec2(0.0, delta2.y)
        };
        assert_eq!(eff_delta2.x, 0.0);
        assert_eq!(eff_delta2.y, -80.0);
    }

    #[test]
    fn test_multiview_aspect_ratio_resize() {
        let orig = RectF { x: 100.0, y: 100.0, w: 200.0, h: 100.0 };
        let aspect = orig.w / orig.h; // 2.0
        assert_eq!(aspect, 2.0);

        // BottomRight ドラッグで右下へ (dx=40, dy=10)
        let delta = egui::vec2(40.0, 10.0);
        let d = if delta.x.abs() >= delta.y.abs() * aspect { delta.x } else { delta.y * aspect };
        assert_eq!(d, 40.0);
        let new_w = orig.w + d; // 240.0
        let new_h = new_w / aspect; // 120.0
        assert_eq!(new_w, 240.0);
        assert_eq!(new_h, 120.0);
        assert_eq!(new_w / new_h, aspect);
    }
}
