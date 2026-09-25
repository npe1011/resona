use egui::{Pos2, Rect, Vec2};

/// プロット領域の座標変換マネージャ
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotTransform {
    /// 画面上の描画矩形 (ピクセル座標)
    pub screen_rect: Rect,
    /// 表示中の PPM 最小値 (右端)
    pub ppm_min: f64,
    /// 表示中の PPM 最大値 (左端)
    pub ppm_max: f64,
    /// 表示中の Y 最小値 (下端)
    pub y_min: f64,
    /// 表示中の Y 最大値 (上端)
    pub y_max: f64,
}

impl PlotTransform {
    pub fn new(screen_rect: Rect, ppm_min: f64, ppm_max: f64, y_min: f64, y_max: f64) -> Self {
        Self {
            screen_rect,
            ppm_min,
            ppm_max,
            y_min,
            y_max,
        }
    }

    /// PPM (データ座標) -> 画面 X 座標 (ピクセル)
    /// ※ NMR の慣例: ppm_max が左、ppm_min が右
    pub fn ppm_to_screen_x(&self, ppm: f64) -> f32 {
        let span = self.ppm_max - self.ppm_min;
        if span.abs() < 1e-12 {
            return self.screen_rect.center().x;
        }
        let norm = (self.ppm_max - ppm) / span;
        self.screen_rect.min.x + (norm as f32) * self.screen_rect.width()
    }

    /// 画面 X 座標 (ピクセル) -> PPM (データ座標)
    pub fn screen_x_to_ppm(&self, screen_x: f32) -> f64 {
        let width = self.screen_rect.width();
        if width <= 0.0 {
            return (self.ppm_min + self.ppm_max) * 0.5;
        }
        let norm = ((screen_x - self.screen_rect.min.x) / width) as f64;
        self.ppm_max - norm * (self.ppm_max - self.ppm_min)
    }

    /// Y (データ強度) -> 画面 Y 座標 (ピクセル)
    /// ※ 画面上端が y_max、下端が y_min
    pub fn y_to_screen_y(&self, y: f64) -> f32 {
        let span = self.y_max - self.y_min;
        if span.abs() < 1e-12 {
            return self.screen_rect.center().y;
        }
        let norm = (y - self.y_min) / span;
        self.screen_rect.max.y - (norm as f32) * self.screen_rect.height()
    }

    /// 画面 Y 座標 (ピクセル) -> Y (データ強度)
    pub fn screen_y_to_y(&self, screen_y: f32) -> f64 {
        let height = self.screen_rect.height();
        if height <= 0.0 {
            return (self.y_min + self.y_max) * 0.5;
        }
        let norm = ((self.screen_rect.max.y - screen_y) / height) as f64;
        self.y_min + norm * (self.y_max - self.y_min)
    }

    /// (PPM, Y) -> 画面座標 Pos2
    pub fn data_to_screen(&self, ppm: f64, y: f64) -> Pos2 {
        Pos2::new(self.ppm_to_screen_x(ppm), self.y_to_screen_y(y))
    }

    /// 画面座標 Pos2 -> (PPM, Y)
    pub fn screen_to_data(&self, pos: Pos2) -> (f64, f64) {
        (self.screen_x_to_ppm(pos.x), self.screen_y_to_y(pos.y))
    }

    /// マウスドラッグによる平行移動 (パン)
    pub fn pan(&mut self, delta: Vec2) {
        let span_ppm = self.ppm_max - self.ppm_min;
        let delta_ppm = (delta.x / self.screen_rect.width()) as f64 * span_ppm;
        // 反転軸なので、右へドラッグすると PPM は増加 (左へシフト)
        self.ppm_min += delta_ppm;
        self.ppm_max += delta_ppm;

        let span_y = self.y_max - self.y_min;
        let delta_y = (delta.y / self.screen_rect.height()) as f64 * span_y;
        // 画面下へドラッグすると Y は増加
        self.y_min += delta_y;
        self.y_max += delta_y;
    }

    /// ピボット位置 (画面座標) を中心としたズーム
    pub fn zoom(&mut self, pivot_screen: Pos2, factor_x: f64, factor_y: f64) {
        let (pivot_ppm, pivot_y) = self.screen_to_data(pivot_screen);

        if factor_x > 0.0 {
            self.ppm_min = pivot_ppm - (pivot_ppm - self.ppm_min) / factor_x;
            self.ppm_max = pivot_ppm + (self.ppm_max - pivot_ppm) / factor_x;
        }

        if factor_y > 0.0 {
            self.y_min = pivot_y - (pivot_y - self.y_min) / factor_y;
            self.y_max = pivot_y + (self.y_max - pivot_y) / factor_y;
        }
    }
}
