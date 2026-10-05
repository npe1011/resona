use egui::{Context, Key};
use crate::gui::mode::AppMode;
use super::ResonaApp;

impl ResonaApp {
    /// キーボードショートカットの処理
    pub(crate) fn handle_shortcuts(&mut self, ctx: &Context) {
        if self.has_open_dialog() {
            return;
        }

        let input = ctx.input(|i| i.clone());

        let ctrl_or_cmd = input.modifiers.command || input.modifiers.ctrl;

        // Ctrl + O: Open Data
        if ctrl_or_cmd && !input.modifiers.shift && input.key_pressed(Key::O) {
            self.open_file_dialog(ctx);
        }

        // Ctrl + S: Save
        if ctrl_or_cmd && input.key_pressed(Key::S) {
            self.handle_save(ctx);
        }

        // Ctrl + Z: Undo
        if ctrl_or_cmd && !input.modifiers.shift && input.key_pressed(Key::Z) {
            if self.project.undo() {
                self.sync_action_bar_from_project();
                self.is_dirty = true;
                self.status_message = "Undo performed".to_string();
            }
        }

        // Ctrl + Y or Ctrl + Shift + Z: Redo
        if (ctrl_or_cmd && input.key_pressed(Key::Y))
            || (ctrl_or_cmd && input.modifiers.shift && input.key_pressed(Key::Z))
        {
            if self.project.redo() {
                self.sync_action_bar_from_project();
                self.is_dirty = true;
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
                        self.push_history();
                        self.status_message = format!("Deleted {} selected multiview inset(s)", count);
                    }
                } else if mode == AppMode::JCoupling {
                    if let Some(idx) = self.selected_j_idx {
                        if idx < self.project.state.j_couplings.len() {
                            self.project.state.j_couplings.remove(idx);
                            self.selected_j_idx = None;
                            self.push_history();
                            self.status_message = "Deleted selected J-coupling result".to_string();
                        }
                    }
                }
            }
        }
    }
}
