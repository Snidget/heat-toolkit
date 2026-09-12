// Hallmark · pre-emit critique: P5 H5 E4 S5 R5 V4
// Genre: modern-minimal / Swiss-industrial · Tone: technical and restrained.
// Anchor: cool blue · Structure: persistent workbench rail + focused task canvas.
// Flat neutral surfaces, strict geometry, and one restrained action accent.

use iced::widget::{button, checkbox, container, pick_list, slider, text_input};
use iced::{Background, Border, Color, Theme};

// ─────────────────────────────────────────────────────────────────────────────
// Spacing — one 4 px scale across shell, pages, and controls.
// ─────────────────────────────────────────────────────────────────────────────

pub const SPACE_2XS: f32 = 2.0;
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SPACE_XL: f32 = 24.0;
pub const SPACE_2XL: f32 = 32.0;

pub const PAGE_SPACING: f32 = SPACE_SM;
pub const CONTENT_WIDTH: f32 = 320.0;
pub const TURNER_CONTENT_WIDTH: f32 = 360.0;
pub const LICENSE_CONTENT_WIDTH: f32 = 410.0;
pub const TURNER_PREVIEW_SIZE: f32 = 200.0;

// Shape and control metrics. Exceptions (status dots and data cells) carry
// meaning and stay local to those components.
pub const RADIUS_SM: f32 = 2.0;
pub const RADIUS_MD: f32 = 2.0;
pub const RADIUS_LG: f32 = 4.0;
pub const CONTROL_HEIGHT: f32 = 34.0;
pub const NAV_HEIGHT: f32 = 32.0;
pub const COMPACT_HEIGHT: f32 = 24.0;

// ─────────────────────────────────────────────────────────────────────────────
// Typography — single consistent scale, no per-widget drift
// ─────────────────────────────────────────────────────────────────────────────

/// Display: hero numbers / page titles.
pub const DISPLAY_SIZE: f32 = 22.0;
/// Heading: section titles.
pub const HEADING_SIZE: f32 = 16.0;
/// Body.
pub const BODY_SIZE: f32 = 13.0;
/// Caption / metadata.
pub const SMALL_SIZE: f32 = 11.5;
/// Mono label (ALL CAPS, instrument panel) — same height as body.
pub const LABEL_SIZE: f32 = 10.5;

// ─────────────────────────────────────────────────────────────────────────────
// Color tokens — Dark
// ─────────────────────────────────────────────────────────────────────────────

pub const DARK_PAPER: Color = Color::from_rgb(23.0 / 255.0, 25.0 / 255.0, 27.0 / 255.0); // #17191B
pub const DARK_SURFACE: Color = Color::from_rgb(30.0 / 255.0, 34.0 / 255.0, 37.0 / 255.0); // #1E2225
pub const DARK_SURFACE_RAISED: Color = Color::from_rgb(40.0 / 255.0, 45.0 / 255.0, 49.0 / 255.0); // #282D31
pub const DARK_BORDER: Color = Color::from_rgb(51.0 / 255.0, 57.0 / 255.0, 62.0 / 255.0); // #33393E
pub const DARK_BORDER_VISIBLE: Color = Color::from_rgb(86.0 / 255.0, 96.0 / 255.0, 106.0 / 255.0); // #56606A
pub const DARK_TEXT_DISABLED: Color = Color::from_rgb(119.0 / 255.0, 128.0 / 255.0, 136.0 / 255.0); // #778088
pub const DARK_TEXT_SECONDARY: Color = Color::from_rgb(164.0 / 255.0, 172.0 / 255.0, 179.0 / 255.0); // #A4ACB3
pub const DARK_TEXT_PRIMARY: Color = Color::from_rgb(224.0 / 255.0, 228.0 / 255.0, 231.0 / 255.0); // #E0E4E7
pub const DARK_TEXT_DISPLAY: Color = Color::from_rgb(247.0 / 255.0, 248.0 / 255.0, 249.0 / 255.0); // #F7F8F9

// ─────────────────────────────────────────────────────────────────────────────
// Color tokens — Light
// ─────────────────────────────────────────────────────────────────────────────

