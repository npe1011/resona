use egui::{
    vec2, Button, Color32, Frame, Margin, RichText, Stroke, Ui,
};

use crate::multispec::state::{MultiSpecState, StackLayoutMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiSpecZoomMode {
    None,
    ZoomX,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MultiSpecToolbarEvent {
    None,
    ToggleSidebar,
    Stack,
    Overlay,
    UnifyYScale,
    ResetZoom,
    Display,
    Print,
}

fn light_button(ui: &mut Ui, text: &str, is_active: bool, min_width: f32) -> egui::Response {
    let (text_color, fill_color, stroke) = if is_active {
        (
            Color32::from_rgb(8, 66, 152),
            Color32::from_rgb(231, 241, 255),
            Stroke::new(1.0_f32, Color32::from_rgb(134, 183, 254)),
        )
    } else {
        (
            Color32::from_rgb(73, 80, 87),
            Color32::WHITE,
            Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)),
        )
    };

    let rich = if is_active {
        RichText::new(text).strong().size(12.0).color(text_color)
    } else {
        RichText::new(text).size(12.0).color(text_color)
    };

    let btn = Button::new(rich)
        .min_size(vec2(min_width, 22.0))
        .fill(fill_color)
        .stroke(stroke)
        .rounding(3.0_f32);

    ui.add(btn)
}

pub fn show_multispec_toolbar(
    ui: &mut Ui,
    state: &mut MultiSpecState,
    zoom_mode: &mut MultiSpecZoomMode,
    sidebar_visible: bool,
) -> MultiSpecToolbarEvent {
    let mut event = MultiSpecToolbarEvent::None;

    let frame_style = Frame::none()
        .fill(Color32::from_rgb(248, 249, 250))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230)))
        .rounding(4.0_f32)
        .inner_margin(Margin::symmetric(6.0, 4.0));

    frame_style.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // 1. サイドバートグル
            if light_button(ui, "Sidebar", sidebar_visible, 60.0).clicked() {
                event = MultiSpecToolbarEvent::ToggleSidebar;
            }

            ui.separator();

            // 2. Zoom X モード
            let is_x = *zoom_mode == MultiSpecZoomMode::ZoomX;
            if light_button(ui, "Zoom X", is_x, 58.0).clicked() {
                *zoom_mode = if is_x { MultiSpecZoomMode::None } else { MultiSpecZoomMode::ZoomX };
            }

            // 3. Reset Zoom
            if light_button(ui, "Reset Zoom", false, 72.0).clicked() {
                *zoom_mode = MultiSpecZoomMode::None;
                event = MultiSpecToolbarEvent::ResetZoom;
            }

            ui.separator();

            // 4. レイアウト (Stack, Overlay)
            let is_stack = !state.is_overlay;
            if light_button(ui, "Stack", is_stack, 56.0).clicked() {
                event = MultiSpecToolbarEvent::Stack;
            }

            // Stack モードが有効な場合、すぐ横に Fit / Scroll 切り替え
            if is_stack {
                let is_fit = state.stack_mode == StackLayoutMode::Fit;
                let is_scroll = state.stack_mode == StackLayoutMode::Scroll;

                if light_button(ui, "Fit", is_fit, 40.0)
                    .on_hover_text("Fit all spectra inside window automatically")
                    .clicked()
                {
                    state.stack_mode = StackLayoutMode::Fit;
                    state.push_history();
                }

                if light_button(ui, "Scroll", is_scroll, 46.0)
                    .on_hover_text("Fixed height with vertical scrollbar")
                    .clicked()
                {
                    state.stack_mode = StackLayoutMode::Scroll;
                    state.push_history();
                }

                if is_scroll {
                    ui.add(
                        egui::DragValue::new(&mut state.fixed_slot_height)
                            .speed(1.0)
                            .range(60.0..=500.0)
                            .suffix(" px")
                    ).on_hover_text("Height per spectrum in scroll mode");
                }
            }

            let is_overlay = state.is_overlay;
            if light_button(ui, "Overlay", is_overlay, 58.0).clicked() {
                event = MultiSpecToolbarEvent::Overlay;
            }

            ui.separator();

            if light_button(ui, "Y-Scale", false, 60.0)
                .on_hover_text("Unify Y-Scale (Max / Min) across all spectra")
                .clicked()
            {
                event = MultiSpecToolbarEvent::UnifyYScale;
            }

            // 右寄せで Display と Print ボタンを配置
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if light_button(ui, "Print", false, 58.0).clicked() {
                    event = MultiSpecToolbarEvent::Print;
                }
                if light_button(ui, "Display", false, 58.0).clicked() {
                    event = MultiSpecToolbarEvent::Display;
                }
            });
        });
    });

    event
}
