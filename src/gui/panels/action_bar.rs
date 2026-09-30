use egui::{
    vec2, Button, Color32, DragValue, Frame, Margin, RichText, Slider, Stroke, Ui,
};

use crate::core::analysis::KNOWN_SOLVENTS;
use crate::core::baseline::BaselineMethod;
use crate::gui::mode::{AppMode, IntegrateSubMode, MultiviewSubMode, PeakSubMode, ZoomTool};

#[derive(Debug, Clone, PartialEq)]
pub enum ActionEvent {
    None,
    ResetZoom,
    UndoZoom,
    AutoPhase,
    ResetPhase,
    ApplyBaseline { method: BaselineMethod },
    ClearBaseline,
    AutoReference,
    ApplyShiftReference { peak_ppm: f64, target_ppm: f64 },
    AutoPeak,
    PickPeaks { threshold: f64 },
    ClearPeaks,
    AutoIntegrate,
    ClearIntegrations,
    AutoMultiview,
    AdjustYMultiview,
    AlignMultiview,
    AlignMultiviewsOneRow,
    ResetMultiview,
    ClearJCoupling,
    CloseMode,
}

/// 専用アクションバーの表示コンポーネント
pub struct ActionBarState {
    pub peak_submode: PeakSubMode,
    pub integrate_submode: IntegrateSubMode,
    pub multiview_submode: MultiviewSubMode,
    pub jcoupling_add_active: bool,
    pub non_integer_protons: bool,

    // パラメータ
    pub baseline_method_kind: usize, // 0: airPLS, 1: Polynomial
    pub baseline_log_lambda: f64,    // Stiffness (log10 λ)
    pub baseline_poly_order: usize,  // 多項式次数
    pub baseline_applied: bool,

    // Reference モード
    pub ref_solvent_idx: usize,
    pub ref_target_ppm: f64,
    pub ref_set_active: bool,

    // Peak モード
    pub peak_threshold: f64,

    // Integrate モード
    pub integration_ref_val: f64,

    // Multiview モード
    pub multiview_ratio: f64,
    pub multiview_auto_align: bool,
}

impl ActionBarState {
    /// ズームツール起動時や Esc 押下時にすべてのサブモードを解除する
    pub fn clear_submodes(&mut self) {
        self.peak_submode = PeakSubMode::None;
        self.integrate_submode = IntegrateSubMode::None;
        self.multiview_submode = MultiviewSubMode::None;
        self.jcoupling_add_active = false;
        self.ref_set_active = false;
    }
}

impl Default for ActionBarState {
    fn default() -> Self {
        Self {
            peak_submode: PeakSubMode::None,
            integrate_submode: IntegrateSubMode::Add,
            multiview_submode: MultiviewSubMode::None,
            jcoupling_add_active: false,
            non_integer_protons: false,
            baseline_method_kind: 0,
            baseline_log_lambda: 8.0,
            baseline_poly_order: 3,
            baseline_applied: false,
            ref_solvent_idx: 0,
            ref_target_ppm: 7.26,
            ref_set_active: false,
            peak_threshold: 0.0,
            integration_ref_val: 1.0,
            multiview_ratio: 3.0,
            multiview_auto_align: true,
        }
    }
}

/// ezNMRライトテーマ準拠のボタン用スタイル
fn light_button(
    ui: &mut Ui,
    text: &str,
    is_active: bool,
    min_width: f32,
) -> egui::Response {
    let (text_color, fill_color, stroke) = if is_active {
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

/// モード終了用の Close ボタン (淡い赤背景・赤文字で識別しやすいデザイン)
fn close_button(ui: &mut Ui) -> egui::Response {
    let rich = RichText::new("Close").size(12.0).strong().color(Color32::from_rgb(185, 28, 28));
    let btn = Button::new(rich)
        .min_size(vec2(46.0, 22.0))
        .fill(Color32::from_rgb(254, 242, 242)) // #fef2f2
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(252, 165, 165))) // #fca5a5
        .rounding(3.0_f32);

    ui.add(btn)
}

