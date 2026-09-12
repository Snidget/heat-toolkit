use std::collections::HashMap;

use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Vector};

use crate::models::Segment;
use crate::parser::ScriptLine;

use super::theme;

/// 3D-превью без GL: изометрическая проекция параллелепипедов, painter's algorithm.
/// Полный аналог egui Preview3D (three-d), но считает грани на CPU и рисует их в canvas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StandardView {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

pub fn standard_view_angles(view: StandardView) -> (f64, f64) {
    match view {
        StandardView::Front => (0.0, 0.0),
        StandardView::Back => (std::f64::consts::PI, 0.0),
        StandardView::Right => (-std::f64::consts::FRAC_PI_2, 0.0),
        StandardView::Left => (std::f64::consts::FRAC_PI_2, 0.0),
        StandardView::Top => (0.0, std::f64::consts::FRAC_PI_2),
        StandardView::Bottom => (0.0, -std::f64::consts::FRAC_PI_2),
    }
}

pub const STANDARD_VIEWS: [[(StandardView, &str); 3]; 2] = [
    [
        (StandardView::Front, "Спереди"),
        (StandardView::Back, "Сзади"),
        (StandardView::Top, "Сверху"),
    ],
    [
        (StandardView::Left, "Слева"),
        (StandardView::Right, "Справа"),
        (StandardView::Bottom, "Снизу"),
    ],
];

#[derive(Debug, Clone, Copy)]
pub enum Canvas3DMessage {
    Rotated(f64, f64),
}

/// Направление условного источника света (совпадает с egui renderer3d).
fn light_dir() -> (f64, f64, f64) {
    let len = (0.4f64 * 0.4 + 0.9 * 0.9 + 0.5 * 0.5).sqrt();
    (0.4 / len, 0.9 / len, 0.5 / len)
}

fn label_char(line: &ScriptLine) -> char {
    line.label
        .as_deref()
        .unwrap_or("")
        .chars()
        .next()
        .unwrap_or('p')
}

fn shape_color_solid(label: char) -> Color {
    match label {
        'p' => Color::from_rgb(1.0, 180.0 / 255.0, 180.0 / 255.0),
        'b' => Color::from_rgb(180.0 / 255.0, 1.0, 180.0 / 255.0),
        'e' => Color::from_rgb(180.0 / 255.0, 200.0 / 255.0, 1.0),
        _ => Color::from_rgb(220.0 / 255.0, 220.0 / 255.0, 220.0 / 255.0),
    }
}

fn material_name_from_trailing(trailing: &str) -> String {
    crate::material_sort::material_name_from_trailing(trailing)
}

#[derive(Default)]
pub struct Preview3DState {
    pub drag_origin: Option<Point>,
    pub shift: bool,
    pub ctrl: bool,
}

/// Программа canvas: рисует сцену каждый кадр (без Cache — пересчёт дёшев,
/// а углы обзора меняются извне через сообщения приложения).
pub struct Preview3D<'a> {
    pub lines: &'a [ScriptLine],
    pub material_colors: &'a HashMap<String, Color>,
    pub azimuth: f64,
    pub elevation: f64,
}

impl<'a> Preview3D<'a> {
    /// Builds a canvas whose lifetime follows the borrowed page data.
    pub fn view(
        lines: &'a [ScriptLine],
        material_colors: &'a HashMap<String, Color>,
        azimuth: f64,
        elevation: f64,
    ) -> iced::Element<'a, Canvas3DMessage> {
        canvas::Canvas::new(Preview3D {
            lines,
            material_colors,
            azimuth,
            elevation,
        })
        .width(iced::Length::Fill)
        .height(300)
        .into()
    }
}

/// Полигон одной грани с глубиной для painter's algorithm.
struct Face {
    depth: f64,
    points: [Point; 4],
    color: Color,
}

/// Камера look-at с изометрической проекцией (аналог V2/egui preview_3d).
struct Camera {
    position: [f64; 3],
    forward: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
    half_w: f64,
    half_h: f64,
    w: f64,
    h: f64,
    near: f64,
    far: f64,
}

