//! Конфигурация приложения: размеры, цвета, подписи.

pub const APP_NAME: &str = "HEAT3 Поворотник";
pub const APP_VERSION: &str = "2.0.0";

pub const TURNER_PAGE_WIDTH: i32 = 320;
pub const NAVIGATION_WIDTH: i32 = 140;
pub const WINDOW_WIDTH: i32 = TURNER_PAGE_WIDTH + NAVIGATION_WIDTH;
pub const WINDOW_HEIGHT: i32 = 600;

pub const PREVIEW_MIN_WIDTH: f32 = 300.0;
pub const PREVIEW_MIN_HEIGHT: f32 = 200.0;
pub const PREVIEW_MAX_HEIGHT: f32 = 200.0;
pub const PREVIEW_PADDING: f32 = 20.0;
pub const PREVIEW_AXIS_RATIO: f32 = 0.1;

pub const LINE_BREAK: &str = "\r\n";

pub const VALID_LABELS: &[&str] = &["p", "b", "e"];

/// Цвета фигур на превью (RGBA).
#[derive(Clone, Copy, Debug)]
pub struct ShapeColors {
    pub p: [u8; 4],
    pub b: [u8; 4],
    pub e: [u8; 4],
}

pub const SHAPE_COLORS: ShapeColors = ShapeColors {
    p: [255, 200, 200, 180],
    b: [200, 255, 200, 180],
    e: [200, 200, 255, 180],
};