pub const LIGHT_PAPER: Color = Color::from_rgb(242.0 / 255.0, 243.0 / 255.0, 244.0 / 255.0); // #F2F3F4
pub const LIGHT_SURFACE: Color = Color::from_rgb(1.0, 1.0, 1.0); // #FFFFFF
pub const LIGHT_SURFACE_RAISED: Color =
    Color::from_rgb(231.0 / 255.0, 234.0 / 255.0, 236.0 / 255.0); // #E7EAEC
pub const LIGHT_BORDER: Color = Color::from_rgb(212.0 / 255.0, 216.0 / 255.0, 219.0 / 255.0); // #D4D8DB
pub const LIGHT_BORDER_VISIBLE: Color =
    Color::from_rgb(174.0 / 255.0, 181.0 / 255.0, 186.0 / 255.0); // #AEB5BA
pub const LIGHT_TEXT_DISABLED: Color = Color::from_rgb(137.0 / 255.0, 145.0 / 255.0, 151.0 / 255.0); // #899197
pub const LIGHT_TEXT_SECONDARY: Color = Color::from_rgb(91.0 / 255.0, 98.0 / 255.0, 104.0 / 255.0); // #5B6268
pub const LIGHT_TEXT_PRIMARY: Color = Color::from_rgb(27.0 / 255.0, 32.0 / 255.0, 36.0 / 255.0); // #1B2024
pub const LIGHT_TEXT_DISPLAY: Color = Color::from_rgb(12.0 / 255.0, 16.0 / 255.0, 19.0 / 255.0); // #0C1013

// ─────────────────────────────────────────────────────────────────────────────
// Accent + semantic status
// ─────────────────────────────────────────────────────────────────────────────

pub const LIGHT_ACCENT: Color = LIGHT_TEXT_PRIMARY;
pub const DARK_ACCENT: Color = DARK_TEXT_PRIMARY;
pub const LIGHT_ACCENT_INK: Color = LIGHT_SURFACE;
pub const DARK_ACCENT_INK: Color = DARK_PAPER;
pub const LIGHT_SUCCESS: Color = Color::from_rgb(20.0 / 255.0, 121.0 / 255.0, 79.0 / 255.0); // #14794F
pub const DARK_SUCCESS: Color = Color::from_rgb(78.0 / 255.0, 195.0 / 255.0, 138.0 / 255.0); // #4EC38A
pub const LIGHT_WARNING: Color = Color::from_rgb(144.0 / 255.0, 95.0 / 255.0, 0.0); // #905F00
pub const DARK_WARNING: Color = Color::from_rgb(232.0 / 255.0, 184.0 / 255.0, 74.0 / 255.0); // #E8B84A
pub const LIGHT_ERROR: Color = Color::from_rgb(196.0 / 255.0, 59.0 / 255.0, 59.0 / 255.0); // #C43B3B
pub const DARK_ERROR: Color = Color::from_rgb(1.0, 123.0 / 255.0, 114.0 / 255.0); // #FF7B72

/// Legacy canvas highlight. UI actions should call `accent(dark)`.
pub const ACCENT: Color = LIGHT_ACCENT;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn is_dark(theme: &Theme) -> bool {
    matches!(theme, Theme::Dark)
}

