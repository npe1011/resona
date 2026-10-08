use egui::{
    vec2, Button, Color32, Frame, Margin, Pos2, Rect, RichText, Rounding, ScrollArea, Stroke, Ui,
};

use crate::multispec::gui::window::MultiSpecUiState;
use crate::multispec::io::{reload_all_from_disk, reload_item_from_disk};
use crate::multispec::state::{clean_path, MultiSpecState};

enum SelectionAction {
    Single(String),
    Toggle(String),
    Range(String),
}

pub fn show_multispec_sidebar(ui: &mut Ui, state: &mut MultiSpecState, ui_state: &mut MultiSpecUiState) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;

        // 1. 最上部: 比較タイトル入力フィールド
        ui.horizontal(|ui| {
            ui.label(RichText::new("Title").size(12.0).strong().color(Color32::from_rgb(33, 37, 41)));
            let mut title_text = state.title.clone();
            let avail_w = ui.available_width();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut title_text)
                    .desired_width(avail_w)
                    .hint_text("Title (optional)..."),
            );
            if resp.changed() {
                state.title = title_text;
            }
            if resp.lost_focus() {
                state.push_history();
            }
        });

        ui.add_space(2.0);

        // 2. スペクトル件数ヘッダー & Reload All ボタン
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Spectra ({})", state.items.len()))
                    .size(13.0)
                    .strong()
                    .color(Color32::from_rgb(33, 37, 41)),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        Button::new(RichText::new("Reload All").size(11.0))
                            .min_size(vec2(65.0, 20.0))
                            .rounding(3.0_f32),
                    )
                    .on_hover_text("Reload all linked spectra from disk")
                    .clicked()
                {
                    reload_all_from_disk(state);
                }
            });
        });

        ui.separator();

        // 3. スペクトル一覧スクロール領域 (カードのドラッグ＆ドロップ並び替え対応)
        let scroll_output = ScrollArea::vertical()
            .auto_shrink([false, false])
            .drag_to_scroll(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;

                let mouse_pos = ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
                let is_primary_down = ui.input(|i| i.pointer.primary_down());
                let any_released = ui.input(|i| i.pointer.any_released());

                let mut to_remove = None;
                let mut any_changed = false;
                let mut selection_action = None;

                let selected_id = state.selected_id.clone();
                let selected_ids = state.selected_ids.clone();
                let mut card_rects = Vec::with_capacity(state.items.len());

                for item in state.items.iter_mut() {
                    let is_selected = selected_ids.contains(&item.id) || selected_id.as_deref() == Some(&item.id);
                    let is_drag_origin = ui_state.dragging_item_id.as_deref() == Some(&item.id);
                    let is_being_dragged = is_drag_origin || (ui_state.dragging_item_id.is_some() && is_selected);

                    let border_color = if is_being_dragged {
                        Color32::from_rgb(13, 110, 253)
                    } else if is_selected {
                        Color32::from_rgb(13, 110, 253)
                    } else {
                        Color32::from_rgb(222, 226, 230)
                    };

                    let bg_color = if is_being_dragged {
                        Color32::from_rgb(238, 244, 255)
                    } else if is_selected {
                        Color32::from_rgb(240, 247, 255)
                    } else {
                        Color32::WHITE
                    };

                    let card_frame = Frame::none()
                        .fill(bg_color)
                        .stroke(Stroke::new(if is_selected || is_being_dragged { 1.5_f32 } else { 1.0_f32 }, border_color))
                        .rounding(4.0_f32)
                        .inner_margin(Margin { left: 4.0, right: 6.0, top: 5.0, bottom: 5.0 });

                    let card_response = card_frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;

                            // --- 左側: ドラッグハンドル領域 (幅20px, 高さ86px) ---
                            let (handle_rect, handle_resp) = ui.allocate_exact_size(
                                vec2(20.0, 86.0),
                                egui::Sense::click_and_drag(),
                            );

                            if handle_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            if handle_resp.clicked() {
                                let is_shift = ui.input(|i| i.modifiers.shift);
                                let is_ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                                if is_shift {
                                    selection_action = Some(SelectionAction::Range(item.id.clone()));
                                } else if is_ctrl {
                                    selection_action = Some(SelectionAction::Toggle(item.id.clone()));
                                } else {
                                    selection_action = Some(SelectionAction::Single(item.id.clone()));
                                }
                            }
                            if handle_resp.drag_started() || (handle_resp.dragged() && ui_state.dragging_item_id.is_none()) {
                                ui_state.dragging_item_id = Some(item.id.clone());
                                if !is_selected {
                                    selection_action = Some(SelectionAction::Single(item.id.clone()));
                                }
                            }

                            // ハンドル背景描画
                            let handle_bg = if is_being_dragged {
                                Color32::from_rgb(190, 220, 255)
                            } else if handle_resp.hovered() {
                                Color32::from_gray(228)
                            } else {
                                Color32::from_gray(245)
                            };

                            let painter = ui.painter();
                            painter.rect_filled(
                                handle_rect,
                                Rounding { nw: 3.0, ne: 0.0, sw: 3.0, se: 0.0 },
                                handle_bg,
                            );

                            // 中央に 6 つのドットを描画 (文字化けフリー)
                            let h_center = handle_rect.center();
                            let dot_col = if is_being_dragged {
                                Color32::from_rgb(13, 110, 253)
                            } else if handle_resp.hovered() {
                                Color32::from_gray(80)
                            } else {
                                Color32::from_gray(150)
                            };
                            for dx in [-3.0, 3.0] {
                                for dy in [-8.0, 0.0, 8.0] {
                                    painter.circle_filled(
                                        Pos2::new(h_center.x + dx, h_center.y + dy),
                                        1.6,
                                        dot_col,
                                    );
                                }
                            }

                            // --- 右側: カードコンテンツ (4段) ---
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 4.0;

                                // 1段目: 可視性チェックボックス、カラーピッカー、右端削除ボタン [X]
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;

                                    if ui.checkbox(&mut item.visible, "").on_hover_text("Toggle visibility").changed() {
                                        any_changed = true;
                                    }

                                    if ui.color_edit_button_srgb(&mut item.color).on_hover_text("Change spectrum color").changed() {
                                        any_changed = true;
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let btn_x = Button::new(RichText::new("X").size(10.5).color(Color32::from_rgb(220, 53, 69)))
                                            .min_size(vec2(20.0, 20.0))
                                            .rounding(2.0);
                                        if ui.add(btn_x).on_hover_text("Remove spectrum").clicked() {
                                            to_remove = Some(item.id.clone());
                                        }
                                    });
                                });

                                // 2段目: スペクトル名 (直接編集テキスト入力) & Integral チェックボックス
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;

                                    let mut name_text = item.name.clone();
                                    let avail_w = ui.available_width();
                                    let text_w = (avail_w - 75.0).max(60.0);
                                    let name_resp = ui.add(
                                        egui::TextEdit::singleline(&mut name_text)
                                            .font(egui::FontId::proportional(12.0))
                                            .desired_width(text_w),
                                    );
                                    if name_resp.changed() {
                                        item.name = name_text;
                                    }
                                    if name_resp.lost_focus() {
                                        any_changed = true;
                                    }

                                    if ui.checkbox(&mut item.show_integral, "Integral").on_hover_text("Show/hide integral curves and values").changed() {
                                        any_changed = true;
                                    }
                                });

                                // 3段目: 参照ステータスバッジ & Reload ボタン & フルパス
                                if let Some(ref path) = item.source_path {
                                    let exists = path.exists();
                                    let full_path_str = clean_path(path).display().to_string();

                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        let (status_label, status_col) = if exists {
                                            ("[Linked]", Color32::from_rgb(22, 163, 74))
                                        } else {
                                            ("[Missing]", Color32::from_rgb(202, 138, 4))
                                        };
                                        ui.label(RichText::new(status_label).size(10.0).strong().color(status_col));

                                        if exists {
                                            let btn_reload = Button::new(RichText::new("Reload").size(10.0))
                                                .min_size(vec2(44.0, 16.0))
                                                .rounding(2.0);
                                            if ui.add(btn_reload).on_hover_text("Reload from original RSN file").clicked() {
                                                reload_item_from_disk(item);
                                                any_changed = true;
                                            }
                                        }
                                    });

                                    let path_label = ui
                                        .add(
                                            egui::Label::new(
                                                RichText::new(&full_path_str)
                                                    .size(9.5)
                                                    .color(Color32::from_gray(115)),
                                            )
                                            .truncate()
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_text(&full_path_str);
                                    if path_label.clicked() {
                                        let is_shift = ui.input(|i| i.modifiers.shift);
                                        let is_ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                                        if is_shift {
                                            selection_action = Some(SelectionAction::Range(item.id.clone()));
                                        } else if is_ctrl {
                                            selection_action = Some(SelectionAction::Toggle(item.id.clone()));
                                        } else {
                                            selection_action = Some(SelectionAction::Single(item.id.clone()));
                                        }
                                    }
                                } else {
                                    ui.label(RichText::new("[Embedded]").size(10.0).color(Color32::from_rgb(100, 116, 139)));
                                }

                                // 4段目: Y-Scale Max / Min 数値直接編集テキストボックス (コロン排除)
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.label(RichText::new("Y-Scale").size(10.5).color(Color32::from_gray(100)));

                                    ui.label(RichText::new("Max").size(10.5));
                                    let resp_max = ui.add(
                                        egui::DragValue::new(&mut item.y_scale_max)
                                            .speed(1.0)
                                            .range(1.0..=10000.0)
                                            .suffix("%"),
                                    ).on_hover_text("Click to type value manually, or drag to adjust");
                                    if resp_max.changed() {
                                        any_changed = true;
                                    }
                                    if resp_max.lost_focus() {
                                        any_changed = true;
                                    }

                                    ui.label(RichText::new("Min").size(10.5));
                                    let resp_min = ui.add(
                                        egui::DragValue::new(&mut item.y_scale_min)
                                            .speed(1.0)
                                            .range(-1000.0..=10000.0)
                                            .suffix("%"),
                                    ).on_hover_text("Click to type value manually, or drag to adjust");
                                    if resp_min.changed() {
                                        any_changed = true;
                                    }
                                    if resp_min.lost_focus() {
                                        any_changed = true;
                                    }
                                });
                            });
                        });
                    });

                    let card_rect = card_response.response.rect;
                    card_rects.push(card_rect);

                    let card_interact = ui.interact(card_rect, ui.id().with("card_click").with(&item.id), egui::Sense::click());
                    if card_interact.clicked() && to_remove.is_none() {
                        let is_shift = ui.input(|i| i.modifiers.shift);
                        let is_ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                        if is_shift {
                            selection_action = Some(SelectionAction::Range(item.id.clone()));
                        } else if is_ctrl {
                            selection_action = Some(SelectionAction::Toggle(item.id.clone()));
                        } else {
                            selection_action = Some(SelectionAction::Single(item.id.clone()));
                        }
                    }
                }

                if let Some(action) = selection_action {
                    match action {
                        SelectionAction::Single(id) => state.select_single(id),
                        SelectionAction::Toggle(id) => state.toggle_select(&id),
                        SelectionAction::Range(id) => state.select_range(&id),
                    }
                }

                // --- ドラッグ中の処理 (ドロップ先判定、ゴースト表示、青い挿入ライン描画) ---
                if let Some(ref from_id) = ui_state.dragging_item_id.clone() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    ui.ctx().request_repaint(); // ドラッグ中も毎フレーム再描画

                    let from_idx = state.items.iter().position(|it| it.id == *from_id);

                    // 1. スロットの連続的判定 (カードの中心 Y 座標を境界として判定、隙間ゼロ)
                    if let Some(pos) = mouse_pos {
                        let mut slot = card_rects.len();
                        for (i, rect) in card_rects.iter().enumerate() {
                            if pos.y < rect.center().y {
                                slot = i;
                                break;
                            }
                        }
                        ui_state.drop_target_slot = Some(slot);
                    }

                    // 2. 有効な挿入先であれば、青い挿入インジケーターラインを描画
                    if let (Some(slot), Some(f_idx)) = (ui_state.drop_target_slot, from_idx) {
                        let is_multi = state.selected_ids.len() > 1 && state.is_selected(from_id);
                        let is_noop = !is_multi && (slot == f_idx || slot == f_idx + 1);
                        if !is_noop && !card_rects.is_empty() {
                            let indicator_y = if slot == 0 {
                                card_rects[0].min.y - 3.0
                            } else if slot >= card_rects.len() {
                                card_rects.last().unwrap().max.y + 3.0
                            } else {
                                (card_rects[slot - 1].max.y + card_rects[slot].min.y) * 0.5
                            };

                            let min_x = card_rects[0].min.x;
                            let max_x = card_rects[0].max.x;
                            let painter = ui.painter();
                            painter.line_segment(
                                [Pos2::new(min_x, indicator_y), Pos2::new(max_x, indicator_y)],
                                Stroke::new(3.5_f32, Color32::from_rgb(13, 110, 253)),
                            );
                            painter.circle_filled(Pos2::new(min_x, indicator_y), 4.0, Color32::from_rgb(13, 110, 253));
                            painter.circle_filled(Pos2::new(max_x, indicator_y), 4.0, Color32::from_rgb(13, 110, 253));
                        }
                    }

                    // 3. マウス追従ゴーストカード描画 (フローティング表示)
                    if let (Some(pos), Some(f_idx)) = (mouse_pos, from_idx) {
                        let drag_item = &state.items[f_idx];
                        let is_multi = state.selected_ids.len() > 1 && state.is_selected(&drag_item.id);
                        let ghost_w = if is_multi { 210.0 } else { 190.0 };
                        let ghost_rect = Rect::from_min_size(pos + vec2(12.0, 10.0), vec2(ghost_w, 36.0));
                        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.id().with("dnd_ghost")));

                        painter.rect_filled(ghost_rect.translate(vec2(2.0, 2.0)), 4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 35));
                        painter.rect_filled(ghost_rect, 4.0, Color32::from_rgba_unmultiplied(245, 250, 255, 240));
                        painter.rect_stroke(ghost_rect, 4.0, Stroke::new(1.5_f32, Color32::from_rgb(13, 110, 253)));

                        let dot_color = Color32::from_rgb(drag_item.color[0], drag_item.color[1], drag_item.color[2]);
                        painter.circle_filled(ghost_rect.left_center() + vec2(14.0, 0.0), 4.5, dot_color);

                        let display_name = if is_multi {
                            format!("{} (+{} others)", drag_item.name, state.selected_ids.len() - 1)
                        } else {
                            drag_item.name.clone()
                        };

                        painter.text(
                            ghost_rect.left_center() + vec2(25.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            &display_name,
                            egui::FontId::proportional(12.0),
                            Color32::from_rgb(33, 37, 41),
                        );
                    }
                }

                // ドロップ時の並び替え確定処理
                if ui_state.dragging_item_id.is_some() && (!is_primary_down || any_released) {
                    if let Some(from_id) = ui_state.dragging_item_id.take() {
                        let target_slot = ui_state.drop_target_slot.take();
                        if let Some(slot) = target_slot {
                            if state.selected_ids.len() > 1 && state.is_selected(&from_id) {
                                state.move_items_to(&state.selected_ids.clone(), slot);
                            } else if let Some(from_idx) = state.items.iter().position(|it| it.id == from_id) {
                                let to_idx = if slot > from_idx {
                                    slot.saturating_sub(1)
                                } else {
                                    slot
                                };
                                if from_idx != to_idx && to_idx < state.items.len() {
                                    state.move_item_to(from_idx, to_idx);
                                    state.select_single(from_id);
                                }
                            }
                        }
                        ui.ctx().request_repaint(); // 即座に再描画
                    }
                }

                if any_changed {
                    state.push_history();
                }

                if let Some(id) = to_remove {
                    state.remove_item(&id);
                }

                // カード下部の空き領域をクリックした場合は選択全解除
                let avail = ui.available_size();
                if avail.y > 0.0 {
                    let (_, empty_resp) = ui.allocate_exact_size(avail, egui::Sense::click());
                    if empty_resp.clicked() && ui_state.dragging_item_id.is_none() {
                        state.clear_selection();
                        ui.ctx().request_repaint();
                    }
                }

                card_rects
            });

        // カードとカードの隙間やサイドバー背景の余白クリックによる選択全解除
        if ui.input(|i| i.pointer.primary_clicked()) && ui_state.dragging_item_id.is_none() {
            if let Some(click_pos) = ui.input(|i| i.pointer.interact_pos()) {
                if scroll_output.inner_rect.contains(click_pos) {
                    let hit_card = scroll_output.inner.iter().any(|r| r.contains(click_pos));
                    if !hit_card {
                        state.clear_selection();
                        ui.ctx().request_repaint();
                    }
                }
            }
        }
    });
}

