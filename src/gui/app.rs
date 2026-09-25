use std::path::{Path, PathBuf};
use egui::{CentralPanel, Color32, Context, Key, Pos2, Rect, Stroke, TopBottomPanel, SidePanel};
use crate::core::{
    analyze_multiplet, auto_detect_integrations, estimate_noise_mad, pick_peaks,
    snap_and_add_peak, IntegrationItem, JCouplingResultItem, MultiviewItem, Project, RectF,
};

use crate::gui::dialogs::{
    show_display_dialog, show_ft_dialog, show_jcoupling_dialog, DisplayDialogState, FtDialogState,
    JCouplingDialogState,
};
use crate::gui::mode::{AppMode, IntegrateSubMode, MultiviewSubMode, PeakSubMode, ZoomSubMode};
use crate::gui::panels::{
    show_action_bar, show_mode_bar, show_side_panel, ActionEvent, ActionBarState,
};
use crate::gui::plot::{paint_spectrum, PlotStyle, PlotTransform};

pub struct ResonaApp {
    pub project: Project,
    pub current_file_path: Option<PathBuf>,
    pub current_directory: Option<PathBuf>,

    pub mode: AppMode,
    pub action_state: ActionBarState,
    pub ft_dialog_state: FtDialogState,
    pub display_dialog_state: DisplayDialogState,
    pub jcoupling_dialog_state: JCouplingDialogState,

    pub plot_style: PlotStyle,
    pub transform: Option<PlotTransform>,

    // ドラッグ状態
    pub drag_start: Option<Pos2>,
    pub drag_current: Option<Pos2>,

    // 選択状態
    pub selected_multiview_id: Option<String>,
    pub selected_j_idx: Option<usize>,

    pub status_message: String,
}

impl Default for ResonaApp {
    fn default() -> Self {
        Self {
            project: Project::new(),
            current_file_path: None,
            current_directory: None,
            mode: AppMode::View,
            action_state: ActionBarState::default(),
            ft_dialog_state: FtDialogState::default(),
            display_dialog_state: DisplayDialogState::default(),
            jcoupling_dialog_state: JCouplingDialogState::default(),
            plot_style: PlotStyle::default(),
            transform: None,
            drag_start: None,
            drag_current: None,
            selected_multiview_id: None,
            selected_j_idx: None,
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

        let res = match ext.as_str() {
            "jdf" => self.project.load_jdf(p, None),
            "rsn" | "ez" => self.project.load_rsn(p),
            _ => {
                self.status_message = format!("Unsupported file extension: {}", ext);
                return;
            }
        };

        match res {
            Ok(_) => {
                self.current_file_path = Some(p.to_path_buf());
                if let Some(parent) = p.parent() {
                    self.current_directory = Some(parent.to_path_buf());
                }
                self.action_state.ref_current_ppm = None;
                self.reset_zoom();
                self.status_message = format!("Loaded: {}", p.display());
            }
            Err(e) => {
                self.status_message = format!("Error loading file: {}", e);
            }
        }
    }

    /// ファイル保存 (.rsn)
    pub fn save_file<P: AsRef<Path>>(&mut self, path: P) {
        let p = path.as_ref();
        match self.project.save_rsn(p) {
            Ok(_) => {
                self.current_file_path = Some(p.to_path_buf());
                self.status_message = format!("Saved project: {}", p.display());
            }
            Err(e) => {
                self.status_message = format!("Error saving project: {}", e);
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
            }
        }
    }

    /// 外部ダイアログ経由でファイルを開く
    pub fn open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("NMR Data (*.jdf, *.rsn, *.ez)", &["jdf", "rsn", "ez"])
            .add_filter("JEOL Raw FID (*.jdf)", &["jdf"])
            .add_filter("Resona Project (*.rsn)", &["rsn", "ez"]);

        if let Some(ref dir) = self.current_directory {
            dialog = dialog.set_directory(dir);
        }

        if let Some(path) = dialog.pick_file() {
            self.open_file(path);
        }
    }

