use egui::{Pos2, Rect};

/// プロット領域の座標変換マネージャ (下部80pxのピークラベル領域を考慮)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotTransform {
    /// 画面上の描画矩形 (ピクセル座標)
    pub screen_rect: Rect,
    /// 下部の余白 (ピークラベル・引き出し線用領域, 通常 80.0px)
    pub bottom_margin: f32,
    /// 表示中の PPM 最小値 (右端)
    pub ppm_min: f64,
    /// 表示中の PPM 最大値 (左端)
    pub ppm_max: f64,
    /// 表示中の Y 最小値 (X軸上)
    pub y_min: f64,
    /// 表示中の Y 最大値 (画面上端)
    pub y_max: f64,
}

impl PlotTransform {
    pub fn new(screen_rect: Rect, ppm_min: f64, ppm_max: f64, y_min: f64, y_max: f64) -> Self {
        Self {
            screen_rect,
            bottom_margin: 80.0,
            ppm_min,
            ppm_max,
            y_min,
            y_max,
        }
    }

    /// メインプロットの底（X軸の位置）の画面Y座標
    pub fn axis_y(&self) -> f32 {
        self.screen_rect.max.y - self.bottom_margin
    }

    /// メインプロットの有効高さ
    pub fn plot_height(&self) -> f32 {
        (self.axis_y() - self.screen_rect.min.y).max(10.0)
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
    /// ※ 画面上端が y_max、X軸（axis_y）が y_min
    pub fn y_to_screen_y(&self, y: f64) -> f32 {
        let span = self.y_max - self.y_min;
        if span.abs() < 1e-12 {
            return (self.screen_rect.min.y + self.axis_y()) * 0.5;
        }
        let norm = (y - self.y_min) / span;
        self.axis_y() - (norm as f32) * self.plot_height()
    }

    /// 画面 Y 座標 (ピクセル) -> Y (データ強度)
    pub fn screen_y_to_y(&self, screen_y: f32) -> f64 {
        let height = self.plot_height();
        if height <= 0.0 {
            return (self.y_min + self.y_max) * 0.5;
        }
        let norm = ((self.axis_y() - screen_y) / height) as f64;
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
