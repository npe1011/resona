use egui::{CentralPanel, Context, Pos2, SidePanel, TopBottomPanel, ViewportBuilder, ViewportId};

use crate::gui::dialogs::print_style_dialog::PrintStyleSettings;
use crate::multispec::gui::display_dialog::{show_multispec_display_dialog, MultiSpecDisplayDialogState};
use crate::multispec::gui::plot::show_multispec_plot;
use crate::multispec::gui::print_dialog::{show_multispec_print_dialog, MultiSpecPrintDialogState};
use crate::multispec::gui::sidebar::show_multispec_sidebar;
use crate::multispec::gui::toolbar::{show_multispec_toolbar, MultiSpecToolbarEvent, MultiSpecZoomMode};
use crate::multispec::gui::y_scale_dialog::{show_multispec_y_scale_dialog, MultiSpecYScaleDialogState};
use crate::multispec::io::{load_rsm, save_rsm};
use crate::multispec::settings::MultiSpecSettings;
use crate::multispec::state::{clean_path, MultiSpecState};

/// MultiSpec ウィンドウ固有の UI インタラクション状態
pub struct MultiSpecUiState {
    pub zoom_mode: MultiSpecZoomMode,
    pub drag_start: Option<Pos2>,
    pub drag_current: Option<Pos2>,
    pub sidebar_visible: bool,
    pub print_dialog_state: MultiSpecPrintDialogState,
    pub display_dialog_state: MultiSpecDisplayDialogState,
    pub y_scale_dialog_state: MultiSpecYScaleDialogState,
    pub dragging_item_id: Option<String>,
    pub drop_target_slot: Option<usize>,
    pub show_close_confirm: bool,
}

impl Default for MultiSpecUiState {
    fn default() -> Self {
        Self {
            zoom_mode: MultiSpecZoomMode::None,
            drag_start: None,
            drag_current: None,
            sidebar_visible: true,
            print_dialog_state: MultiSpecPrintDialogState::default(),
            display_dialog_state: MultiSpecDisplayDialogState::default(),
            y_scale_dialog_state: MultiSpecYScaleDialogState::default(),
            dragging_item_id: None,
            drop_target_slot: None,
            show_close_confirm: false,
        }
    }
}