pub fn paper(dark: bool) -> Color {
    if dark {
        DARK_PAPER
    } else {
        LIGHT_PAPER
    }
}
pub fn surface(dark: bool) -> Color {
    if dark {
        DARK_SURFACE
    } else {
        LIGHT_SURFACE
    }
}
pub fn surface_raised(dark: bool) -> Color {
    if dark {
        DARK_SURFACE_RAISED
    } else {
        LIGHT_SURFACE_RAISED
    }
}
pub fn ink(dark: bool) -> Color {
    if dark {
        DARK_TEXT_PRIMARY
    } else {
        LIGHT_TEXT_PRIMARY
    }
}
pub fn ink_display(dark: bool) -> Color {
    if dark {
        DARK_TEXT_DISPLAY
    } else {
        LIGHT_TEXT_DISPLAY
    }
}
pub fn muted(dark: bool) -> Color {
    if dark {
        DARK_TEXT_SECONDARY
    } else {
        LIGHT_TEXT_SECONDARY
    }
}
pub fn disabled_color(dark: bool) -> Color {
    if dark {
        DARK_TEXT_DISABLED
    } else {
        LIGHT_TEXT_DISABLED
    }
}
pub fn accent(dark: bool) -> Color {
    if dark {
        DARK_ACCENT
    } else {
        LIGHT_ACCENT
    }
}
pub fn accent_ink(dark: bool) -> Color {
    if dark {
        DARK_ACCENT_INK
    } else {
        LIGHT_ACCENT_INK
    }
}
pub fn line(dark: bool) -> Color {
    if dark {
        DARK_BORDER
    } else {
        LIGHT_BORDER
    }
}
pub fn line_visible(dark: bool) -> Color {
    if dark {
        DARK_BORDER_VISIBLE
    } else {
        LIGHT_BORDER_VISIBLE
    }
}

