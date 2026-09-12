// Shared widget kit for the HEAT3 desktop utility.
// JetBrains Mono carries the technical display register; Inter carries UI copy.

use iced::widget::{button, column, container, row, rule, text, Column};
use iced::{Background, Border, Color, Element, Font, Length, Theme};

use super::app::Message;
use super::fonts::{INTER_REGULAR, INTER_SEMIBOLD, JETBRAINS_MONO, JETBRAINS_MONO_BOLD};
use super::theme::{self, Category, Status};

// ─────────────────────────────────────────────────────────────────────────────
// Font helpers — one token source, no per-widget family declarations.
// ─────────────────────────────────────────────────────────────────────────────

/// Technical display: page title and primary numeric readout.
pub fn display_font() -> Font {
    JETBRAINS_MONO_BOLD
}

/// Body / UI text.
pub fn body_font(strong: bool) -> Font {
    if strong {
        INTER_SEMIBOLD
    } else {
        INTER_REGULAR
    }
}

/// One immutable face for every interactive label.
///
/// Button hierarchy comes from surface, border and color—not from changing
/// glyph metrics between states or categories.
pub fn button_font() -> Font {
    INTER_REGULAR
}

/// Labels / meta / instrument text.
pub fn mono_font() -> Font {
    JETBRAINS_MONO
}

/// Emphasized technical labels and metrics.
pub fn mono_bold() -> Font {
    JETBRAINS_MONO_BOLD
}

// ─────────────────────────────────────────────────────────────────────────────
// Section title
// ─────────────────────────────────────────────────────────────────────────────

pub fn section_title<'a>(label: &'a str) -> Element<'a, Message> {
    container(
        text(label)
            .size(theme::SMALL_SIZE)
            .font(body_font(true))
            .style(|theme| text::Style {
                color: Some(theme::ink(theme::is_dark(theme))),
            }),
    )
    .width(Length::Fill)
    .padding(
        iced::Padding::new(0.0)
            .top(theme::SPACE_LG)
            .bottom(theme::SPACE_XS),
    )
    .into()
}

/// Larger hero title — display grotesk SemiBold, used once per screen.
pub fn page_title<'a>(label: &'a str) -> Element<'a, Message> {
    text(label)
        .size(theme::DISPLAY_SIZE)
        .font(display_font())
        .style(|theme| text::Style {
            color: Some(theme::ink_display(theme::is_dark(theme))),
        })
        .into()
}

/// Quiet supporting label. Technical values opt into monospace separately.
pub fn meta_label<'a>(label: impl Into<String>) -> Element<'a, Message> {
    text(label.into())
        .size(theme::SMALL_SIZE)
        .font(body_font(false))
        .style(|theme| text::Style {
            color: Some(theme::muted(theme::is_dark(theme))),
        })
        .into()
}

/// Compact readout. It deliberately avoids a card or dashboard-style metric.
pub fn hero_metric<'a>(label: &'a str, value: String, status: Status) -> Element<'a, Message> {
    container(
        row![
            text(value)
                .size(18.0)
                .font(mono_bold())
                .style(move |theme| {
                    let dark = theme::is_dark(theme);
                    text::Style {
                        color: Some(if status == Status::Muted {
                            theme::ink_display(dark)
                        } else {
                            theme::status_color(status, dark)
                        }),
                    }
                }),
            text(label)
                .size(theme::SMALL_SIZE)
                .font(body_font(false))
                .style(move |theme| text::Style {
                    color: Some(theme::muted(theme::is_dark(theme))),
                }),
        ]
        .spacing(theme::SPACE_SM)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([theme::SPACE_SM, 0.0])
    .into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Buttons
// ─────────────────────────────────────────────────────────────────────────────

pub fn page_button<'a>(
    label: impl Into<String>,
    category: Category,
    _bold: bool,
    message: Message,
) -> Element<'a, Message> {
    page_button_maybe(label, category, _bold, Some(message))
}

pub fn page_button_maybe<'a>(
    label: impl Into<String>,
    category: Category,
    _bold: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    page_button_sized(label, category, _bold, message, theme::CONTROL_HEIGHT)
}

