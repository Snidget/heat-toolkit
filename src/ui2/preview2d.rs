use iced::widget::canvas::{self, Cache, Path, Program, Stroke};
use iced::{Color, Point, Rectangle, Size};

use crate::parser::ScriptLine;
use crate::turner2d::Rect2D;

use super::canvas::{CanvasMessage, CanvasState};
use super::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Projection {
    #[default]
    XY,
    XZ,
}

impl std::fmt::Display for Projection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::XY => f.write_str("XY"),
            Self::XZ => f.write_str("XZ"),
        }
    }
}

#[derive(Clone, Copy)]
struct Box2D {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    label: char,
}

fn label_char(line: &ScriptLine) -> char {
    line.label
        .as_deref()
        .unwrap_or("")
        .chars()
        .next()
        .unwrap_or('p')
}

fn shape_color(label: char) -> Color {
    match label {
        'p' => Color::from_rgba(1.0, 200.0 / 255.0, 200.0 / 255.0, 0.7),
        'b' => Color::from_rgba(200.0 / 255.0, 1.0, 200.0 / 255.0, 0.7),
        'e' => Color::from_rgba(200.0 / 255.0, 200.0 / 255.0, 1.0, 0.7),
        _ => Color::from_rgba(220.0 / 255.0, 220.0 / 255.0, 220.0 / 255.0, 0.7),
    }
}

/// Maps world coordinates to screen space with a 10% margin and centered content.
#[derive(Clone, Copy)]
struct ScreenTransform {
    min_x: f64,
    min_y: f64,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    width: f64,
    height: f64,
}

impl ScreenTransform {
    fn position(&self, x: f64, y: f64) -> Point {
        Point::new(
            (self.offset_x + (x - self.min_x) * self.scale) as f32,
            (self.height - (self.offset_y + (y - self.min_y) * self.scale)) as f32,
        )
    }
}

const PADDING: f64 = 20.0;