    /// 外部ダイアログ経由でプロジェクトを保存する
    pub fn save_rsn_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Resona Project (*.rsn)", &["rsn"]);

        if let Some(ref dir) = self.current_directory {
            dialog = dialog.set_directory(dir);
        }

        // ファイル名の自動プリセット: {stem}.rsn
        if let Some(ref cur) = self.current_file_path {
            if let Some(stem) = cur.file_stem().and_then(|s| s.to_str()) {
                dialog = dialog.set_file_name(format!("{}.rsn", stem));
            }
        }

        if let Some(path) = dialog.save_file() {
            self.save_file(path);
        }
    }

    /// キーボードショートカットの処理
    fn handle_shortcuts(&mut self, ctx: &Context) {
        let input = ctx.input(|i| i.clone());

        // Ctrl / Cmd 判定
        let ctrl_or_cmd = input.modifiers.command || input.modifiers.ctrl;

        // Ctrl + O: Open
        if ctrl_or_cmd && input.key_pressed(Key::O) {
            self.open_file_dialog();
        }

        // Ctrl + S: Save
        if ctrl_or_cmd && input.key_pressed(Key::S) {
            self.save_rsn_dialog();
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

        // Home: Reset Zoom (どのモード・フォーカスでも常に動作)
        if input.key_pressed(Key::Home) {
            self.reset_zoom();
        }

        // Delete / Backspace: 選択されたMultiviewまたはJ-Coupling行の削除
        if input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace) {
            if self.mode == AppMode::Multiview {
                if let Some(ref sel_id) = self.selected_multiview_id {
                    self.project.state.multiviews.retain(|mv| mv.id != *sel_id);
                    self.selected_multiview_id = None;
                    self.project.push_history();
                    self.status_message = "Deleted selected multiview inset".to_string();
                }
            } else if self.mode == AppMode::JCoupling {
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
        self.handle_shortcuts(ctx);
        self.handle_drag_and_drop(ctx);

        // 1. トップメニューバー
        TopBottomPanel::top("top_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open... (Ctrl+O)").clicked() {
                        self.open_file_dialog();
                        ui.close_menu();
                    }
                    if ui.button("Save .rsn (Ctrl+S)").clicked() {
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
                        self.ft_dialog_state.open = true;
                        ui.close_menu();
                    }
                    if ui.button("Display Settings...").clicked() {
                        if let Some(ref t) = self.transform {
                            self.display_dialog_state.ppm_min = t.ppm_min;
                            self.display_dialog_state.ppm_max = t.ppm_max;
                            self.display_dialog_state.y_min = t.y_min;
                            self.display_dialog_state.y_max = t.y_max;
                        }
                        self.display_dialog_state.ppm_decimals = self.plot_style.ppm_decimals;
                        self.display_dialog_state.integral_decimals = self.plot_style.integral_decimals;
                        self.display_dialog_state.open = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // 2. モード切替ツールバー
        TopBottomPanel::top("mode_toolbar").show(ctx, |ui| {
            show_mode_bar(ui, &mut self.mode);
        });

        // 3. コンテキスト専用アクションバー
        let mut p0 = self.project.state.p0;
        let mut p1 = self.project.state.p1;
        let mut int_scale = self.project.state.integration_scale;

        let action_event = TopBottomPanel::top("action_bar").show(ctx, |ui| {
            show_action_bar(
                ui,
                self.mode,
                &mut self.action_state,
                &mut p0,
                &mut p1,
                &mut int_scale,
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
            ActionEvent::AutoPhase => {
                let (new_p0, new_p1) = self.project.auto_phase();
                self.project.push_history();
                self.status_message = format!("ACME Autophase applied: P0={:.2}°, P1={:.2}°", new_p0, new_p1);
            }
            ActionEvent::ResetPhase => {
                self.project.update_phase(0.0, 0.0);
                self.project.push_history();
            }
            ActionEvent::ApplyBaseline { log_lambda, p } => {
                let lam = 10.0_f64.powf(log_lambda);
                self.project.auto_baseline(lam, p);
                self.project.push_history();
                self.status_message = format!("ALS baseline corrected: λ=1e{:.1}, p={:.4}", log_lambda, p);
            }
            ActionEvent::ClearBaseline => {
                self.project.clear_baseline();
                self.project.push_history();
                self.status_message = "Baseline correction cleared".to_string();
            }
            ActionEvent::ApplyShiftReference { target_ppm } => {
                if let Some(cur) = self.action_state.ref_current_ppm {
                    self.project.set_shift_reference(cur, target_ppm);
                    self.action_state.ref_current_ppm = Some(target_ppm);
                    self.project.push_history();
                    self.status_message = format!("Chemical shift calibrated: {:.3} -> {:.3} ppm", cur, target_ppm);
                }
            }
            ActionEvent::AutoPeak => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    let noise = estimate_noise_mad(spec);
                    let thresh = noise * 10.0;
                    self.project.state.peak_threshold = Some(thresh);
                    self.project.state.peaks = pick_peaks(spec, ppm, thresh, &self.project.state.peaks);
                    self.project.push_history();
                    self.status_message = format!("Auto detected {} peaks (thresh={:.1})", self.project.state.peaks.len(), thresh);
                }
            }
            ActionEvent::PickAllPeaks => {
                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                    let thresh = self.project.state.peak_threshold.unwrap_or(0.0);
                    self.project.state.peaks = pick_peaks(spec, ppm, thresh, &self.project.state.peaks);
                    self.project.push_history();
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
                    self.project.push_history();
                    self.status_message = format!("Auto detected {} integration regions", self.project.state.integrations.len());
                }
            }
            ActionEvent::ClearIntegrations => {
                self.project.state.integrations.clear();
                self.project.push_history();
                self.status_message = "All integrations cleared".to_string();
            }
            ActionEvent::AlignMultiview => {
                // インセットをプロット上部に等間隔で横並びに自動配置
                let n = self.project.state.multiviews.len();
                if n > 0 {
                    if let Some(ref t) = self.transform {
                        let total_w = t.screen_rect.width();
                        let item_w = (total_w / (n as f32)).min(200.0).max(80.0);
                        let item_h = (item_w * 0.75).min(150.0);
                        for (i, mv) in self.project.state.multiviews.iter_mut().enumerate() {
                            mv.geometry.x = t.screen_rect.min.x + (i as f32) * (item_w + 5.0) + 10.0;
                            mv.geometry.y = t.screen_rect.min.y + 10.0;
                            mv.geometry.w = item_w;
                            mv.geometry.h = item_h;
                        }
                    }
                    self.status_message = "Aligned multiview insets".to_string();
                }
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
        }

        // 4. 右サイドパネル (メタデータ & J-Coupling)
        SidePanel::right("side_panel")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                show_side_panel(
                    ui,
                    &self.project.metadata,
                    &mut self.project.state.j_couplings,
                    &mut self.selected_j_idx,
                );
            });

        // 5. ステータスバー (下部)
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_message);
            });
        });

        // 6. メインプロット領域
        let mut should_reset_zoom = false;
        CentralPanel::default().show(ctx, |ui| {
            let available_rect = ui.available_rect_before_wrap();
            let plot_rect = Rect::from_min_max(
                available_rect.min,
                Pos2::new(available_rect.max.x, available_rect.max.y - 5.0),
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

            if let Some(ref mut t) = self.transform {
                t.screen_rect = plot_rect;

                // プロット描画
                if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                    let ref_factor = self.project.state.integration_ref_value
                        / self.project.state.integration_ref_area.max(1e-12);
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
                        &self.project.state.multiviews,
                        self.action_state.multiview_ratio,
                        self.selected_multiview_id.as_deref(),
                        self.project.state.peak_threshold,
                        &self.plot_style,
                    );
                } else {
                    let painter = ui.painter_at(plot_rect);
                    painter.rect_filled(plot_rect, 0.0, Color32::WHITE);
                    painter.text(
                        plot_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No NMR data loaded.\nDrop a .jdf or .rsn file here, or use File -> Open...",
                        egui::FontId::proportional(16.0),
                        Color32::from_gray(120),
                    );
                }

                // マウスインタラクション
                let response = ui.allocate_rect(plot_rect, egui::Sense::click_and_drag());
                let pointer_pos = response.hover_pos();

                if let Some(pos) = pointer_pos {
                    let (cur_ppm, cur_y) = t.screen_to_data(pos);
                    self.status_message = format!("PPM: {:.3}, Intensity: {:.1}", cur_ppm, cur_y);
                }

                // スクロールホイールでのズーム
                let scroll_delta = ui.input(|i| i.raw_scroll_delta);
                if scroll_delta.y.abs() > 0.0 && plot_rect.contains(pointer_pos.unwrap_or_default()) {
                    let factor = if scroll_delta.y > 0.0 { 1.15 } else { 0.85 };
                    if let Some(pivot) = pointer_pos {
                        t.zoom(pivot, factor, factor);
                    }
                }

                // 中央クリックでのリセット
                if response.middle_clicked() {
                    should_reset_zoom = true;
                }

                // 右クリックでの全体表示リセット (Zoomモード時)
                if response.secondary_clicked() && self.mode == AppMode::Zoom {
                    should_reset_zoom = true;
                }

                // ドラッグ開始
                if response.drag_started_by(egui::PointerButton::Primary) {
                    self.drag_start = pointer_pos;
                    self.drag_current = pointer_pos;
                }

                // ドラッグ中
                if response.dragged_by(egui::PointerButton::Primary) {
                    self.drag_current = pointer_pos;

                    if self.mode == AppMode::View {
                        let delta = response.drag_delta();
                        t.pan(delta);
                    }
                }

                // 右ドラッグでのスムーズズーム (Viewモード時)
                if response.dragged_by(egui::PointerButton::Secondary) && self.mode == AppMode::View {
                    let delta = response.drag_delta();
                    let factor_x = 1.0 + (delta.x as f64) * 0.01;
                    let factor_y = 1.0 - (delta.y as f64) * 0.01;
                    if let Some(pivot) = pointer_pos {
                        t.zoom(pivot, factor_x, factor_y);
                    }
                }

                // ラバーバンド描画 (ドラッグ中)
                if let (Some(start), Some(curr)) = (self.drag_start, self.drag_current) {
                    let painter = ui.painter_at(plot_rect);
                    match self.mode {
                        AppMode::Zoom => {
                            let band_rect = match self.action_state.zoom_submode {
                                ZoomSubMode::Rect => Rect::from_two_pos(start, curr),
                                ZoomSubMode::X => Rect::from_min_max(
                                    Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                    Pos2::new(start.x.max(curr.x), plot_rect.max.y),
                                ),
                                ZoomSubMode::Y => Rect::from_min_max(
                                    Pos2::new(plot_rect.min.x, start.y.min(curr.y)),
                                    Pos2::new(plot_rect.max.x, start.y.max(curr.y)),
                                ),
                            };
                            painter.rect_filled(band_rect, 0.0, self.plot_style.rubberband_color);
                            painter.rect_stroke(band_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(0, 120, 255)));
                        }
                        AppMode::Reference => {
                            painter.line_segment(
                                [Pos2::new(curr.x, plot_rect.min.y), Pos2::new(curr.x, plot_rect.max.y)],
                                Stroke::new(2.0_f32, Color32::from_rgb(220, 180, 0)),
                            );
                        }
                        AppMode::Peak => {
                            let band_rect = Rect::from_min_max(
                                Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                Pos2::new(start.x.max(curr.x), plot_rect.max.y),
                            );
                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(0, 200, 100, 30));
                        }
                        AppMode::Integrate => {
                            if self.action_state.integrate_submode == IntegrateSubMode::Add {
                                let band_rect = Rect::from_min_max(
                                    Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                    Pos2::new(start.x.max(curr.x), plot_rect.max.y),
                                );
                                painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(255, 100, 100, 40));
                            }
                        }
                        AppMode::Multiview => {
                            let band_rect = match self.action_state.multiview_submode {
                                MultiviewSubMode::AddRect => Rect::from_two_pos(start, curr),
                                MultiviewSubMode::AddX => Rect::from_min_max(
                                    Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                    Pos2::new(start.x.max(curr.x), plot_rect.max.y),
                                ),
                                _ => Rect::from_two_pos(start, curr),
                            };
                            painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(200, 0, 200, 30));
                        }
                        AppMode::JCoupling => {
                            if self.action_state.jcoupling_add_active {
                                let band_rect = Rect::from_min_max(
                                    Pos2::new(start.x.min(curr.x), plot_rect.min.y),
                                    Pos2::new(start.x.max(curr.x), plot_rect.max.y),
                                );
                                painter.rect_filled(band_rect, 0.0, Color32::from_rgba_premultiplied(0, 150, 255, 40));
                            }
                        }
                        _ => {}
                    }
                }

                // ドラッグ終了 (解放) 時の処理
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
                        let (p_start, y_start) = t.screen_to_data(start);
                        let (p_end, y_end) = t.screen_to_data(end);

                        match self.mode {
                            AppMode::Zoom => {
                                let dx = (start.x - end.x).abs();
                                let dy = (start.y - end.y).abs();
                                if dx > 5.0 || dy > 5.0 {
                                    match self.action_state.zoom_submode {
                                        ZoomSubMode::Rect => {
                                            t.ppm_min = p_start.min(p_end);
                                            t.ppm_max = p_start.max(p_end);
                                            t.y_min = y_start.min(y_end);
                                            t.y_max = y_start.max(y_end);
                                        }
                                        ZoomSubMode::X => {
                                            t.ppm_min = p_start.min(p_end);
                                            t.ppm_max = p_start.max(p_end);
                                        }
                                        ZoomSubMode::Y => {
                                            t.y_min = y_start.min(y_end);
                                            t.y_max = y_start.max(y_end);
                                        }
                                    }
                                }
                            }
                            AppMode::Reference => {
                                self.action_state.ref_current_ppm = Some(p_end);
                            }
                            AppMode::Peak => {
                                if let (Some(spec), Some(ppm)) = (&self.project.spectrum_real, &self.project.ppm) {
                                    let p_low = p_start.min(p_end);
                                    let p_high = p_start.max(p_end);
                                    if (start.x - end.x).abs() > 5.0 {
                                        if self.action_state.peak_submode == PeakSubMode::Add {
                                            // 範囲内の最大値をピークとして追加
                                            let mut best_p = p_low;
                                            let mut max_val = f64::MIN;
                                            for i in 0..ppm.len().min(spec.len()) {
                                                let p = ppm[i];
                                                if p >= p_low && p <= p_high && spec[i] > max_val {
                                                    max_val = spec[i];
                                                    best_p = p;
                                                }
                                            }
                                            if max_val > f64::MIN {
                                                self.project.state.peaks = snap_and_add_peak(spec, ppm, best_p, &self.project.state.peaks);
                                                self.project.push_history();
                                            }
                                        } else {
                                            // 範囲内のピークを削除
                                            self.project.state.peaks.retain(|pk| pk.ppm < p_low || pk.ppm > p_high);
                                            self.project.push_history();
                                        }
                                    }
                                }
                            }
                            AppMode::Integrate => {
                                if self.action_state.integrate_submode == IntegrateSubMode::Add && (start.x - end.x).abs() > 5.0 {
                                    let s_ppm = p_start.max(p_end);
                                    let e_ppm = p_start.min(p_end);
                                    let new_item = IntegrationItem {
                                        id: format!("intg-{}", self.project.state.integrations.len() + 1),
                                        start_ppm: s_ppm,
                                        end_ppm: e_ppm,
                                        y_start: 0.0,
                                        y_end: 0.0,
                                    };
                                    self.project.state.integrations.push(new_item);
                                    self.project.push_history();
                                }
                            }
                            AppMode::Multiview => {
                                if (start.x - end.x).abs() > 10.0 {
                                    let s_ppm = p_start.min(p_end);
                                    let e_ppm = p_start.max(p_end);
                                    let geom = match self.action_state.multiview_submode {
                                        MultiviewSubMode::AddRect => RectF {
                                            x: start.x.min(end.x),
                                            y: start.y.min(end.y),
                                            w: (start.x - end.x).abs().max(80.0),
                                            h: (start.y - end.y).abs().max(60.0),
                                        },
                                        _ => RectF {
                                            x: start.x.min(end.x),
                                            y: plot_rect.min.y + 20.0,
                                            w: 200.0,
                                            h: 150.0,
                                        },
                                    };
                                    let mv_id = format!("mv-{}", self.project.state.multiviews.len() + 1);
                                    self.project.state.multiviews.push(MultiviewItem {
                                        id: mv_id.clone(),
                                        src_x_min: s_ppm,
                                        src_x_max: e_ppm,
                                        geometry: geom,
                                    });
                                    self.selected_multiview_id = Some(mv_id);
                                    self.project.push_history();
                                }
                            }
                            AppMode::JCoupling => {
                                if self.action_state.jcoupling_add_active && (start.x - end.x).abs() > 5.0 {
                                    let p_low = p_start.min(p_end);
                                    let p_high = p_start.max(p_end);
                                    let freq_mhz = self.project.metadata.obs_freq_mhz;

                                    // 範囲内のピークを収集
                                    let mut peaks_hz = Vec::new();
                                    let mut intensities = Vec::new();
                                    for pk in &self.project.state.peaks {
                                        if pk.ppm >= p_low && pk.ppm <= p_high {
                                            peaks_hz.push(pk.ppm * freq_mhz);
                                            intensities.push(pk.intensity);
                                        }
                                    }

                                    let center_ppm = (p_low + p_high) * 0.5;
                                    let shift_str = format!("{:.2}", center_ppm);
                                    let shift_str_m = format!("{:.2}-{:.2}", p_low, p_high);

                                    let candidates = analyze_multiplet(
                                        peaks_hz,
                                        intensities,
                                        &shift_str,
                                        &shift_str_m,
                                        &self.project.metadata.nucleus,
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
                            _ => {}
                        }
                    }
                    self.drag_start = None;
                    self.drag_current = None;
                }
            }
        });

        if should_reset_zoom {
            self.reset_zoom();
        }

        // 7. ダイアログの表示と処理
        if let Some(ft_settings) = show_ft_dialog(ctx, &mut self.ft_dialog_state) {
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
                        self.project.state.ft_settings = ft_settings;
                        self.project.push_history();
                        self.reset_zoom();
                        self.status_message = "Fourier Transform applied with updated settings".to_string();
                    }
                    Err(e) => {
                        self.status_message = format!("FT error: {}", e);
                    }
                }
            }
        }

        if let Some((p_min, p_max, y_min, y_max, p_dec, i_dec)) = show_display_dialog(ctx, &mut self.display_dialog_state) {
            if let Some(ref mut t) = self.transform {
                t.ppm_min = p_min;
                t.ppm_max = p_max;
                t.y_min = y_min;
                t.y_max = y_max;
            }
            self.plot_style.ppm_decimals = p_dec;
            self.plot_style.integral_decimals = i_dec;
        }

        if let Some((text, center_ppm)) = show_jcoupling_dialog(ctx, &mut self.jcoupling_dialog_state) {
            self.project.state.j_couplings.push(JCouplingResultItem {
                text,
                ppm: center_ppm,
            });
            self.project.push_history();
            self.status_message = "Added J-coupling multiplet to results".to_string();
        }
    }
}