/// OS 独立ウィンドウ (eframe Multi-viewport) として MultiSpec 画面を表示する
pub fn show_multispec_window(
    ctx: &Context,
    open: &mut bool,
    state: &mut MultiSpecState,
    ui_state: &mut MultiSpecUiState,
    settings: &mut MultiSpecSettings,
    style_settings: &mut PrintStyleSettings,
) {
    if !*open {
        return;
    }

    let dirty_suffix = if state.is_dirty { " *" } else { "" };
    let win_title = if let Some(ref path) = state.rsm_path {
        format!("Resona MultiSpec - {}{}", clean_path(path).display(), dirty_suffix)
    } else {
        format!("Resona MultiSpec{}", dirty_suffix)
    };

    let viewport_id = ViewportId::from_hash_of("resona_multispec_viewport");
    let viewport_builder = ViewportBuilder::default()
        .with_title(win_title.clone())
        .with_inner_size([1150.0, 780.0])
        .with_min_inner_size([700.0, 480.0]);

    ctx.show_viewport_immediate(viewport_id, viewport_builder, |ctx, _class| {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(win_title.clone()));

        // ウィンドウ閉じる要求の検知 -> 未保存なら確認ダイアログ
        if ctx.input(|i| i.viewport().close_requested()) {
            if state.is_dirty && !state.items.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ui_state.show_close_confirm = true;
            } else {
                close_multispec_window(open, state, ui_state);
            }
        }

        let modal_locked = ui_state.print_dialog_state.is_open
            || ui_state.display_dialog_state.is_open
            || ui_state.y_scale_dialog_state.is_open
            || ui_state.show_close_confirm;

        // キーボードショートカットの処理 (モーダルダイアログ非表示時のみ)
        let (shortcut_new, shortcut_open_rsn, shortcut_open_rsm, shortcut_save, shortcut_undo, shortcut_redo, shortcut_close) =
            if !modal_locked {
                ctx.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                        i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::O),
                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
                        i.consume_key(egui::Modifiers::COMMAND, egui::Key::W),
                    )
                })
            } else {
                (false, false, false, false, false, false, false)
            };

        if shortcut_new {
            action_new(state, ui_state);
        }
        if shortcut_open_rsn {
            action_open_rsn(state, settings);
        }
        if shortcut_open_rsm {
            action_open_rsm(state, settings);
        }
        if shortcut_save {
            action_save_rsm(state, settings);
        }
        if shortcut_undo {
            action_undo(state);
        }
        if shortcut_redo {
            action_redo(state);
        }
        if shortcut_close {
            if state.is_dirty && !state.items.is_empty() {
                ui_state.show_close_confirm = true;
            } else {
                close_multispec_window(open, state, ui_state);
            }
        }

        // 1. ファイルドラッグ＆ドロップ受け入れ (.rsn / .rsm)
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped_files {
            if let Some(path) = file.path {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if ext == "rsn" {
                    if state.add_rsn_file(&path).is_ok() {
                        if state.items.len() == 1 {
                            state.set_stack();
                        }
                        state.update_common_ppm_range();
                        state.push_history();
                        if state.rsm_path.is_none() {
                            if let Some(parent) = path.parent() {
                                settings.current_directory = Some(parent.to_path_buf());
                                settings.save();
                            }
                        }
                    }
                } else if ext == "rsm" {
                    if let Ok(mut loaded) = load_rsm(&path) {
                        loaded.rsm_path = Some(clean_path(&path));
                        loaded.is_dirty = false;
                        *state = loaded;
                        if let Some(parent) = path.parent() {
                            settings.current_directory = Some(parent.to_path_buf());
                            settings.save();
                        }
                    }
                }
            }
        }

        // 2. メニューバー
        TopBottomPanel::top("multispec_menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Multi Spec (Ctrl+N)").clicked() {
                        action_new(state, ui_state);
                        ui.close_menu();
                    }
                    if ui.button("Open RSN... (Ctrl+O)").clicked() {
                        action_open_rsn(state, settings);
                        ui.close_menu();
                    }
                    if ui.button("Open RSM... (Ctrl+Shift+O)").clicked() {
                        action_open_rsm(state, settings);
                        ui.close_menu();
                    }
                    if ui.button("Save as RSM... (Ctrl+S)").clicked() {
                        action_save_rsm(state, settings);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Close (Ctrl+W)").clicked() {
                        if state.is_dirty && !state.items.is_empty() {
                            ui_state.show_close_confirm = true;
                        } else {
                            close_multispec_window(open, state, ui_state);
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    ui.add_enabled_ui(state.history.can_undo(), |ui| {
                        if ui.button("Undo (Ctrl+Z)").clicked() {
                            action_undo(state);
                            ui.close_menu();
                        }
                    });
                    ui.add_enabled_ui(state.history.can_redo(), |ui| {
                        if ui.button("Redo (Ctrl+Y)").clicked() {
                            action_redo(state);
                            ui.close_menu();
                        }
                    });
                });
            });
        });

        // 3. ウィンドウ上部ツールバー
        let mut toolbar_event = MultiSpecToolbarEvent::None;
        TopBottomPanel::top("multispec_top_panel").show(ctx, |ui| {
            if modal_locked {
                ui.disable();
            }
            toolbar_event = show_multispec_toolbar(ui, state, &mut ui_state.zoom_mode, ui_state.sidebar_visible);
        });

        // ツールバーイベントのハンドリング
        match toolbar_event {
            MultiSpecToolbarEvent::None => {}
            MultiSpecToolbarEvent::ToggleSidebar => {
                ui_state.sidebar_visible = !ui_state.sidebar_visible;
            }
            MultiSpecToolbarEvent::Stack => {
                state.set_stack();
                state.push_history();
            }
            MultiSpecToolbarEvent::Overlay => {
                state.set_overlay();
            }
            MultiSpecToolbarEvent::UnifyYScale => {
                ui_state.y_scale_dialog_state.open_from(state);
            }
            MultiSpecToolbarEvent::ResetZoom => {
                state.update_common_ppm_range();
                state.push_history();
            }
            MultiSpecToolbarEvent::Display => {
                ui_state.display_dialog_state.open_from(state, settings);
            }
            MultiSpecToolbarEvent::Print => {
                ui_state.print_dialog_state.open();
            }
        }

        // 3. ウィンドウ左側サイドバー
        if ui_state.sidebar_visible {
            SidePanel::left("multispec_left_panel")
                .default_width(260.0)
                .min_width(200.0)
                .max_width(450.0)
                .resizable(!modal_locked)
                .show(ctx, |ui| {
                    if modal_locked {
                        ui.disable();
                    }
                    show_multispec_sidebar(ui, state, ui_state);
                });
        }

        // 4. ウィンドウ中央スタックプロット
        CentralPanel::default().show(ctx, |ui| {
            if modal_locked {
                ui.disable();
            }
            show_multispec_plot(
                ui,
                state,
                settings,
                &mut ui_state.zoom_mode,
                &mut ui_state.drag_start,
                &mut ui_state.drag_current,
            );
        });

        // 5. 表示設定ダイアログ (モーダル)
        show_multispec_display_dialog(ctx, &mut ui_state.display_dialog_state, state, settings);

        // 6. 一括 Y-Scale 設定ダイアログ (モーダル)
        show_multispec_y_scale_dialog(ctx, &mut ui_state.y_scale_dialog_state, state);

        // 7. 印刷 / SVG 出力ダイアログ
        if ui_state.print_dialog_state.is_open {
            settings.print_settings.ppm_decimals = settings.ppm_decimals;
            settings.print_settings.integral_decimals = settings.integral_decimals;
            settings.print_settings.auto_ticks = settings.auto_ticks;
            settings.print_settings.tick_major = settings.tick_major;
            settings.print_settings.tick_minor = settings.tick_minor;

            show_multispec_print_dialog(
                ctx,
                &mut ui_state.print_dialog_state,
                state,
                &mut settings.print_settings,
                style_settings,
            );
        }

        // 8. 保存確認ダイアログ
        if ui_state.show_close_confirm {
            let mut show_confirm = true;
            let mut do_cancel = false;
            let mut do_dont_save = false;
            let mut do_save = false;
            let label = if let Some(ref path) = state.rsm_path {
                path.file_name().and_then(|s| s.to_str()).unwrap_or("comparison.rsm").to_string()
            } else {
                "current MultiSpec project".to_string()
            };

            egui::Window::new("Save MultiSpec Changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .open(&mut show_confirm)
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);
                    ui.label(format!("Do you want to save changes to \"{}\" before closing?", label));
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
                ui_state.show_close_confirm = false;
            } else if do_dont_save {
                ui_state.show_close_confirm = false;
                close_multispec_window(open, state, ui_state);
            } else if do_save {
                if action_save_rsm(state, settings) {
                    ui_state.show_close_confirm = false;
                    close_multispec_window(open, state, ui_state);
                }
            }
        }
    });
}