/// Button with an explicit height for context-specific control geometry.
pub fn page_button_sized<'a>(
    label: impl Into<String>,
    category: Category,
    _bold: bool,
    message: Option<Message>,
    height: f32,
) -> Element<'a, Message> {
    let label = label.into();
    let style_fn = theme::button_style(category);
    let label = text(label)
        .size(theme::BODY_SIZE)
        .font(button_font())
        .width(Length::Fill)
        .align_x(iced::Alignment::Center)
        .wrapping(iced::widget::text::Wrapping::None);
    let content = container(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center);
    button(content)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding([0.0, theme::SPACE_MD])
        .style(move |theme: &Theme, status| style_fn(theme, status))
        .on_press_maybe(message)
        .into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Nav button — centered, compact, with one persistent selected state.
// ─────────────────────────────────────────────────────────────────────────────

pub fn nav_button<'a>(label: &'a str, selected: bool, message: Message) -> Element<'a, Message> {
    let style_fn = theme::nav_button_style(selected);
    let label = text(label)
        .size(theme::BODY_SIZE)
        .font(button_font())
        .width(Length::Fill)
        .align_x(iced::Alignment::Center);
    let content = container(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center);
    button(content)
        .width(Length::Fill)
        .height(Length::Fixed(theme::NAV_HEIGHT))
        .padding([0.0, theme::SPACE_SM])
        .style(move |theme: &Theme, status| style_fn(theme, status))
        .on_press(message)
        .into()
}

/// Small square button for arrows / symbols — consistent with page buttons.
pub fn square_button<'a>(label: &'a str, message: Option<Message>) -> Element<'a, Message> {
    let style_fn = theme::button_style(Category::Secondary);
    let label = text(label)
        .size(theme::BODY_SIZE)
        .font(button_font())
        .width(Length::Fill)
        .align_x(iced::Alignment::Center);
    let content = container(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center);
    button(content)
        .width(Length::Fixed(theme::NAV_HEIGHT))
        .height(Length::Fixed(theme::NAV_HEIGHT))
        .style(move |theme: &Theme, status| style_fn(theme, status))
        .on_press_maybe(message)
        .into()
}

/// Intrinsic-width button for controls that sit beside an expanding field.
pub fn inline_button_maybe<'a>(
    label: impl Into<String>,
    category: Category,
    _bold: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    let label = label.into();
    let style_fn = theme::button_style(category);
    let label = text(label)
        .size(theme::BODY_SIZE)
        .font(button_font())
        .width(Length::Fill)
        .align_x(iced::Alignment::Center);
    let content = container(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center);
    button(content)
        .height(Length::Fixed(theme::CONTROL_HEIGHT))
        .padding([0.0, theme::SPACE_MD])
        .style(move |theme: &Theme, status| style_fn(theme, status))
        .on_press_maybe(message)
        .into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Status panel — flat surface with status tint, no shadows
// ─────────────────────────────────────────────────────────────────────────────

pub fn status_panel<'a>(
    title: &'a str,
    message: Option<String>,
    status: Status,
) -> Element<'a, Message> {
    let mut content = Column::new().spacing(theme::SPACE_XS).push(
        text(title)
            .size(theme::BODY_SIZE)
            .font(body_font(true))
            .style(move |theme| text::Style {
                color: Some(theme::status_color(status, theme::is_dark(theme))),
            }),
    );
    if let Some(value) = message {
        content =
            content.push(
                text(value)
                    .size(theme::BODY_SIZE)
                    .style(move |theme| text::Style {
                        color: Some(theme::ink(theme::is_dark(theme))),
                    }),
            );
    }
    container(content)
        .width(Length::Fill)
        .padding(theme::SPACE_MD)
        .style(move |theme| theme::status_panel_style(theme, status))
        .into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Modal — dimmed backdrop + centered card. One element per screen.
// ─────────────────────────────────────────────────────────────────────────────

