use std::collections::HashSet;
use std::path::PathBuf;
use egui::{CentralPanel, Color32, Context, Margin, Pos2, Rect, RichText, Stroke, TopBottomPanel, SidePanel};
use crate::core::{
    compute_integral, parse_jcoupling_sort_ppm, JCouplingResultItem, MultiviewItem,
    Project,
};

use crate::gui::dialogs::{
    show_about_dialog, show_display_dialog, show_ft_dialog, show_full_auto_dialog,
    show_jcoupling_dialog, show_multiview_yscale_dialog, show_peak_list_dialog, show_print_dialog,
    AboutDialogState, DisplayDialogState, FtDialogState, FullAutoBaselineChoice,
    FullAutoDialogState, JCouplingDialogState, MultiviewYScaleDialogState, PeakListDialogState,
    PrintDialogState,
};
use crate::gui::mode::{AppMode, IntegrateSubMode, MultiviewSubMode, PeakSubMode, ZoomTool};
use crate::gui::panels::{
    show_action_bar, show_mode_bar, show_side_panel, ActionBarState, ModeBarEvent,
};
use crate::gui::plot::{paint_spectrum, PlotStyle, PlotTransform};
use crate::multispec::{show_multispec_window, MultiSpecSettings, MultiSpecState, MultiSpecUiState};

pub mod actions;
pub mod keyboard;
pub mod mouse;

pub use actions::*;
pub use mouse::*;

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
    pub peak_list_dialog_state: PeakListDialogState,
    pub print_dialog_state: PrintDialogState,
    pub about_dialog_state: AboutDialogState,

    // MultiSpec (マルチスペクトル比較独立ウィンドウ)
    pub multispec_open: bool,
    pub multispec_state: MultiSpecState,
    pub multispec_ui_state: MultiSpecUiState,
    pub multispec_settings: MultiSpecSettings,

    pub plot_style: PlotStyle,
    pub transform: Option<PlotTransform>,
    pub zoom_history: Vec<(f64, f64, f64, f64)>, // (ppm_min, ppm_max, y_min, y_max)

    // ドラッグ状態
    pub drag_start: Option<Pos2>,
    pub drag_current: Option<Pos2>,
    pub is_dragging_threshold: bool,
    pub is_dragging_pivot: bool,
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
    pub last_transform_y: Option<(f64, f64)>,
    pub arrow_key_hold_time: f64,
    pub temp_zoom_saved: Option<Option<ZoomTool>>,
    pub is_dirty: bool,
    pub show_close_confirm: bool,
    pub prev_mode: Option<AppMode>,
    pub is_file_dialog_open: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub file_dialog_result: std::sync::Arc<std::sync::Mutex<Option<FileDialogResult>>>,
    pub close_after_save: bool,
}

impl Default for ResonaApp {
    fn default() -> Self {
        let settings = crate::gui::config::AppSettings::load();
        let ms_settings = MultiSpecSettings::load();

        let mut plot_style = PlotStyle::default();
        plot_style.ppm_decimals = settings.ppm_decimals;
        plot_style.integral_decimals = settings.integral_decimals;

        let mut display_dialog_state = DisplayDialogState::default();
        display_dialog_state.ppm_decimals = settings.ppm_decimals;
        display_dialog_state.integral_decimals = settings.integral_decimals;

        let mut action_state = ActionBarState::default();
        action_state.multiview_ratio = settings.multiview_ratio;
        action_state.multiview_auto_align = settings.multiview_auto_align;

        let mut full_auto_dialog_state = FullAutoDialogState::default();
        full_auto_dialog_state.baseline_choice = settings.full_auto_baseline_choice;
        full_auto_dialog_state.airpls_log_lambda = settings.full_auto_airpls_lambda;
        full_auto_dialog_state.poly_order = settings.full_auto_poly_order;
        full_auto_dialog_state.enable_integration = settings.full_auto_integration;

        let mut print_dialog_state = PrintDialogState::default();
        print_dialog_state.settings = settings.print_settings;

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
            peak_list_dialog_state: PeakListDialogState::default(),
            print_dialog_state,
            about_dialog_state: AboutDialogState::default(),

            multispec_open: false,
            multispec_state: MultiSpecState::default(),
            multispec_ui_state: MultiSpecUiState::default(),
            multispec_settings: ms_settings,

            plot_style,
            transform: None,
            zoom_history: Vec::new(),
            drag_start: None,
            drag_current: None,
            is_dragging_threshold: false,
            is_dragging_pivot: false,
            integrate_drag: None,
            multiview_drag: None,
            selected_multiview_ids: HashSet::new(),
            hovered_multiview_id: None,
            selected_j_idx: None,
            last_multiview_ratio: settings.multiview_ratio,
            status_message: "Ready".to_string(),
            y_max_scale: 80.0,
            y_min_scale: 10.0,
            last_transform_y: None,
            arrow_key_hold_time: 0.0,
            temp_zoom_saved: None,
            is_dirty: false,
            show_close_confirm: false,
            prev_mode: None,
            is_file_dialog_open: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            file_dialog_result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            close_after_save: false,
        }
    }
}

