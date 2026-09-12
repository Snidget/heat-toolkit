use iced::widget::canvas::{self, Cache, Geometry, Program};
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme, Vector};

use super::theme;

#[derive(Debug, Clone)]
pub enum CanvasMessage {
    Hover(Option<usize>),
    Drag(Vector),
}

#[derive(Default)]
pub struct CanvasState {
    pub cache: Cache,
    pub hovered: Option<usize>,
    pub drag_delta: Vector,
}

pub fn stroke(color: Color, width: f32) -> canvas::Stroke<'static> {
    canvas::Stroke::default()
        .with_color(color)
        .with_width(width)
}

pub fn centered_text(content: String, position: Point, color: Color, size: f32) -> canvas::Text {
    canvas::Text {
        content,
        position,
        color,
        size: iced::Pixels(size),
        ..canvas::Text::default()
    }
}

pub fn path_rectangle(x: f32, y: f32, width: f32, height: f32) -> canvas::Path {
    canvas::Path::rectangle(Point::new(x, y), iced::Size::new(width, height))
}

/// Nothing-style dot-grid background: 1px dots on a 12px grid.
/// Used as the empty-state backdrop of preview canvases.
pub fn draw_dot_grid(frame: &mut canvas::Frame, bounds: Rectangle, dark: bool) {
    frame.fill_rectangle(Point::ORIGIN, bounds.size(), theme::paper(dark));
    let dot = theme::dot_grid_color(dark);
    let step = 12.0_f32;
    let radius = 0.6_f32;
    let cols = (bounds.width / step).ceil() as i32 + 1;
    let rows = (bounds.height / step).ceil() as i32 + 1;
    for col in 0..cols {
        for row in 0..rows {
            let cx = col as f32 * step;
            let cy = row as f32 * step;
            if cx > bounds.width || cy > bounds.height {
                continue;
            }
            frame.fill(&canvas::Path::circle(Point::new(cx, cy), radius), dot);
        }
    }
}

pub struct SceneProgram<S> {
    pub scene: S,
}

impl<S> Program<CanvasMessage> for SceneProgram<S>
where
    S: Default + 'static,
{
    type State = CanvasState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = state.cache.draw(renderer, bounds.size(), |_| {});
        vec![geometry]
    }
}

/// Standalone dot-grid canvas — used as a decorative background layer
/// (Nothing motif) behind surfaces like the about card.
pub struct DotGrid;

impl Program<CanvasMessage> for DotGrid {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        draw_dot_grid(&mut frame, bounds, theme::is_dark(theme));
        vec![frame.into_geometry()]
    }
}

/// Widget that renders the dot grid across a full container.
pub fn dot_grid_canvas() -> iced::widget::Canvas<DotGrid, CanvasMessage, Theme, Renderer> {
    iced::widget::canvas(DotGrid)
}
