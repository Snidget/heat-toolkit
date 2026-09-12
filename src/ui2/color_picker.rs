use iced::mouse;
use iced::widget::canvas::{self, Path, Program, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};

use super::app::Message;
use super::theme;

const PLANE_WIDTH: f32 = 220.0;
const PICKER_HEIGHT: f32 = 164.0;
const HUE_X: f32 = 232.0;
const HUE_WIDTH: f32 = 24.0;
const PICKER_WIDTH: f32 = HUE_X + HUE_WIDTH;
const CELL_SIZE: f32 = 4.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DragTarget {
    #[default]
    None,
    SaturationValue,
    Hue,
}

#[derive(Debug, Clone, Copy)]
pub struct ColorPicker {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
}

impl ColorPicker {
    pub fn view(self) -> Element<'static, Message> {
        canvas::Canvas::new(self)
            .width(Length::Fixed(PICKER_WIDTH))
            .height(Length::Fixed(PICKER_HEIGHT))
            .into()
    }

    fn message_for_point(&self, target: DragTarget, point: Point) -> Message {
        match target {
            DragTarget::SaturationValue => Message::AirColorHsvChanged(
                self.hue,
                (point.x / PLANE_WIDTH).clamp(0.0, 1.0),
                (1.0 - point.y / PICKER_HEIGHT).clamp(0.0, 1.0),
            ),
            DragTarget::Hue => Message::AirColorHsvChanged(
                (point.y / PICKER_HEIGHT * 360.0).clamp(0.0, 359.999),
                self.saturation,
                self.value,
            ),
            DragTarget::None => Message::AirColorHsvChanged(self.hue, self.saturation, self.value),
        }
    }
}

impl Program<Message> for ColorPicker {
    type State = DragTarget;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        iced_theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let columns = (PLANE_WIDTH / CELL_SIZE).ceil() as usize;
        let rows = (PICKER_HEIGHT / CELL_SIZE).ceil() as usize;
        for row in 0..rows {
            for column in 0..columns {
                let x = column as f32 * CELL_SIZE;
                let y = row as f32 * CELL_SIZE;
                let saturation = (x / PLANE_WIDTH).clamp(0.0, 1.0);
                let value = (1.0 - y / PICKER_HEIGHT).clamp(0.0, 1.0);
                let color = rgb_color(hsv_to_rgb(self.hue, saturation, value));
                frame.fill_rectangle(
                    Point::new(x, y),
                    Size::new(CELL_SIZE + 0.5, CELL_SIZE + 0.5),
                    color,
                );
            }
        }

        for row in 0..rows {
            let y = row as f32 * CELL_SIZE;
            let hue = y / PICKER_HEIGHT * 360.0;
            frame.fill_rectangle(
                Point::new(HUE_X, y),
                Size::new(HUE_WIDTH, CELL_SIZE + 0.5),
                rgb_color(hsv_to_rgb(hue, 1.0, 1.0)),
            );
        }

        let dark = theme::is_dark(iced_theme);
        let border = theme::line_visible(dark);
        for (origin, size) in [
            (Point::ORIGIN, Size::new(PLANE_WIDTH, PICKER_HEIGHT)),
            (Point::new(HUE_X, 0.0), Size::new(HUE_WIDTH, PICKER_HEIGHT)),
        ] {
            frame.stroke(
                &Path::rectangle(origin, size),
                Stroke::default().with_color(border).with_width(1.0),
            );
        }

        let selector = Point::new(
            (self.saturation.clamp(0.0, 1.0) * PLANE_WIDTH).clamp(6.0, PLANE_WIDTH - 6.0),
            ((1.0 - self.value.clamp(0.0, 1.0)) * PICKER_HEIGHT).clamp(6.0, PICKER_HEIGHT - 6.0),
        );
        let marker_color = if self.value > 0.58 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        frame.stroke(
            &Path::circle(selector, 6.0),
            Stroke::default().with_color(marker_color).with_width(2.0),
        );

        let hue_y = self.hue.rem_euclid(360.0) / 360.0 * PICKER_HEIGHT;
        frame.stroke(
            &Path::rectangle(
                Point::new(HUE_X - 2.0, hue_y - 2.0),
                Size::new(HUE_WIDTH + 4.0, 4.0),
            ),
            Stroke::default()
                .with_color(theme::ink_display(dark))
                .with_width(2.0),
        );

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if *state != DragTarget::None =>
            {
                *state = DragTarget::None;
                Some(canvas::Action::capture())
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                let target = if point.x <= PLANE_WIDTH {
                    DragTarget::SaturationValue
                } else if point.x >= HUE_X && point.x <= HUE_X + HUE_WIDTH {
                    DragTarget::Hue
                } else {
                    DragTarget::None
                };
                if target == DragTarget::None {
                    return None;
                }
                *state = target;
                Some(canvas::Action::publish(self.message_for_point(target, point)).and_capture())
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) if *state != DragTarget::None => {
                let point = cursor.position_in(bounds)?;
                Some(canvas::Action::publish(self.message_for_point(*state, point)).and_capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(point) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        if point.x <= PLANE_WIDTH || (point.x >= HUE_X && point.x <= HUE_X + HUE_WIDTH) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

fn rgb_color((red, green, blue): (u8, u8, u8)) -> Color {
    Color::from_rgb8(red, green, blue)
}

pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (u8, u8, u8) {
    let hue = hue.rem_euclid(360.0);
    let saturation = saturation.clamp(0.0, 1.0);
    let value = value.clamp(0.0, 1.0);
    let chroma = value * saturation;
    let x = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue {
        h if h < 60.0 => (chroma, x, 0.0),
        h if h < 120.0 => (x, chroma, 0.0),
        h if h < 180.0 => (0.0, chroma, x),
        h if h < 240.0 => (0.0, x, chroma),
        h if h < 300.0 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = value - chroma;
    let channel = |value: f32| ((value + offset) * 255.0).round() as u8;
    (channel(red), channel(green), channel(blue))
}

pub fn rgb_to_hsv((red, green, blue): (u8, u8, u8)) -> (f32, f32, f32) {
    let red = red as f32 / 255.0;
    let green = green as f32 / 255.0;
    let blue = blue as f32 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if max == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    let saturation = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    (hue, saturation, max)
}

#[cfg(test)]
mod tests {
    use super::{hsv_to_rgb, rgb_to_hsv};

    #[test]
    fn hsv_picker_reaches_primary_colors_and_black_white() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), (0, 0, 255));
        assert_eq!(hsv_to_rgb(0.0, 0.0, 1.0), (255, 255, 255));
        assert_eq!(hsv_to_rgb(180.0, 1.0, 0.0), (0, 0, 0));
    }

    #[test]
    fn rgb_hsv_roundtrip_is_stable_within_one_channel_step() {
        for color in [(180, 220, 255), (253, 186, 116), (0, 255, 127)] {
            let (hue, saturation, value) = rgb_to_hsv(color);
            let roundtrip = hsv_to_rgb(hue, saturation, value);
            assert_eq!(roundtrip, color);
        }
    }
}