impl ResonaApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    /// いずれかのモーダルダイアログが開いているか判定
    pub fn has_open_dialog(&self) -> bool {
        self.ft_dialog_state.open
            || self.full_auto_dialog_state.open
            || self.display_dialog_state.open
            || self.jcoupling_dialog_state.open
            || self.multiview_yscale_dialog_state.open
            || self.peak_list_dialog_state.open
            || self.print_dialog_state.is_open
            || self.print_dialog_state.style_dialog_state.is_open
            || self.about_dialog_state.open
            || self.is_file_dialog_open.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl eframe::App for ResonaApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let dialog_result = if let Ok(mut lock) = self.file_dialog_result.lock() {
            lock.take()
        } else {
            None
        };
        if let Some(res) = dialog_result {
            match res {
                FileDialogResult::OpenFile(path) => {
                    self.open_file(path);
                }
                FileDialogResult::SaveFile(path) => {
                    let ok = self.save_file(path);
                    if ok && self.close_after_save {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    self.close_after_save = false;
                }
                FileDialogResult::SaveCancelled => {
                    self.close_after_save = false;
                }
            }
        }

        let prev_print_settings = self.print_dialog_state.settings.clone();
        let prev_multiview_ratio = self.action_state.multiview_ratio;

        // ezNMR / 科学NMR標準の洗練されたライトテーマを設定
        let mut visuals = egui::Visuals::light();
        visuals.window_fill = Color32::WHITE;
        visuals.panel_fill = Color32::from_rgb(248, 249, 250); // #f8f9fa
        ctx.set_visuals(visuals);

        // モード変更検知とクリーンアップ
        if self.mode != self.prev_mode {
            self.action_state.clear_submodes();
            self.drag_start = None;
            self.drag_current = None;
            self.is_dragging_threshold = false;
            self.is_dragging_pivot = false;
            self.integrate_drag = None;
            self.multiview_drag = None;
            self.prev_mode = self.mode;
        }

        // ウィンドウ閉じる要求の検知 (Dirty Flag が立っていれば保存確認ダイアログを開く)
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.is_dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_close_confirm = true;
            }
        }

        // ウィンドウタイトルの更新: 未ロード時は "Resona"、ロード時は "Resona - (フルパス)"、未保存変更があれば "*"
        let dirty_suffix = if self.is_dirty { " *" } else { "" };
        let window_title = match &self.current_file_path {
            Some(path) => format!("Resona - {}{}", path.display(), dirty_suffix),
            None => format!("Resona{}", dirty_suffix),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(window_title));

        self.handle_shortcuts(ctx);
        self.handle_drag_and_drop(ctx);

        let is_modal_active = self.has_open_dialog() || self.show_close_confirm;

        // 1. トップメニューバー
        TopBottomPanel::top("top_menu")
            .frame(egui::Frame::none()
                .fill(Color32::WHITE)
                .inner_margin(Margin { left: 10.0, right: 10.0, top: 4.0, bottom: 4.0 })
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))))
            .show(ctx, |ui| {
                if is_modal_active {
                    ui.disable();
                }
                egui::menu::bar(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;

                    ui.menu_button("File", |ui| {
                        if ui.button("Open Data... (Ctrl+O)").clicked() {
                            self.open_file_dialog(ctx);
                            ui.close_menu();
                        }
                        if ui.button("Save as rsn... (Ctrl+S)").clicked() {
                            self.handle_save(ctx);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            if self.is_dirty {
                                self.show_close_confirm = true;
                            } else {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    });

                    ui.menu_button("Edit", |ui| {
                        if ui.button("Peak List").clicked() {
                            self.open_peak_list_dialog();
                            ui.close_menu();
                        }
                        ui.separator();
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

                    ui.menu_button("About", |ui| {
                        if ui.button("About Resona...").clicked() {
                            self.about_dialog_state.open = true;
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
                    ModeBarEvent::OpenMultiSpec => {
                        self.multispec_open = true;
                    }
                }
            });

        // 3. アクションバー (2行目: サブツールバー)
        let mut p0 = self.project.state.p0;
        let mut p1 = self.project.state.p1;
        let mut int_scale = self.project.state.integration_scale;
        let noise_level = self.project.noise_level();
        let has_spectrum = self.project.spectrum_real.is_some();

        let old_max_scale = self.y_max_scale;
        let old_min_scale = self.y_min_scale;
        let max_intensity = self.project.max_intensity().max(1e-6);

        // 前回保存された Y 範囲からユーザーが手動でスケール変更した場合は同期
        if let Some(ref t) = self.transform {
            if let Some((last_min, last_max)) = self.last_transform_y {
                if (t.y_max - last_max).abs() > 1e-6 || (t.y_min - last_min).abs() > 1e-6 {
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
                    self.y_max_scale = top_pct;
                    self.y_min_scale = min_pct;
                }
            }
        }

        // Peak ピックモードに入った直後で閾値が未初期化の場合、ノイズレベル×感度係数で自動設定
        if self.mode == Some(AppMode::Peak) && (self.action_state.peak_threshold == 0.0 || self.project.state.peak_threshold.is_none()) {
            let factor = self.project.state.auto_sensitivity.peak_noise_factor();
            let initial_thresh = noise_level * factor;
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
            self.is_dirty = true;
        }
        if (self.project.state.integration_scale - int_scale).abs() > 1e-4 {
            self.project.state.integration_scale = int_scale;
            self.is_dirty = true;
        }

        // アクションイベントの実行
        self.handle_action_event(ctx, action_event);

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

                if let Some(ref mut t) = self.transform {
                    t.screen_rect = plot_rect;

                    // プロット描画
                    if let (Some(ppm), Some(spec)) = (&self.project.ppm, &self.project.spectrum_real) {
                        let ref_factor = self.project.state.integration_ref_factor();
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
                            "No Data",
                            egui::FontId::proportional(16.0),
                            Color32::from_gray(140),
                        );
                    }
                }

                if self.transform.is_some() {
                    self.handle_plot_interaction(ctx, ui, plot_rect, is_modal_active);
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No Data").size(24.0).strong().color(Color32::from_gray(160)));
                    });
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
                        self.project.auto_pivot();
                        self.project.state.ft_settings = ft_settings;
                        self.project.baseline_array = None;
                        self.project.state.baseline_method = crate::core::baseline::BaselineMethod::None;
                        self.sync_action_bar_from_project();

                        // フロント側から自動位相補正を自動実行
                        let (p0, p1) = self.project.auto_phase();
                        self.push_history();
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
            self.push_history();

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
            self.push_history();
            self.status_message = "Added J-coupling multiplet to results".to_string();
        }

        if let Some(res) = show_multiview_yscale_dialog(ctx, &mut self.multiview_yscale_dialog_state) {
            if let Some(mv) = self.project.state.multiviews.iter_mut().find(|m| m.id == res.target_id) {
                mv.src_y_min = res.y_min;
                mv.src_y_max = res.y_max;
                self.push_history();
                self.status_message = format!("Updated Y-scale for inset {}", res.target_id);
            }
        }

        let ref_factor = self.project.state.integration_ref_factor();

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

        show_peak_list_dialog(ctx, &mut self.peak_list_dialog_state);
        show_about_dialog(ctx, &mut self.about_dialog_state);

        // MultiSpec 独立ウィンドウ (メインウィンドウをロックしない独立 OS ウィンドウ)
        if self.multispec_open {
            show_multispec_window(
                ctx,
                &mut self.multispec_open,
                &mut self.multispec_state,
                &mut self.multispec_ui_state,
                &mut self.multispec_settings,
                &mut self.print_dialog_state.style_settings,
            );
        }

        if self.show_close_confirm {
            let mut show_confirm = true;
            let mut do_save = false;
            let mut do_dont_save = false;
            let mut do_cancel = false;
            let file_label = self.current_file_path.as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("current project")
                .to_string();

            egui::Window::new("Save Changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .open(&mut show_confirm)
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);
                    ui.label(format!("Do you want to save changes to \"{}\" before closing?", file_label));
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                do_cancel = true;
                            }
                            if ui.button("Don't Save").clicked() {
                                do_dont_save = true;
                            }
                            if ui.button("Save").clicked() {
                                do_save = true;
                            }
                        });
                    });
                });

            if !show_confirm || do_cancel {
                self.show_close_confirm = false;
            } else if do_dont_save {
                self.is_dirty = false;
                self.show_close_confirm = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if do_save {
                self.close_after_save = true;
                self.handle_save(ctx);
                self.show_close_confirm = false;
            }
        }

        if self.print_dialog_state.settings != prev_print_settings
            || (self.action_state.multiview_ratio - prev_multiview_ratio).abs() > 1e-6
        {
            self.save_app_settings();
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_app_settings();
        self.multispec_settings.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RectF;

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