fn layout(bounds: Rectangle, boxes: &[Box2D]) -> Option<ScreenTransform> {
    if boxes.is_empty() {
        return None;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for b in boxes {
        min_x = min_x.min(b.min_x);
        min_y = min_y.min(b.min_y);
        max_x = max_x.max(b.max_x);
        max_y = max_y.max(b.max_y);
    }
    if (max_x - min_x).abs() < 1e-12 {
        max_x += 1.0;
    }
    if (max_y - min_y).abs() < 1e-12 {
        max_y += 1.0;
    }
    let pad = (max_x - min_x).max(max_y - min_y) * 0.1;
    min_x -= pad;
    max_x += pad;
    min_y -= pad;
    max_y += pad;

    let w = bounds.width as f64 - 2.0 * PADDING;
    let h = bounds.height as f64 - 2.0 * PADDING;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = (w / (max_x - min_x)).min(h / (max_y - min_y));
    Some(ScreenTransform {
        min_x,
        min_y,
        scale,
        offset_x: (bounds.width as f64 - (max_x - min_x) * scale) / 2.0,
        offset_y: (bounds.height as f64 - (max_y - min_y) * scale) / 2.0,
        width: bounds.width as f64,
        height: bounds.height as f64,
    })
}

pub struct Preview2D {
    pub projection: Projection,
    pub lines: Vec<ScriptLine>,
    pub rects: Vec<Rect2D>,
    pub show_grid: bool,
    pub cache: Cache,
    #[cfg(test)]
    cache_invalidation_epoch: std::cell::Cell<u64>,
}

impl Default for Preview2D {
    fn default() -> Self {
        Self {
            projection: Projection::XY,
            lines: Vec::new(),
            rects: Vec::new(),
            show_grid: true,
            cache: Cache::new(),
            #[cfg(test)]
            cache_invalidation_epoch: std::cell::Cell::new(0),
        }
    }
}

impl Preview2D {
    pub fn invalidate_cache(&self) {
        self.cache.clear();
        #[cfg(test)]
        self.cache_invalidation_epoch
            .set(self.cache_invalidation_epoch.get().wrapping_add(1));
    }

    pub fn set_segments(&mut self, lines: &[ScriptLine]) {
        self.lines = lines.to_vec();
        self.rects.clear();
        self.invalidate_cache();
    }

    pub fn set_rects(&mut self, rects: &[Rect2D]) {
        self.rects = rects.to_vec();
        self.lines.clear();
        self.invalidate_cache();
    }

    pub fn set_projection(&mut self, projection: Projection) {
        if self.projection != projection {
            self.projection = projection;
            self.invalidate_cache();
        }
    }

    pub fn segment_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.segment.is_some())
            .count()
    }

    pub fn view(&self) -> iced::Element<'_, CanvasMessage> {
        canvas::Canvas::new(self)
            .width(iced::Length::Fixed(theme::TURNER_PREVIEW_SIZE))
            .height(iced::Length::Fixed(theme::TURNER_PREVIEW_SIZE))
            .into()
    }

    fn boxes(&self) -> Vec<Box2D> {
        if !self.rects.is_empty() {
            return self
                .rects
                .iter()
                .map(|r| Box2D {
                    min_x: r.x1.min(r.x2),
                    min_y: r.y1.min(r.y2),
                    max_x: r.x1.max(r.x2),
                    max_y: r.y1.max(r.y2),
                    label: 'p',
                })
                .collect();
        }
        self.lines
            .iter()
            .filter_map(|line| {
                let s = line.segment?;
                let [x1, y1, z1, x2, y2, z2] = s.as_tuple();
                Some(Box2D {
                    min_x: x1.min(x2),
                    min_y: if self.projection == Projection::XY {
                        y1.min(y2)
                    } else {
                        z1.min(z2)
                    },
                    max_x: x1.max(x2),
                    max_y: if self.projection == Projection::XY {
                        y1.max(y2)
                    } else {
                        z1.max(z2)
                    },
                    label: label_char(line),
                })
            })
            .collect()
    }

    fn hit_test(&self, point: Point, bounds: Rectangle) -> Option<usize> {
        let boxes = self.boxes();
        let transform = layout(bounds, &boxes)?;
        boxes.iter().position(|b| {
            let p1 = transform.position(b.min_x, b.min_y);
            let p2 = transform.position(b.max_x, b.max_y);
            let x = p1.x.min(p2.x);
            let y = p1.y.min(p2.y);
            let w = (p2.x - p1.x).abs().max(1.0);
            let h = (p2.y - p1.y).abs().max(1.0);
            point.x >= x && point.x <= x + w && point.y >= y && point.y <= y + h
        })
    }
}

