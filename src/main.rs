#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> iced::Result {
    iced::application(
        heat3_povorotnik::ui2::app::App::default,
        heat3_povorotnik::ui2::app::update,
        heat3_povorotnik::ui2::app::view,
    )
    .font(heat3_povorotnik::ui2::fonts::INTER_REGULAR_DATA)
    .font(heat3_povorotnik::ui2::fonts::INTER_SEMIBOLD_DATA)
    .font(heat3_povorotnik::ui2::fonts::JETBRAINS_MONO_DATA)
    .font(heat3_povorotnik::ui2::fonts::JETBRAINS_MONO_BOLD_DATA)
    .default_font(heat3_povorotnik::ui2::fonts::INTER_REGULAR)
    .subscription(heat3_povorotnik::ui2::app::subscription)
    .theme(heat3_povorotnik::ui2::app::theme)
    .title(heat3_povorotnik::ui2::platform::WINDOW_TITLE)
    .window(iced::window::Settings {
        size: iced::Size::new(460.0, 600.0),
        min_size: Some(iced::Size::new(460.0, 600.0)),
        max_size: Some(iced::Size::new(460.0, 600.0)),
        resizable: false,
        level: iced::window::Level::AlwaysOnTop,
        icon: heat3_povorotnik::ui2::platform::app_icon(),
        ..Default::default()
    })
    .run()
}