impl Camera {
    /// Проекция мировой точки в экранные координаты.
    fn project(&self, v: [f64; 3]) -> Option<[f64; 2]> {
        let dx = v[0] - self.position[0];
        let dy = v[1] - self.position[1];
        let dz = v[2] - self.position[2];
        let vz = dx * self.forward[0] + dy * self.forward[1] + dz * self.forward[2];
        if vz < self.near || vz > self.far {
            return None;
        }
        let vx = dx * self.right[0] + dy * self.right[1] + dz * self.right[2];
        let vy = dx * self.up[0] + dy * self.up[1] + dz * self.up[2];
        let sx = (vx / self.half_w * 0.5 + 0.5) * self.w;
        let sy = (0.5 - vy / self.half_h * 0.5) * self.h;
        Some([sx, sy])
    }
}

/// Центр и радиус сцены по всем сегментам.
fn scene_bounds(lines: &[ScriptLine]) -> Option<([f64; 3], f64)> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for line in lines {
        if let Some(segment) = line.segment {
            any = true;
            let [x1, y1, z1, x2, y2, z2] = segment.as_tuple();
            for (i, v) in [x1, y1, z1, x2, y2, z2].into_iter().enumerate() {
                min[i % 3] = min[i % 3].min(v);
                max[i % 3] = max[i % 3].max(v);
            }
        }
    }
    if !any {
        return None;
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let mut radius = 0.0f64;
    for i in 0..3 {
        radius = radius.max(max[i] - min[i]);
    }
    Some((center, (radius * 0.5).max(1e-6)))
}

fn build_camera(
    lines: &[ScriptLine],
    azimuth: f64,
    elevation: f64,
    bounds: Rectangle,
) -> Option<(Camera, [f64; 3], f64)> {
    let (center, radius) = scene_bounds(lines)?;
    let el = elevation.clamp(-1.45, 1.45);
    let dir = (el.cos() * azimuth.sin(), el.sin(), el.cos() * azimuth.cos());

    let dist = radius * 6.0 + 1.0;
    let position = [
        center[0] + dir.0 * dist,
        center[1] + dir.1 * dist,
        center[2] + dir.2 * dist,
    ];

    // Look-at базис: forward от камеры к центру, up = Y.
    let (fx, fy, fz) = (-dir.0, -dir.1, -dir.2);
    let (rx, ry, rz) = cross((fx, fy, fz), (0.0, 1.0, 0.0));
    let (rx, ry, rz) = if norm((rx, ry, rz)) < 1e-12 {
        (1.0, 0.0, 0.0)
    } else {
        normalize((rx, ry, rz))
    };
    let (ux, uy, uz) = cross((rx, ry, rz), (fx, fy, fz));

    let w = bounds.width.max(1.0) as f64;
    let h = bounds.height.max(1.0) as f64;
    let aspect = w / h;
    let half_h = (radius / aspect).max(radius) * 1.15;
    let half_w = half_h * aspect;
    let near = (dist - radius * 3.0).max(0.1);
    let far = dist + radius * 3.0;

    Some((
        Camera {
            position,
            forward: [fx, fy, fz],
            right: [rx, ry, rz],
            up: [ux, uy, uz],
            half_w,
            half_h,
            w,
            h,
            near,
            far,
        },
        center,
        radius,
    ))
}