/// Auto処理用の強調ボタン (パープル背景・白文字・パープル枠線で強調)
fn auto_button(ui: &mut Ui, text: &str, min_width: f32) -> egui::Response {
    let rich = RichText::new(text).strong().size(12.0).color(Color32::WHITE);
    let btn = Button::new(rich)
        .min_size(vec2(min_width, 22.0))
        .fill(Color32::from_rgb(126, 34, 206)) // #7e22ce (purple-700)
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(107, 33, 168))) // #6b21a8
        .rounding(3.0_f32);

    ui.add(btn)
}

/// スライダー微調整用の連続変化対応ボタン (< / >, ▲ / ▼ など)
/// クリック時はbase_step分変化し、押し続けると指定時間で加速したのち等速で動き続ける
pub fn continuous_step_button(
    ui: &mut Ui,
    id_salt: &str,
    text: &str,
    base_step: f64,
    min_speed: f64,
    max_speed: f64,
    accel_duration: f64,
) -> f64 {
    let id = ui.make_persistent_id(id_salt);
    let rich = RichText::new(text).strong().size(11.0).color(Color32::from_rgb(73, 80, 87));
    let btn = Button::new(rich)
        .min_size(vec2(18.0, 22.0))
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218)))
        .rounding(3.0_f32)
        .sense(egui::Sense::click_and_drag());
    let response = ui.add(btn);

    let active_key = egui::Id::new("continuous_step_active_id");
    let active_val: Option<u64> = ui.data_mut(|d| d.get_temp(active_key));
    let my_val = id.value();
    let is_primary_down = ui.input(|i| i.pointer.primary_down());

    // このボタン上で押された瞬間の検知
    if (response.is_pointer_button_down_on() || (response.hovered() && ui.input(|i| i.pointer.primary_pressed())))
        && active_val.is_none()
    {
        ui.data_mut(|d| d.insert_temp(active_key, my_val));
    }

    let is_this_active = active_val == Some(my_val);
    let dt = (ui.input(|i| i.stable_dt) as f64).min(0.1);
    let mut delta = 0.0_f64;

    if is_this_active && is_primary_down {
        ui.ctx().request_repaint();
        let hold_time: f64 = ui.data_mut(|d| d.get_temp::<f64>(id).unwrap_or(0.0));

        if hold_time == 0.0 {
            // 初回押下フレーム: 1ステップ分適用
            delta = base_step;
            ui.data_mut(|d| d.insert_temp(id, 0.0001_f64));
        } else {
            let next_hold = hold_time + dt;
            ui.data_mut(|d| d.insert_temp(id, next_hold));

            // ディレイ 0.25秒後から加速・等速フェーズ
            const DELAY: f64 = 0.25;
            if next_hold > DELAY {
                let t = next_hold - DELAY;
                // accel_duration 秒間で min_speed から max_speed まで加速し、以降は等速運動
                let progress = (t / accel_duration.max(0.001)).clamp(0.0, 1.0);
                let speed = min_speed + (max_speed - min_speed) * progress;
                delta = speed * dt;
            }
        }
    } else {
        if is_this_active {
            ui.data_mut(|d| d.remove_temp::<u64>(active_key));
        }
        ui.data_mut(|d| d.remove_temp::<f64>(id));
    }

    delta
}

/// ステップボタンの幾何学矢印方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepDirection {
    Up,
    Down,
}

