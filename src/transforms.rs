//! Геометрические преобразования координатных отрезков.
//!
//! Логика повторяет формулы из оригинального Povorotnik.py (v1.4).
//! Некоторые операции переставляют значения между концами отрезка —
//! это поведение оригинала, необходимое для ориентации нормалей в HEAT3.

use std::sync::OnceLock;

use crate::models::Segment;

fn enable_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(%enable=)([01]{6})([^01]|$)").unwrap())
}

pub fn rotate_xy_90_cw(s: &Segment) -> Segment {
    let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
    Segment::new(y1, -x1, z1, y2, -x2, z2)
}

pub fn rotate_xy_90_ccw(s: &Segment) -> Segment {
    let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
    Segment::new(-y2, x2, z1, -y1, x1, z2)
}

pub fn swap_xy_xz(s: &Segment) -> Segment {
    let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
    Segment::new(x1, z1, y1, x2, z2, y2)
}

pub fn mirror_xy_x(s: &Segment) -> Segment {
    let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
    Segment::new(x1, -y2, z1, x2, -y1, z2)
}

/// Зеркало по Y в плоскости XY: (x1,x2) -> (-x2,-x1), y и z не меняются.
///
/// ВАЖНО: для axis-aligned боксов результат математически эквивалентен
/// `mirror_xz_z` (там та же формула `(-x2, y1, z1, -x1, y2, z2)`), потому что
/// инверсия оси X одинакова в обеих плоскостях. Отдельная кнопка сохранена
/// для UX-совместимости с оригинальным Povorotnik.py (v1.4).
pub fn mirror_xy_y(s: &Segment) -> Segment {
    let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
    Segment::new(-x2, y1, z1, -x1, y2, z2)
}

pub fn mirror_xz_x(s: &Segment) -> Segment {
    let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
    Segment::new(x1, y1, -z2, x2, y2, -z1)
}

/// Зеркало по Z в плоскости XZ.
///
/// ВАЖНО: для axis-aligned боксов формула идентична `mirror_xy_y`
/// (`(-x2, y1, z1, -x1, y2, z2)` — инверсия оси X). Это намеренное повторение
/// оригинала Povorotnik.py: обе операции выглядят одинаково для
/// прямоугольных боксов, но сохранены как отдельные кнопки UI.
pub fn mirror_xz_z(s: &Segment) -> Segment {
    let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
    Segment::new(-x2, y1, z1, -x1, y2, z2)
}

fn remap_enable(mask: &str, mapping: [usize; 6]) -> String {
    let chars: Vec<char> = mask.chars().collect();
    mapping.iter().map(|&i| chars[i]).collect()
}

fn enable_transform(mapping: [usize; 6]) -> impl Fn(&str) -> String {
    move |mask: &str| remap_enable(mask, mapping)
}

pub fn transform_enable_flags(text: &str, transform_name: &str) -> String {
    let mut found: Option<EnableFn> = None;
    for (n, f) in ENABLE_TRANSFORMS {
        if *n == transform_name {
            found = Some(*f);
            break;
        }
    }
    let transform = match found {
        Some(f) => f,
        None => return text.to_string(),
    };
    enable_re()
        .replace_all(text, |caps: &regex::Captures| {
            format!("{}{}{}", &caps[1], transform(&caps[2]), &caps[3])
        })
        .to_string()
}

pub fn apply_transform(name: &str, segment: &Segment) -> Option<Segment> {
    for (n, f) in TRANSFORMS {
        if *n == name {
            return Some(f(segment));
        }
    }
    None
}

type TransformFn = fn(&Segment) -> Segment;
type EnableFn = fn(&str) -> String;

pub const TRANSFORMS: &[(&str, TransformFn)] = &[
    ("rotate_clockwise", rotate_xy_90_cw),
    ("rotate_counterclockwise", rotate_xy_90_ccw),
    ("swap_xy_xz", swap_xy_xz),
    ("mirror_xy_x", mirror_xy_x),
    ("mirror_xy_y", mirror_xy_y),
    ("mirror_xz_x", mirror_xz_x),
    ("mirror_xz_z", mirror_xz_z),
];

pub const ENABLE_TRANSFORMS: &[(&str, EnableFn)] = &[
    ("rotate_clockwise", enable_transform_x),
    ("rotate_counterclockwise", enable_transform_ccw),
    ("swap_xy_xz", enable_transform_swap),
    ("mirror_xy_x", enable_transform_mxy_x),
    ("mirror_xy_y", enable_transform_mxy_y),
    ("mirror_xz_x", enable_transform_mxz_x),
    ("mirror_xz_z", enable_transform_mxz_z),
];

fn enable_transform_x(mask: &str) -> String {
    enable_transform([2, 3, 0, 1, 4, 5])(mask)
}
fn enable_transform_ccw(mask: &str) -> String {
    enable_transform([3, 2, 1, 0, 4, 5])(mask)
}
fn enable_transform_swap(mask: &str) -> String {
    enable_transform([0, 1, 4, 5, 2, 3])(mask)
}
fn enable_transform_mxy_x(mask: &str) -> String {
    enable_transform([0, 1, 3, 2, 4, 5])(mask)
}
fn enable_transform_mxy_y(mask: &str) -> String {
    enable_transform([1, 0, 2, 3, 4, 5])(mask)
}
fn enable_transform_mxz_x(mask: &str) -> String {
    enable_transform([0, 1, 2, 3, 5, 4])(mask)
}
fn enable_transform_mxz_z(mask: &str) -> String {
    enable_transform([1, 0, 2, 3, 4, 5])(mask)
}