fn project_scene(
    lines: &[ScriptLine],
    material_colors: &HashMap<String, Color>,
    azimuth: f64,
    elevation: f64,
    bounds: Rectangle,
) -> Vec<Face> {
    let light = light_dir();
    let Some((camera, _, _)) = build_camera(lines, azimuth, elevation, bounds) else {
        return Vec::new();
    };

    let segments: Vec<(char, Segment, Color)> = lines
        .iter()
        .filter_map(|line| line.segment.map(|segment| (label_char(line), segment)))
        .map(|(label, segment)| {
            let default_fill = shape_color_solid(label);
            let fill = if material_colors.is_empty() {
                default_fill
            } else {
                let key = material_name_from_trailing(
                    lines
                        .iter()
                        .find(|line| line.segment == Some(segment))
                        .map(|line| line.trailing.as_str())
                        .unwrap_or(""),
                );
                if key.is_empty() {
                    default_fill
                } else {
                    material_colors.get(&key).copied().unwrap_or(default_fill)
                }
            };
            (label, segment, fill)
        })
        .collect();
    if segments.is_empty() {
        return Vec::new();
    }

    let mut faces = Vec::new();
    for (_, segment, fill) in &segments {
        for face in box_faces(*segment, *fill, &light) {
            let [v0, v1, v2, v3] = face.vertices;
            let mut out = [[0.0f64; 2]; 4];
            let mut visible = true;
            for (i, v) in [v0, v1, v2, v3].into_iter().enumerate() {
                if let Some(point) = camera.project(v) {
                    out[i] = point;
                } else {
                    visible = false;
                    break;
                }
            }
            if !visible {
                continue;
            }
            let [cx, cy, cz] = face.center;
            let depth = (cx - camera.position[0]) * camera.forward[0]
                + (cy - camera.position[1]) * camera.forward[1]
                + (cz - camera.position[2]) * camera.forward[2];
            faces.push(Face {
                depth,
                points: [
                    Point::new(out[0][0] as f32, out[0][1] as f32),
                    Point::new(out[1][0] as f32, out[1][1] as f32),
                    Point::new(out[2][0] as f32, out[2][1] as f32),
                    Point::new(out[3][0] as f32, out[3][1] as f32),
                ],
                color: face.color,
            });
        }
    }
    faces.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    faces
}

struct BoxFace {
    vertices: [[f64; 3]; 4],
    center: [f64; 3],
    color: Color,
}

/// 6 граней параллелепипеда с диффузным затенением (как egui renderer3d).
fn box_faces(segment: Segment, color: Color, light: &(f64, f64, f64)) -> Vec<BoxFace> {
    let [x1, y1, z1, x2, y2, z2] = segment.as_tuple();
    let c = [
        [x1, y1, z1],
        [x1, y1, z2],
        [x1, y2, z1],
        [x1, y2, z2],
        [x2, y1, z1],
        [x2, y1, z2],
        [x2, y2, z1],
        [x2, y2, z2],
    ];
    let center = [(x1 + x2) * 0.5, (y1 + y2) * 0.5, (z1 + z2) * 0.5];
    let faces: [(usize, usize, usize, usize); 6] = [
        (0, 1, 3, 2),
        (4, 5, 7, 6),
        (0, 1, 5, 4),
        (2, 3, 7, 6),
        (0, 2, 6, 4),
        (1, 3, 7, 5),
    ];

    let mut out = Vec::with_capacity(faces.len());
    for (a, b, n, d) in faces {
        let v0 = c[a];
        let v1 = c[b];
        let v2 = c[n];
        let v3 = c[d];
        let face_center = [
            (v0[0] + v1[0] + v2[0] + v3[0]) * 0.25,
            (v0[1] + v1[1] + v2[1] + v3[1]) * 0.25,
            (v0[2] + v1[2] + v2[2] + v3[2]) * 0.25,
        ];
        let to_face = sub(face_center, center);
        let normal = if norm(to_face) < 1e-9 {
            (0.0, 0.0, 0.0)
        } else {
            normalize(to_face)
        };
        let diffuse = (dot(normal, *light)).max(0.0);
        let shade = (0.45 + 0.6 * diffuse).min(1.0) as f32;
        out.push(BoxFace {
            vertices: [v0, v1, v2, v3],
            center: face_center,
            color: Color::from_rgb(color.r * shade, color.g * shade, color.b * shade),
        });
    }
    out
}

fn cross(a: (f64, f64, f64), b: (f64, f64, f64)) -> (f64, f64, f64) {
    (
        a.1 * b.2 - a.2 * b.1,
        a.2 * b.0 - a.0 * b.2,
        a.0 * b.1 - a.1 * b.0,
    )
}