/// 幾何学三角形を描画する長押し加速対応ステップボタン (文字化けゼロ・全OS完全互換)
pub fn continuous_step_arrow_button(
    ui: &mut Ui,
    id_salt: &str,
    direction: StepDirection,
    base_step: f64,
    min_speed: f64,
    max_speed: f64,
    accel_duration: f64,
) -> f64 {
    let id = ui.make_persistent_id(id_salt);
    let size = vec2(18.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());

    let bg_fill = if response.is_pointer_button_down_on() {
        Color32::from_rgb(222, 226, 230)
    } else if response.hovered() {
        Color32::from_rgb(241, 243, 245)
    } else {
        Color32::WHITE
    };
    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(206, 212, 218));
    ui.painter().rect(rect, 3.0_f32, bg_fill, stroke);

    let center = rect.center();
    let arrow_color = Color32::from_rgb(73, 80, 87);
    let points = match direction {
        StepDirection::Up => vec![
            egui::pos2(center.x, center.y - 3.5),
            egui::pos2(center.x - 3.5, center.y + 3.0),
            egui::pos2(center.x + 3.5, center.y + 3.0),
        ],
        StepDirection::Down => vec![
            egui::pos2(center.x - 3.5, center.y - 3.0),
            egui::pos2(center.x + 3.5, center.y - 3.0),
            egui::pos2(center.x, center.y + 3.5),
        ],
    };
    ui.painter().add(egui::Shape::convex_polygon(points, arrow_color, Stroke::NONE));

    let active_key = egui::Id::new("continuous_step_active_id");
    let active_val: Option<u64> = ui.data_mut(|d| d.get_temp(active_key));
    let my_val = id.value();
    let is_primary_down = ui.input(|i| i.pointer.primary_down());

    if (response.is_pointer_button_down_on() || (response.hovered() && ui.input(|i| i.pointer.primary_pressed())))
        && active_val.is_none()
    {
        ui.data_mut(|d| d.insert_temp(active_key, my_val));
    }

    let is_this_active = active_val == Some(my_val);
    let dt = (ui.input(|i| i.stable_dt) as f64).min(0.1);
    let mut delta = 0.0_f64;

    if is_this_active && is_primary_down {
        ui.ctx().request_repaint();
        let hold_time: f64 = ui.data_mut(|d| d.get_temp::<f64>(id).unwrap_or(0.0));

        if hold_time == 0.0 {
            delta = base_step;
            ui.data_mut(|d| d.insert_temp(id, 0.0001_f64));
        } else {
            let next_hold = hold_time + dt;
            ui.data_mut(|d| d.insert_temp(id, next_hold));

            const DELAY: f64 = 0.20;
            if next_hold > DELAY {
                let t = next_hold - DELAY;
                let progress = (t / accel_duration.max(0.001)).clamp(0.0, 1.0);
                let speed = min_speed + (max_speed - min_speed) * (progress * progress);
                delta = speed * dt;
            }
        }
    } else {
        if is_this_active {
            ui.data_mut(|d| d.remove_temp::<u64>(active_key));
        }
        ui.data_mut(|d| d.remove_temp::<f64>(id));
    }

    delta
}