impl Program<CanvasMessage> for Preview2D {
    type State = CanvasState;
    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let dark = theme::is_dark(theme);
        // Single cache owner: draw through the page-owned cache that external
        // handlers (`set_segments`/`set_rects`/`set_projection`/theme/grid) clear.
        // Using `State::cache` here would leave those invalidations dormant.
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let boxes = self.boxes();
            if boxes.is_empty() {
                // Empty state — dot-grid paper with [ NO DATA ] center label.
                if self.show_grid {
                    super::canvas::draw_dot_grid(frame, bounds, dark);
                }
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
                return;
            }
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), theme::paper(dark));
            let Some(transform) = layout(bounds, &boxes) else {
                return;
            };
            let axis_len = (transform.width - 2.0 * PADDING).max(1.0) * 0.1;
            let origin = transform.position(0.0, 0.0);

            let x_end = transform.position(axis_len / transform.scale, 0.0);
            if inside(bounds, x_end) {
                frame.stroke(
                    &canvas::Path::line(origin, x_end),
                    Stroke::default()
                        .with_color(Color::from_rgb(1.0, 0.0, 0.0))
                        .with_width(1.0),
                );
                frame.fill_text(canvas::Text {
                    content: "X".to_owned(),
                    position: x_end,
                    color: Color::from_rgb(1.0, 0.0, 0.0),
                    size: iced::Pixels(12.0),
                    align_x: iced::alignment::Horizontal::Left.into(),
                    align_y: iced::alignment::Vertical::Bottom,
                    ..canvas::Text::default()
                });
            }

            let axis_label = if self.projection == Projection::XY {
                "Y"
            } else {
                "Z"
            };
            let y_end = transform.position(0.0, axis_len / transform.scale);
            if inside(bounds, y_end) {
                frame.stroke(
                    &canvas::Path::line(origin, y_end),
                    Stroke::default()
                        .with_color(Color::from_rgb(0.0, 1.0, 0.0))
                        .with_width(1.0),
                );
                frame.fill_text(canvas::Text {
                    content: axis_label.to_owned(),
                    position: y_end,
                    color: Color::from_rgb(0.0, 1.0, 0.0),
                    size: iced::Pixels(12.0),
                    align_x: iced::alignment::Horizontal::Left.into(),
                    align_y: iced::alignment::Vertical::Bottom,
                    ..canvas::Text::default()
                });
            }

            for (i, b) in boxes.iter().enumerate() {
                let p1 = transform.position(b.min_x, b.min_y);
                let p2 = transform.position(b.max_x, b.max_y);
                let x = p1.x.min(p2.x);
                let y = p1.y.min(p2.y);
                let w = (p2.x - p1.x).abs().max(1.0);
                let h = (p2.y - p1.y).abs().max(1.0);
                let path = Path::rectangle(Point::new(x, y), Size::new(w, h));
                frame.fill(&path, shape_color(b.label));
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(if state.hovered == Some(i) {
                            theme::accent(dark)
                        } else {
                            theme::ink(dark)
                        })
                        .with_width(if state.hovered == Some(i) { 2.0 } else { 1.0 }),
                );
            }
        });
        vec![geometry]
    }
    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<CanvasMessage>> {
        let point = cursor.position_in(bounds)?;
        if let iced::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) = event {
            let hovered = self.hit_test(point, bounds);
            if hovered != state.hovered {
                state.hovered = hovered;
                self.invalidate_cache();
                return Some(canvas::Action::publish(CanvasMessage::Hover(hovered)));
            }
        }
        None
    }
    fn mouse_interaction(
        &self,
        state: &Self::State,
        _: Rectangle,
        _: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        if state.hovered.is_some() {
            iced::mouse::Interaction::Pointer
        } else {
            iced::mouse::Interaction::default()
        }
    }
}

fn inside(bounds: Rectangle, point: Point) -> bool {
    point.x >= 0.0 && point.y >= 0.0 && point.x <= bounds.width && point.y <= bounds.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> ScriptLine {
        crate::parser::parse_line(&format!("p {x1} {y1} {z1} {x2} {y2} {z2} m"))
    }

    #[test]
    fn projection_selects_the_matching_world_axis_for_the_same_geometry() {
        let mut preview = Preview2D::default();
        preview.set_segments(&[line(0.0, 0.0, 0.0, 1.0, 2.0, 3.0)]);

        let xy = preview.boxes();
        assert_eq!((xy[0].min_y, xy[0].max_y), (0.0, 2.0));

        preview.set_projection(Projection::XZ);
        let xz = preview.boxes();
        assert_eq!((xz[0].min_y, xz[0].max_y), (0.0, 3.0));
    }

    #[test]
    fn data_and_projection_changes_advance_the_render_cache_generation() {
        let mut preview = Preview2D::default();
        preview.set_segments(&[line(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)]);

        // Treat the current generation as a previously rendered frame. Iced's
        // Cache has no public populated-state constructor, so the invalidation
        // epoch verifies the generation transition directly.
        let rendered_generation = preview.cache_invalidation_epoch.get();
        preview.set_projection(Projection::XZ);
        assert_ne!(
            preview.cache_invalidation_epoch.get(),
            rendered_generation,
            "projection changes must invalidate already-rendered geometry"
        );

        let rendered_generation = preview.cache_invalidation_epoch.get();
        preview.set_rects(&[Rect2D {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
        }]);
        assert_ne!(
            preview.cache_invalidation_epoch.get(),
            rendered_generation,
            "new data must invalidate the current rendered geometry"
        );
    }
}
