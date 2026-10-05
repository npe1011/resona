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
use crate::multispec::state::MultiSpecState;

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

    let viewport_id = ViewportId::from_hash_of("resona_multispec_viewport");
    let viewport_builder = ViewportBuilder::default()
        .with_title("Resona - MultiSpec Comparison")
        .with_inner_size([1150.0, 780.0])
        .with_min_inner_size([700.0, 480.0]);

    ctx.show_viewport_immediate(viewport_id, viewport_builder, |ctx, _class| {
        // ウィンドウ閉じる要求の検知 -> 状態を空にリセット
        if ctx.input(|i| i.viewport().close_requested()) {
            *open = false;
            *state = MultiSpecState::default();
        }

        // キーボードショートカットの処理 (Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+Z, Ctrl+Y, Ctrl+W)
        let (shortcut_new, shortcut_open, shortcut_save, shortcut_undo, shortcut_redo, shortcut_close) =
            ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::W),
                )
            });

        if shortcut_new {
            action_new(state);
        }
        if shortcut_open {
            action_open_rsn(state);
        }
        if shortcut_save {
            action_save_rsm(state);
        }
        if shortcut_undo {
            action_undo(state);
        }
        if shortcut_redo {
            action_redo(state);
        }
        if shortcut_close {
            *open = false;
            *state = MultiSpecState::default();
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
                    }
                } else if ext == "rsm" {
                    if let Ok(loaded) = load_rsm(&path) {
                        *state = loaded;
                    }
                }
            }
        }

        // 2. メニューバー
        TopBottomPanel::top("multispec_menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Multi Spec (Ctrl+N)").clicked() {
                        action_new(state);
                        ui.close_menu();
                    }
                    if ui.button("Open RSN... (Ctrl+O)").clicked() {
                        action_open_rsn(state);
                        ui.close_menu();
                    }
                    if ui.button("Save as RSM... (Ctrl+S)").clicked() {
                        action_save_rsm(state);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Close (Ctrl+W)").clicked() {
                        *open = false;
                        *state = MultiSpecState::default();
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

        let modal_locked = ui_state.print_dialog_state.is_open
            || ui_state.display_dialog_state.is_open
            || ui_state.y_scale_dialog_state.is_open;

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
            show_multispec_print_dialog(
                ctx,
                &mut ui_state.print_dialog_state,
                state,
                &mut settings.print_settings,
                style_settings,
            );
        }
    });
}

fn action_new(state: &mut MultiSpecState) {
    *state = MultiSpecState::default();
    state.push_history();
}

fn action_open_rsn(state: &mut MultiSpecState) {
    if let Some(paths) = rfd::FileDialog::new()
        .set_title("Add Spectrum from RSN File(s)")
        .add_filter("Resona Project", &["rsn"])
        .pick_files()
    {
        let was_empty = state.items.is_empty();
        for path in paths {
            let _ = state.add_rsn_file(&path);
        }
        if was_empty && !state.items.is_empty() {
            state.set_stack();
        }
        state.update_common_ppm_range();
        state.push_history();
    }
}

fn action_save_rsm(state: &mut MultiSpecState) {
    let default_name = if let Some(first) = state.items.first() {
        format!("{}_multispec.rsm", first.name.replace(' ', "_"))
    } else {
        "comparison.rsm".to_string()
    };

    if let Some(path) = rfd::FileDialog::new()
        .set_title("Save MultiSpec Archive (.rsm)")
        .add_filter("MultiSpec Archive", &["rsm"])
        .set_file_name(&default_name)
        .save_file()
    {
        let _ = save_rsm(state, &path);
    }
}

fn action_undo(state: &mut MultiSpecState) {
    state.undo();
}

fn action_redo(state: &mut MultiSpecState) {
    state.redo();
}