pub fn modal<'a>(body: Element<'a, Message>, width: f32) -> Element<'a, Message> {
    container(
        container(body)
            .width(Length::Fixed(width))
            .padding(theme::SPACE_LG)
            .style(theme::card),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center)
    .style(move |theme| container::Style {
        background: Some(Background::Color(theme::backdrop(theme::is_dark(theme)))),
        ..Default::default()
    })
    .into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline status — "[OK] message" / "[ERROR] message" mono caps
// ─────────────────────────────────────────────────────────────────────────────

pub fn status_text<'a>(message: &'a str, error: bool) -> Element<'a, Message> {
    let status = if error { Status::Error } else { Status::Ok };
    let prefix = if error { "[ERROR] " } else { "[OK] " };
    let label = format!("{prefix}{message}");
    text(label)
        .size(theme::SMALL_SIZE)
        .font(mono_font())
        .style(move |theme| text::Style {
            color: Some(theme::status_color(status, theme::is_dark(theme))),
        })
        .into()
}

pub fn muted_text<'a>(message: &'a str) -> Element<'a, Message> {
    text(message)
        .size(theme::SMALL_SIZE)
        .font(mono_font())
        .style(|theme| text::Style {
            color: Some(theme::muted(theme::is_dark(theme))),
        })
        .into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Card
// ─────────────────────────────────────────────────────────────────────────────

pub fn page_card<'a>(title: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    column![
        section_title(title),
        container(body)
            .width(Length::Fill)
            .padding(theme::SPACE_LG)
            .style(theme::card),
    ]
    .spacing(theme::SPACE_SM)
    .into()
}

/// Compact selected control for dense tables and segmented choices.
pub fn selectable_button<'a>(
    label: impl Into<String>,
    selected: bool,
    message: Message,
    height: f32,
    width: Length,
) -> Element<'a, Message> {
    selectable_button_with_font(label, selected, message, height, width, button_font())
}

/// Compact numeric control for dense data tables.
pub fn selectable_data_button<'a>(
    label: impl Into<String>,
    selected: bool,
    message: Message,
    height: f32,
    width: Length,
) -> Element<'a, Message> {
    selectable_button_with_font(label, selected, message, height, width, mono_font())
}

fn selectable_button_with_font<'a>(
    label: impl Into<String>,
    selected: bool,
    message: Message,
    height: f32,
    width: Length,
    font: Font,
) -> Element<'a, Message> {
    let style_fn = theme::selectable_button_style(selected);
    let label = label.into();
    let content = container(
        text(label)
            .size(theme::SMALL_SIZE)
            .font(font)
            .width(Length::Fill)
            .align_x(iced::Alignment::Center)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center);

    button(content)
        .width(width)
        .height(Length::Fixed(height))
        .padding([0.0, theme::SPACE_SM])
        .style(move |theme: &Theme, status| style_fn(theme, status))
        .on_press(message)
        .into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Rule + spacer
// ─────────────────────────────────────────────────────────────────────────────

pub fn separator<'a>() -> Element<'a, Message> {
    rule::horizontal(1.0)
        .style(move |theme| {
            let dark = theme::is_dark(theme);
            rule::Style {
                color: theme::line(dark),
                radius: 0.0.into(),
                fill_mode: rule::FillMode::Full,
                snap: true,
            }
        })
        .into()
}

pub fn hairline<'a>() -> Element<'a, Message> {
    rule::horizontal(1.0)
        .style(move |theme| {
            let dark = theme::is_dark(theme);
            rule::Style {
                color: theme::line(dark),
                radius: 0.0.into(),
                fill_mode: rule::FillMode::Full,
                snap: true,
            }
        })
        .into()
}

pub fn spacer() -> Element<'static, Message> {
    iced::widget::space::vertical().into()
}

/// Small colored status dot — used in inline rows.
pub fn status_dot<'a>(status: Status) -> Element<'a, Message> {
    container(text(""))
        .width(Length::Fixed(6.0))
        .height(Length::Fixed(6.0))
        .style(move |theme| {
            let dark = theme::is_dark(theme);
            container::Style {
                background: Some(Background::Color(theme::status_color(status, dark))),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 999.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}