fn dot(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn inside(bounds: Rectangle, point: Point) -> bool {
    point.x >= bounds.x
        && point.x <= bounds.x + bounds.width
        && point.y >= bounds.y
        && point.y <= bounds.y + bounds.height
}

fn norm(a: (f64, f64, f64)) -> f64 {
    (a.0 * a.0 + a.1 * a.1 + a.2 * a.2).sqrt()
}

fn normalize(a: (f64, f64, f64)) -> (f64, f64, f64) {
    let n = norm(a);
    if n < 1e-12 {
        (0.0, 0.0, 0.0)
    } else {
        (a.0 / n, a.1 / n, a.2 / n)
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> (f64, f64, f64) {
    (a[0] - b[0], a[1] - b[1], a[2] - b[2])
}

fn rotated_view(
    azimuth: f64,
    elevation: f64,
    delta: Vector,
    ctrl: bool,
    shift: bool,
) -> (f64, f64) {
    let azimuth = if ctrl {
        azimuth
    } else {
        azimuth + delta.x as f64 * 0.02
    };
    let elevation = if shift {
        elevation
    } else {
        (elevation - delta.y as f64 * 0.02).clamp(-1.55, 1.55)
    };
    (azimuth, elevation)
}

impl<'a> canvas::Program<Canvas3DMessage> for Preview3D<'a> {
    type State = Preview3DState;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let dark = theme::is_dark(theme);
        let has_geometry = self.lines.iter().any(|line| line.segment.is_some());

        // Empty state — dot-grid paper, [NO DATA] mono caps centered.
        if !has_geometry {
            super::canvas::draw_dot_grid(&mut frame, bounds, dark);
            let center = Point::new(bounds.width * 0.5, bounds.height * 0.5);
            frame.fill_text(canvas::Text {
                content: "[ NO DATA ]".to_owned(),
                position: center,
                color: theme::muted(dark),
                size: iced::Pixels(12.0),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
            return vec![frame.into_geometry()];
        }

        frame.fill_rectangle(Point::ORIGIN, bounds.size(), theme::paper(dark));

        if let Some((camera, _center, radius)) =
            build_camera(self.lines, self.azimuth, self.elevation, bounds)
        {
            if let Some(origin) = camera.project([0.0, 0.0, 0.0]) {
                let axis_len = radius * 2.0 * 0.1;
                let axes = [
                    (Color::from_rgb(1.0, 0.0, 0.0), [axis_len, 0.0, 0.0], "X"),
                    (Color::from_rgb(0.0, 1.0, 0.0), [0.0, axis_len, 0.0], "Y"),
                    (Color::from_rgb(0.0, 0.0, 1.0), [0.0, 0.0, axis_len], "Z"),
                ];
                let start = Point::new(origin[0] as f32, origin[1] as f32);
                for (color, end, label) in axes {
                    if let Some(end) = camera.project(end) {
                        let end = Point::new(end[0] as f32, end[1] as f32);
                        if inside(bounds, start) && inside(bounds, end) {
                            frame.stroke(
                                &canvas::Path::line(start, end),
                                Stroke::default().with_color(color).with_width(2.0),
                            );
                            frame.fill_text(canvas::Text {
                                content: label.to_owned(),
                                position: end,
                                color,
                                size: iced::Pixels(12.0),
                                align_x: iced::alignment::Horizontal::Left.into(),
                                align_y: iced::alignment::Vertical::Bottom,
                                ..canvas::Text::default()
                            });
                        }
                    }
                }
            }
        }

        let faces = project_scene(
            self.lines,
            self.material_colors,
            self.azimuth,
            self.elevation,
            bounds,
        );
        for face in faces {
            let path = Path::new(|builder| {
                builder.move_to(face.points[0]);
                builder.line_to(face.points[1]);
                builder.line_to(face.points[2]);
                builder.line_to(face.points[3]);
                builder.close();
            });
            frame.fill(&path, face.color);
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(theme::ink(dark))
                    .with_width(1.0),
            );
        }
        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<Canvas3DMessage>> {
        match event {
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift = modifiers.shift();
                state.ctrl = modifiers.control();
                None
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                state.drag_origin = cursor.position_in(bounds);
                None
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                state.drag_origin = None;
                None
            }
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => {
                let origin = state.drag_origin?;
                let position = cursor.position_in(bounds)?;
                let delta = position - origin;
                if delta == Vector::ZERO {
                    return None;
                }
                let (azimuth, elevation) =
                    rotated_view(self.azimuth, self.elevation, delta, state.ctrl, state.shift);
                state.drag_origin = Some(position);
                Some(canvas::Action::publish(Canvas3DMessage::Rotated(
                    azimuth, elevation,
                )))
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        if state.drag_origin.is_some() || cursor.position_in(_bounds).is_some() {
            iced::mouse::Interaction::Grabbing
        } else {
            iced::mouse::Interaction::default()
        }
    }
}
