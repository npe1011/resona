use egui::{vec2, Button, Color32, RichText, Stroke, Ui};

use crate::gui::mode::AppMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeBarEvent {
    None,
    OpenReFt,
    OpenDisplay,
    OpenPrint,
}

/// ezNMRライトテーマ準拠のモードボタン用スタイル
fn mode_button_style(is_selected: bool) -> (Color32, Color32, Stroke) {
    if is_selected {
        // アクティブ: 淡いブルー背景 (#e7f1ff), 濃いブルー文字 (#084298), ブルー枠線 (#86b7fe)
        (
            Color32::from_rgb(8, 66, 152),
            Color32::from_rgb(231, 241, 255),
            Stroke::new(1.0_f32, Color32::from_rgb(134, 183, 254)),
        )
    } else {
        // 通常: 白背景 (#ffffff), 濃いグレー文字 (#495057), 薄いグレー枠線 (#ced4da)
        (
            Color32::from_rgb(73, 80, 87),
            Color32::WHITE,
            Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)),
        )
    }
}

/// メインモード切替ツールバー (1行目: ezNMR完全準拠のライトテーマボタン)
pub fn show_mode_bar(ui: &mut Ui, current_mode: &mut Option<AppMode>) -> ModeBarEvent {
    let mut event = ModeBarEvent::None;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // 1. Re-FT ボタン (通常ボタン)
        let (txt, fill, strk) = mode_button_style(false);
        let btn_reft = Button::new(RichText::new("Re-FT").size(13.0).color(txt))
            .min_size(vec2(60.0, 26.0))
            .fill(fill)
            .stroke(strk)
            .rounding(3.0_f32);
        if ui.add(btn_reft).clicked() {
            event = ModeBarEvent::OpenReFt;
        }

        // 2. モード選択ボタングループ
        let modes = [
            (AppMode::Phase, "Phase"),
            (AppMode::Baseline, "Baseline"),
            (AppMode::Reference, "Reference"),
            (AppMode::Peak, "Peak Pick"),
            (AppMode::Integrate, "Integrate"),
            (AppMode::Multiview, "Multiview"),
            (AppMode::JCoupling, "J Coupling"),
        ];

        for (mode, label) in modes {
            let is_selected = *current_mode == Some(mode);
            let (text_color, fill_color, stroke) = mode_button_style(is_selected);

            let rich = if is_selected {
                RichText::new(label).strong().size(13.0).color(text_color)
            } else {
                RichText::new(label).size(13.0).color(text_color)
            };

            let button = Button::new(rich)
                .min_size(vec2(72.0, 26.0))
                .fill(fill_color)
                .stroke(stroke)
                .rounding(3.0_f32);

            if ui.add(button).clicked() {
                *current_mode = Some(mode);
            }
        }

        // 3. Display ボタン (通常ボタン)
        let (txt, fill, strk) = mode_button_style(false);
        let btn_display = Button::new(RichText::new("Display").size(13.0).color(txt))
            .min_size(vec2(60.0, 26.0))
            .fill(fill)
            .stroke(strk)
            .rounding(3.0_f32);
        if ui.add(btn_display).clicked() {
            event = ModeBarEvent::OpenDisplay;
        }

        // 4. Print ボタン (通常ボタン)
        let btn_print = Button::new(RichText::new("Print").size(13.0).color(txt))
            .min_size(vec2(50.0, 26.0))
            .fill(fill)
            .stroke(strk)
            .rounding(3.0_f32);
        if ui.add(btn_print).clicked() {
            event = ModeBarEvent::OpenPrint;
        }
    });

    event
}