/// Color for the Nothing-style dot-grid background motif. Subtle,
/// never competing with foreground geometry.
pub fn dot_grid_color(dark: bool) -> Color {
    if dark {
        DARK_BORDER_VISIBLE
    } else {
        LIGHT_BORDER_VISIBLE
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Status
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Error,
    Info,
    Muted,
}

pub fn status_color(status: Status, dark: bool) -> Color {
    match status {
        Status::Ok => {
            if dark {
                DARK_SUCCESS
            } else {
                LIGHT_SUCCESS
            }
        }
        Status::Warn => {
            if dark {
                DARK_WARNING
            } else {
                LIGHT_WARNING
            }
        }
        Status::Error => {
            if dark {
                DARK_ERROR
            } else {
                LIGHT_ERROR
            }
        }
        Status::Info => accent(dark),
        Status::Muted => muted(dark),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Button category — hierarchy is encoded by fill, not only border weight.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Filled action accent — reserved for the main action per screen.
    Primary,
    /// Outlined neutral action.
    Secondary,
    /// Text-only / transparent.
    Ghost,
    /// Red-tinted destructive moment.
    Destructive,
}

// ─────────────────────────────────────────────────────────────────────────────
// Backwards-compatible data/rendering constants (legacy call sites)
// ─────────────────────────────────────────────────────────────────────────────

pub const MUTED: Color = LIGHT_TEXT_DISABLED;

/// Semi-transparent backdrop for modal overlays.
pub fn backdrop(dark: bool) -> Color {
    if dark {
        Color::from_rgba(0.0, 0.0, 0.0, 0.66)
    } else {
        Color::from_rgba(0.0, 0.0, 0.0, 0.38)
    }
}

/// Resolve a semantic action category to its resting surface.
pub fn category_fill(category: Category, dark: bool) -> Color {
    match category {
        Category::Primary => accent(dark),
        Category::Secondary => surface(dark),
        Category::Ghost => surface(dark),
        Category::Destructive => blend(
            surface(dark),
            status_color(Status::Error, dark),
            if dark { 0.12 } else { 0.07 },
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Blend helper
// ─────────────────────────────────────────────────────────────────────────────

pub fn blend(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        1.0,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Container styles
// ─────────────────────────────────────────────────────────────────────────────

pub fn panel(theme: &Theme) -> container::Style {
    let dark = is_dark(theme);
    container::Style {
        background: Some(Background::Color(paper(dark))),
        text_color: Some(ink(dark)),
        border: Border::default(),
        shadow: Default::default(),
        snap: true,
    }
}

pub fn side_panel(theme: &Theme) -> container::Style {
    let dark = is_dark(theme);
    container::Style {
        background: Some(Background::Color(surface(dark))),
        text_color: Some(ink(dark)),
        border: Border {
            color: line(dark),
            width: 0.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
        snap: true,
    }
}

/// Quiet sidebar surface separated from the workspace by a single rule.
pub fn side_panel_rail(theme: &Theme) -> container::Style {
    let dark = is_dark(theme);
    container::Style {
        background: Some(Background::Color(surface(dark))),
        text_color: Some(ink(dark)),
        border: Border {
            color: line(dark),
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
        snap: true,
    }
}

pub fn card(theme: &Theme) -> container::Style {
    let dark = is_dark(theme);
    container::Style {
        background: Some(Background::Color(surface(dark))),
        text_color: Some(ink(dark)),
        border: Border {
            color: line(dark),
            width: 1.0,
            radius: RADIUS_LG.into(),
        },
        shadow: Default::default(),
        snap: true,
    }
}

pub fn status_panel_style(theme: &Theme, status: Status) -> container::Style {
    let dark = is_dark(theme);
    let color = status_color(status, dark);
    let bg = blend(surface(dark), color, if dark { 0.10 } else { 0.05 });
    container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(color),
        border: Border {
            color: line(dark),
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        shadow: Default::default(),
        snap: true,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Button styles
// ─────────────────────────────────────────────────────────────────────────────

pub fn button_style(category: Category) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let dark = is_dark(theme);
        let disabled = matches!(status, button::Status::Disabled);
        let hovered = matches!(status, button::Status::Hovered);
        let pressed = matches!(status, button::Status::Pressed);
        let base = category_fill(category, dark);

        let (background, text_color) = if disabled {
            (
                Some(Background::Color(surface_raised(dark))),
                disabled_color(dark),
            )
        } else {
            match category {
                Category::Primary => {
                    let bg = if pressed {
                        blend(base, DARK_PAPER, 0.18)
                    } else if hovered {
                        blend(base, LIGHT_SURFACE, 0.10)
                    } else {
                        base
                    };
                    (Some(Background::Color(bg)), accent_ink(dark))
                }
                Category::Destructive => {
                    let error = status_color(Status::Error, dark);
                    let bg = if hovered || pressed {
                        Some(Background::Color(blend(
                            surface(dark),
                            error,
                            if dark { 0.14 } else { 0.07 },
                        )))
                    } else {
                        None
                    };
                    (bg, error)
                }
                Category::Secondary => {
                    let bg = if hovered || pressed {
                        surface_raised(dark)
                    } else {
                        surface(dark)
                    };
                    (Some(Background::Color(bg)), ink(dark))
                }
                Category::Ghost => {
                    let bg = if hovered || pressed {
                        Some(Background::Color(surface_raised(dark)))
                    } else {
                        None
                    };
                    (bg, muted(dark))
                }
            }
        };

        let border_color = if disabled {
            line(dark)
        } else {
            match category {
                Category::Primary => accent(dark),
                Category::Destructive => {
                    blend(status_color(Status::Error, dark), line_visible(dark), 0.35)
                }
                Category::Secondary => {
                    if hovered || pressed {
                        line_visible(dark)
                    } else {
                        line(dark)
                    }
                }
                Category::Ghost => Color::TRANSPARENT,
            }
        };

        button::Style {
            background,
            text_color,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: RADIUS_SM.into(),
            },
            shadow: Default::default(),
            snap: true,
        }
    }
}

pub fn nav_button_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let dark = is_dark(theme);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (background, text_color) = if selected {
            (Some(Background::Color(accent(dark))), accent_ink(dark))
        } else if hovered {
            (Some(Background::Color(surface_raised(dark))), ink(dark))
        } else {
            (None, ink(dark))
        };
        button::Style {
            background,
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: RADIUS_SM.into(),
            },
            shadow: Default::default(),
            snap: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Native iced widget themes — flat and aligned with shared control metrics.
// ─────────────────────────────────────────────────────────────────────────────

pub fn checkbox_style(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let dark = is_dark(theme);
    let checked = match status {
        checkbox::Status::Active { is_checked }
        | checkbox::Status::Hovered { is_checked }
        | checkbox::Status::Disabled { is_checked } => is_checked,
    };
    checkbox::Style {
        background: Background::Color(if checked {
            ink_display(dark)
        } else {
            surface(dark)
        }),
        icon_color: if checked { paper(dark) } else { muted(dark) },
        border: Border {
            color: if checked {
                ink_display(dark)
            } else {
                line_visible(dark)
            },
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        text_color: Some(ink(dark)),
    }
}

pub fn slider_style(theme: &Theme, status: slider::Status) -> slider::Style {
    let dark = is_dark(theme);
    let hovered = matches!(status, slider::Status::Dragged | slider::Status::Hovered);
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Background::Color(line_visible(dark)),
                Background::Color(line_visible(dark)),
            ),
            width: 3.0,
            border: Border::default(),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 7.0 },
            background: Background::Color(if hovered {
                ink_display(dark)
            } else {
                ink(dark)
            }),
            border_width: 2.0,
            border_color: surface(dark),
        },
    }
}

pub fn pick_list_style(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let dark = is_dark(theme);
    let hovered = match status {
        pick_list::Status::Active => false,
        pick_list::Status::Hovered => true,
        pick_list::Status::Opened { .. } => true,
    };
    pick_list::Style {
        text_color: ink(dark),
        placeholder_color: muted(dark),
        handle_color: if hovered {
            ink_display(dark)
        } else {
            muted(dark)
        },
        background: Background::Color(surface(dark)),
        border: Border {
            color: if hovered {
                ink_display(dark)
            } else {
                line_visible(dark)
            },
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
    }
}

/// Dropdown menu style — matches the pick_list / theme tokens exactly.
pub fn pick_list_menu_style(theme: &Theme) -> iced::widget::overlay::menu::Style {
    let dark = is_dark(theme);
    iced::widget::overlay::menu::Style {
        background: Background::Color(surface(dark)),
        border: Border {
            color: line_visible(dark),
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        text_color: ink(dark),
        selected_text_color: ink_display(dark),
        selected_background: Background::Color(surface_raised(dark)),
        shadow: iced::Shadow::default(),
    }
}

pub fn text_input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let dark = is_dark(theme);
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: Background::Color(surface(dark)),
        border: Border {
            color: if focused {
                ink_display(dark)
            } else {
                line_visible(dark)
            },
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        icon: muted(dark),
        placeholder: muted(dark),
        value: ink(dark),
        selection: blend(surface_raised(dark), accent(dark), 0.35),
    }
}

/// Compact selected/unselected control used by tables and segmented choices.
pub fn selectable_button_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let dark = is_dark(theme);
        let disabled = matches!(status, button::Status::Disabled);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let action = accent(dark);
        let background = if disabled {
            surface_raised(dark)
        } else if selected {
            if hovered {
                blend(action, DARK_PAPER, 0.12)
            } else {
                action
            }
        } else if hovered {
            surface_raised(dark)
        } else {
            surface(dark)
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: if disabled {
                disabled_color(dark)
            } else if selected {
                accent_ink(dark)
            } else {
                ink(dark)
            },
            border: Border {
                color: if selected && !disabled {
                    action
                } else {
                    line_visible(dark)
                },
                width: 1.0,
                radius: RADIUS_SM.into(),
            },
            shadow: Default::default(),
            snap: true,
        }
    }
}

/// Square data-surface control for chart/table affordances. Its zero radius is
/// a deliberate exception matching the rectangular data cells it contains.
pub fn data_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let dark = is_dark(theme);
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(surface(dark))),
        text_color: ink(dark),
        border: Border {
            color: if hovered {
                accent(dark)
            } else {
                line_visible(dark)
            },
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Default::default(),
        snap: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CONTENT_WIDTH, DARK_ACCENT, DARK_TEXT_PRIMARY, LIGHT_ACCENT, LIGHT_TEXT_PRIMARY,
        TURNER_CONTENT_WIDTH,
    };

    const _: () = assert!(TURNER_CONTENT_WIDTH > CONTENT_WIDTH);

    #[test]
    fn action_accents_use_neutral_ink_tokens() {
        assert_eq!(LIGHT_ACCENT, LIGHT_TEXT_PRIMARY);
        assert_eq!(DARK_ACCENT, DARK_TEXT_PRIMARY);
    }
}