fn close_multispec_window(
    open: &mut bool,
    state: &mut MultiSpecState,
    ui_state: &mut MultiSpecUiState,
) {
    *open = false;
    *state = MultiSpecState::default();
    *ui_state = MultiSpecUiState::default();
}

fn action_new(state: &mut MultiSpecState, ui_state: &mut MultiSpecUiState) {
    *state = MultiSpecState::default();
    *ui_state = MultiSpecUiState::default();
    state.is_dirty = false;
    state.push_history();
    state.is_dirty = false;
}

fn action_open_rsn(state: &mut MultiSpecState, settings: &mut MultiSpecSettings) {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Add Spectrum from RSN File(s)")
        .add_filter("Resona Project", &["rsn"]);
    if let Some(ref dir) = settings.current_directory {
        dialog = dialog.set_directory(dir);
    }
    if let Some(paths) = dialog.pick_files() {
        let was_empty = state.items.is_empty();
        for path in &paths {
            let _ = state.add_rsn_file(path);
        }
        if was_empty && !state.items.is_empty() {
            state.set_stack();
        }
        state.update_common_ppm_range();
        state.push_history();

        // RSM が既に存在する場合はカレントディレクトリを更新せず、なければ最初の RSN の親ディレクトリで更新
        if state.rsm_path.is_none() {
            if let Some(first_path) = paths.first() {
                if let Some(parent) = first_path.parent() {
                    settings.current_directory = Some(parent.to_path_buf());
                    settings.save();
                }
            }
        }
    }
}

fn action_open_rsm(state: &mut MultiSpecState, settings: &mut MultiSpecSettings) {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Open MultiSpec Archive (.rsm)")
        .add_filter("MultiSpec Archive", &["rsm"]);
    if let Some(ref dir) = settings.current_directory {
        dialog = dialog.set_directory(dir);
    }
    if let Some(path) = dialog.pick_file() {
        if let Ok(mut loaded) = load_rsm(&path) {
            loaded.rsm_path = Some(clean_path(&path));
            loaded.is_dirty = false;
            *state = loaded;
            if let Some(parent) = path.parent() {
                settings.current_directory = Some(parent.to_path_buf());
                settings.save();
            }
        }
    }
}

fn action_save_rsm(state: &mut MultiSpecState, settings: &mut MultiSpecSettings) -> bool {
    let default_name = if let Some(ref rsm) = state.rsm_path {
        rsm.file_name().and_then(|s| s.to_str()).unwrap_or("comparison.rsm").to_string()
    } else if let Some(first) = state.items.first() {
        format!("{}_multispec.rsm", first.name.replace(' ', "_"))
    } else {
        "comparison.rsm".to_string()
    };

    let mut dialog = rfd::FileDialog::new()
        .set_title("Save MultiSpec Archive (.rsm)")
        .add_filter("MultiSpec Archive", &["rsm"])
        .set_file_name(&default_name);
    if let Some(ref dir) = settings.current_directory {
        dialog = dialog.set_directory(dir);
    }
    if let Some(path) = dialog.save_file() {
        if save_rsm(state, &path).is_ok() {
            state.rsm_path = Some(clean_path(&path));
            state.is_dirty = false;
            if let Some(parent) = path.parent() {
                settings.current_directory = Some(parent.to_path_buf());
                settings.save();
            }
            true
        } else {
            false
        }
    } else {
        false
    }
}

fn action_undo(state: &mut MultiSpecState) {
    state.undo();
}

fn action_redo(state: &mut MultiSpecState) {
    state.redo();
}