/// ezNMR 2段目ツールバーの描画 (左: 常駐ZOOMフレーム, 中央: コンテキストフレーム, 右: 常駐Y-Axisスケールフレーム)
pub fn show_action_bar(
    ui: &mut Ui,
    active_mode: Option<AppMode>,
    active_zoom: &mut Option<ZoomTool>,
    state: &mut ActionBarState,
    p0: &mut f64,
    p1: &mut f64,
    integration_scale: &mut f64,
    nucleus: &str,
    noise_level: f64,
    baseline_method: BaselineMethod,
    y_max_scale: &mut f64,
    y_min_scale: &mut f64,
    has_spectrum: bool,
) -> ActionEvent {
    let mut event = ActionEvent::None;

    let frame_style = Frame::none()
        .fill(Color32::from_rgb(248, 249, 250)) // #f8f9fa
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(222, 226, 230))) // #dee2e6
        .rounding(4.0_f32)
        .inner_margin(Margin::symmetric(6.0, 3.0));

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // -------------------------------------------------------------
        // 1. 右端: Y-Scale (%) フレーム (常駐・一行表示)
        // -------------------------------------------------------------
        frame_style.show(ui, |ui| {
            ui.add_enabled_ui(has_spectrum, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;

                    // ※ right_to_left レイアウト内のため、右端の要素から順に追加することで
                    // 画面上では左から右へ
                    // [Y-Scale (%)]  Max [入力] [▲] [▼]   Min [入力] [▲] [▼]
                    // と正しく整列する。

                    // 1. Min 下ボタン (▼) [値を下げる]
                    let ymin_down = continuous_step_arrow_button(
                        ui,
                        "ab_ymin_dec",
                        StepDirection::Down,
                        2.0,
                        15.0,
                        150.0,
                        1.0,
                    );
                    if ymin_down > 0.0 {
                        *y_min_scale = (*y_min_scale - ymin_down).clamp(0.0, 10000.0);
                    }

                    // 2. Min 上ボタン (▲) [値を上げる]
                    let ymin_up = continuous_step_arrow_button(
                        ui,
                        "ab_ymin_inc",
                        StepDirection::Up,
                        2.0,
                        15.0,
                        150.0,
                        1.0,
                    );
                    if ymin_up > 0.0 {
                        *y_min_scale = (*y_min_scale + ymin_up).clamp(0.0, 10000.0);
                    }

                    // 3. Min 入力欄 (絶対値表示 0.0..=10000.0)
                    ui.add(
                        DragValue::new(y_min_scale)
                            .speed(1.0)
                            .range(0.0..=10000.0),
                    );

                    // 4. Min ラベル
                    ui.label(
                        RichText::new("Min")
                            .size(11.5)
                            .color(Color32::from_rgb(108, 117, 125)),
                    );

                    ui.add_space(5.0);

                    // 5. Max 下ボタン (▼)
                    let ymax_dec = continuous_step_arrow_button(
                        ui,
                        "ab_ymax_dec",
                        StepDirection::Down,
                        2.5,
                        15.0,
                        250.0,
                        1.0,
                    );
                    if ymax_dec > 0.0 {
                        *y_max_scale = (*y_max_scale - ymax_dec).clamp(1.0, 10000.0);
                    }

                    // 6. Max 上ボタン (▲)
                    let ymax_inc = continuous_step_arrow_button(
                        ui,
                        "ab_ymax_inc",
                        StepDirection::Up,
                        2.5,
                        15.0,
                        250.0,
                        1.0,
                    );
                    if ymax_inc > 0.0 {
                        *y_max_scale = (*y_max_scale + ymax_inc).clamp(1.0, 10000.0);
                    }

                    // 7. Max 入力欄
                    ui.add(
                        DragValue::new(y_max_scale)
                            .speed(1.0)
                            .range(1.0..=10000.0),
                    );

                    // 8. Max ラベル
                    ui.label(
                        RichText::new("Max")
                            .size(11.5)
                            .color(Color32::from_rgb(108, 117, 125)),
                    );

                    ui.add_space(3.0);

                    // 9. Y-Scale (%) タイトルラベル [一番左端]
                    ui.label(
                        RichText::new("Y-Scale (%)")
                            .size(11.5)
                            .strong()
                            .color(Color32::from_rgb(52, 58, 64)),
                    );
                });
            });
        });

        // -------------------------------------------------------------
        // 2. 残り領域: 左から右へ (ZOOM フレーム + モード別サブツールバー)
        // -------------------------------------------------------------
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            // 左側: ZOOM フレーム (常駐)
            frame_style.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;

                // ヘッダーラベル: "ZOOM" (小さな大文字グレー太字, コロンなし)
                ui.label(
                    RichText::new("ZOOM")
                        .size(10.0)
                        .strong()
                        .color(Color32::from_rgb(108, 117, 125)),
                );

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;

                    let is_rect = *active_zoom == Some(ZoomTool::Rect);
                    if light_button(ui, "X-Y", is_rect, 46.0).clicked() {
                        *active_zoom = Some(ZoomTool::Rect);
                        state.clear_submodes();
                    }

                    let is_x = *active_zoom == Some(ZoomTool::X);
                    if light_button(ui, "X", is_x, 34.0).clicked() {
                        *active_zoom = Some(ZoomTool::X);
                        state.clear_submodes();
                    }

                    let is_y = *active_zoom == Some(ZoomTool::Y);
                    if light_button(ui, "Y", is_y, 34.0).clicked() {
                        *active_zoom = Some(ZoomTool::Y);
                        state.clear_submodes();
                    }

                    if light_button(ui, "Reset", false, 48.0).clicked() {
                        event = ActionEvent::ResetZoom;
                    }
                });
            });
        });

        // -------------------------------------------------------------
        // 右側: モード専用コンテキストフレーム
        // -------------------------------------------------------------
        if let Some(mode) = active_mode {
            frame_style.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;

                    // モード別ヘッダーラベル (小さな大文字グレー太字, コロンなし)
                    let header_text = match mode {
                        AppMode::Phase => "PHASE CORRECTION",
                        AppMode::Baseline => "BASELINE CORRECTION",
                        AppMode::Reference => "REFERENCE",
                        AppMode::Peak => "PEAK PICKING",
                        AppMode::Integrate => "INTEGRATION",
                        AppMode::Multiview => "MULTIVIEW",
                        AppMode::JCoupling => "J COUPLING",
                    };

                    ui.label(
                        RichText::new(header_text)
                            .size(10.0)
                            .strong()
                            .color(Color32::from_rgb(108, 117, 125)),
                    );

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;

                        match mode {
                            AppMode::Phase => {
                                if auto_button(ui, "Auto", 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoPhase;
                                }

                                ui.separator();

                                ui.label(RichText::new("P0").size(12.0));
                                ui.add(DragValue::new(p0).speed(0.1).suffix("°"));
                                let p0_dec = continuous_step_button(ui, "p0_dec", "<", 0.5, 3.0, 30.0, 1.0);
                                if p0_dec > 0.0 {
                                    *p0 = (*p0 - p0_dec).clamp(-180.0, 180.0);
                                }
                                ui.add(Slider::new(p0, -180.0..=180.0).show_value(false));
                                let p0_inc = continuous_step_button(ui, "p0_inc", ">", 0.5, 3.0, 30.0, 1.0);
                                if p0_inc > 0.0 {
                                    *p0 = (*p0 + p0_inc).clamp(-180.0, 180.0);
                                }

                                ui.separator();

                                ui.label(RichText::new("P1").size(12.0));
                                ui.add(DragValue::new(p1).speed(0.5).suffix("°"));
                                let p1_dec = continuous_step_button(ui, "p1_dec", "<", 1.0, 6.0, 60.0, 1.0);
                                if p1_dec > 0.0 {
                                    *p1 = (*p1 - p1_dec).clamp(-360.0, 360.0);
                                }
                                ui.add(Slider::new(p1, -360.0..=360.0).show_value(false));
                                let p1_inc = continuous_step_button(ui, "p1_inc", ">", 1.0, 6.0, 60.0, 1.0);
                                if p1_inc > 0.0 {
                                    *p1 = (*p1 + p1_inc).clamp(-360.0, 360.0);
                                }

                                ui.separator();

                                if light_button(ui, "Reset", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ResetPhase;
                                }
                            }
                            AppMode::Baseline => {
                                ui.label(RichText::new("Method").size(12.0));
                                egui::ComboBox::from_id_salt("baseline_method_selector")
                                    .width(90.0)
                                    .selected_text(if state.baseline_method_kind == 0 {
                                        "airPLS"
                                    } else {
                                        "Polynomial"
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut state.baseline_method_kind, 0, "airPLS");
                                        ui.selectable_value(&mut state.baseline_method_kind, 1, "Polynomial");
                                    });

                                ui.separator();

                                if state.baseline_method_kind == 0 {
                                    ui.label(RichText::new("Stiffness (log10 λ)").size(12.0));
                                    ui.add(
                                        DragValue::new(&mut state.baseline_log_lambda)
                                            .speed(0.1)
                                            .range(3.0..=12.0),
                                    );
                                } else {
                                    ui.label(RichText::new("Order").size(12.0));
                                    ui.add(
                                        DragValue::new(&mut state.baseline_poly_order)
                                            .speed(0.1)
                                            .range(1..=6),
                                    );
                                }

                                let btn_apply_text = "Apply Correction";
                                if light_button(ui, btn_apply_text, false, 110.0).clicked() {
                                    *active_zoom = None;
                                    let method = if state.baseline_method_kind == 0 {
                                        BaselineMethod::AirPLS {
                                            log_lambda: state.baseline_log_lambda,
                                            max_iter: 15,
                                        }
                                    } else {
                                        BaselineMethod::Polynomial {
                                            order: state.baseline_poly_order,
                                            max_iter: 10,
                                        }
                                    };
                                    event = ActionEvent::ApplyBaseline { method };
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ClearBaseline;
                                }

                                let (status_txt, status_color) = match baseline_method {
                                    BaselineMethod::AirPLS { log_lambda, .. } => (
                                        format!("Status Applied (airPLS, logλ={:.1})", log_lambda),
                                        Color32::from_rgb(25, 135, 84), // Bootstrap green
                                    ),
                                    BaselineMethod::Polynomial { order, .. } => (
                                        format!("Status Applied (Poly, order={})", order),
                                        Color32::from_rgb(25, 135, 84),
                                    ),
                                    BaselineMethod::None => (
                                        "Status Not Applied".to_string(),
                                        Color32::from_rgb(108, 117, 125),
                                    ),
                                };
                                ui.label(RichText::new(status_txt).size(11.0).strong().color(status_color));
                            }
                            AppMode::Reference => {
                                let is_13c = nucleus.contains("13C") || nucleus.contains("C13");
                                let mut solvents: Vec<(String, f64)> = Vec::new();
                                for info in KNOWN_SOLVENTS {
                                    let target = if is_13c { info.carbon_ppm } else { info.proton_ppm };
                                    if let Some(ppm) = target {
                                        solvents.push((format!("{} ({:.2} ppm)", info.name, ppm), ppm));
                                    }
                                }

                                ui.label(RichText::new("Solvent").size(12.0));
                                let current_solvent_name = solvents
                                    .get(state.ref_solvent_idx)
                                    .map(|s| s.0.as_str())
                                    .unwrap_or("Custom");
                                egui::ComboBox::from_id_salt("ref_solvent_combo")
                                    .selected_text(current_solvent_name)
                                    .width(140.0)
                                    .show_ui(ui, |ui| {
                                        for (i, (name, val)) in solvents.iter().enumerate() {
                                            if ui.selectable_label(state.ref_solvent_idx == i, name).clicked() {
                                                state.ref_solvent_idx = i;
                                                state.ref_target_ppm = *val;
                                            }
                                        }
                                    });

                                ui.label(RichText::new("Target (ppm)").size(12.0));
                                ui.add(DragValue::new(&mut state.ref_target_ppm).speed(0.01));

                                if auto_button(ui, "Auto", 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoReference;
                                }

                                if light_button(ui, "Set", state.ref_set_active, 46.0).clicked() {
                                    state.ref_set_active = true;
                                    *active_zoom = None;
                                }
                            }
                            AppMode::Peak => {
                                if auto_button(ui, "Auto", 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoPeak;
                                }
                                if light_button(ui, "Pick", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::PickPeaks { threshold: state.peak_threshold };
                                }

                                let is_thresh = state.peak_submode == PeakSubMode::Threshold;
                                if light_button(ui, "Threshold", is_thresh, 72.0).clicked() {
                                    state.peak_submode = PeakSubMode::Threshold;
                                    *active_zoom = None;
                                }
                                ui.add(DragValue::new(&mut state.peak_threshold).speed(0.5).range(0.0..=1e9));

                                ui.label(RichText::new(format!("Noise {:.2}", noise_level)).size(11.0).color(Color32::from_rgb(108, 117, 125)));

                                let is_add = state.peak_submode == PeakSubMode::Add;
                                if light_button(ui, "Add", is_add, 46.0).clicked() {
                                    state.peak_submode = PeakSubMode::Add;
                                    *active_zoom = None;
                                }

                                let is_del = state.peak_submode == PeakSubMode::Delete;
                                if light_button(ui, "Delete", is_del, 52.0).clicked() {
                                    state.peak_submode = PeakSubMode::Delete;
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ClearPeaks;
                                }
                            }
                            AppMode::Integrate => {
                                if auto_button(ui, "Auto", 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoIntegrate;
                                }

                                let is_add = state.integrate_submode == IntegrateSubMode::Add;
                                if light_button(ui, "Add", is_add, 44.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Add;
                                    *active_zoom = None;
                                }

                                let is_edit = state.integrate_submode == IntegrateSubMode::Edit;
                                if light_button(ui, "Edit", is_edit, 44.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Edit;
                                    *active_zoom = None;
                                }

                                let is_split = state.integrate_submode == IntegrateSubMode::Split;
                                if light_button(ui, "Split", is_split, 44.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Split;
                                    *active_zoom = None;
                                }

                                let is_del = state.integrate_submode == IntegrateSubMode::Delete;
                                if light_button(ui, "Delete", is_del, 50.0).clicked() {
                                    if is_del {
                                        state.integrate_submode = IntegrateSubMode::Add;
                                    } else {
                                        state.integrate_submode = IntegrateSubMode::Delete;
                                    }
                                    *active_zoom = None;
                                }

                                ui.label(RichText::new("Scale").size(12.0));
                                ui.add(DragValue::new(integration_scale).speed(0.1).range(1e-12..=1e12));

                                ui.label(RichText::new("Ref Val").size(12.0));
                                ui.add(DragValue::new(&mut state.integration_ref_val).speed(0.1).range(0.01..=1000.0));

                                let is_ref = state.integrate_submode == IntegrateSubMode::Reference;
                                if light_button(ui, "Set", is_ref, 40.0).clicked() {
                                    state.integrate_submode = IntegrateSubMode::Reference;
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ClearIntegrations;
                                }

                            }
                            AppMode::Multiview => {
                                ui.label(RichText::new("Ratio").size(12.0));
                                ui.add(DragValue::new(&mut state.multiview_ratio).speed(0.5).range(0.5..=100.0));

                                ui.checkbox(&mut state.multiview_auto_align, "Auto Align");

                                if auto_button(ui, "Auto", 44.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AutoMultiview;
                                }

                                let is_add_edit = state.multiview_submode != MultiviewSubMode::Delete;
                                if light_button(ui, "Add & Edit", is_add_edit, 72.0).clicked() {
                                    state.multiview_submode = MultiviewSubMode::None;
                                    *active_zoom = None;
                                }

                                let is_del = state.multiview_submode == MultiviewSubMode::Delete;
                                if light_button(ui, "Delete", is_del, 50.0).clicked() {
                                    if is_del {
                                        state.multiview_submode = MultiviewSubMode::None;
                                    } else {
                                        state.multiview_submode = MultiviewSubMode::Delete;
                                    }
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Adjust-Y", false, 64.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AdjustYMultiview;
                                }

                                if light_button(ui, "Align", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AlignMultiview;
                                }

                                if light_button(ui, "1-Row", false, 48.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::AlignMultiviewsOneRow;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ResetMultiview;
                                }
                            }
                            AppMode::JCoupling => {
                                ui.checkbox(&mut state.non_integer_protons, "Allow Non-Integer");

                                let is_add = state.jcoupling_add_active;
                                if light_button(ui, "Add", is_add, 46.0).clicked() {
                                    state.jcoupling_add_active = true;
                                    *active_zoom = None;
                                }

                                if light_button(ui, "Clear", false, 46.0).clicked() {
                                    *active_zoom = None;
                                    event = ActionEvent::ClearJCoupling;
                                }
                            }
                        }

                        // 各モードの一番右端に Close ボタンを配置 (押すとモードを閉じる)
                        ui.add_space(6.0);
                        if close_button(ui).clicked() {
                            event = ActionEvent::CloseMode;
                        }
                    });
                });
            });
        }
        });
    });

    event
}
