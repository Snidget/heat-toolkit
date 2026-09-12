use std::collections::HashMap;

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::clipboard;
use crate::parser::{parse_script, ScriptLine};
use crate::step_export::write_step;

use super::app::Message;
use super::preview3d::{self, Canvas3DMessage, Preview3D, StandardView};
use super::theme;
use super::widgets::{hero_metric, page_button, page_button_maybe, status_text};

pub struct Step3DPage {
    pub status: Option<(String, bool)>,
    pub azimuth: f64,
    pub elevation: f64,
    pub boxes: usize,
    pub lines: Vec<ScriptLine>,
    pub active_view: Option<StandardView>,
    pub material_colors: HashMap<String, iced::Color>,
}

impl Default for Step3DPage {
    fn default() -> Self {
        Self {
            status: None,
            azimuth: 30.0f64.to_radians(),
            elevation: 25.0f64.to_radians(),
            boxes: 0,
            lines: Vec::new(),
            active_view: None,
            material_colors: HashMap::new(),
        }
    }
}

impl Step3DPage {
    pub fn sync_lines(&mut self, script: &str) {
        let lines = parse_script(script);
        self.boxes = lines.iter().filter(|line| line.segment.is_some()).count();
        self.lines = lines;
    }

    pub fn set_material_colors(&mut self, colors: HashMap<String, iced::Color>) {
        self.material_colors = colors;
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(page_button(
            "Вставить из буфера",
            theme::Category::Primary,
            true,
            Message::StepPaste,
        ));
        content = content.push(hero_metric(
            "ЭЛЕМЕНТОВ",
            self.boxes.to_string(),
            if self.boxes == 0 {
                theme::Status::Muted
            } else {
                theme::Status::Ok
            },
        ));

        if self.boxes == 0 {
            if let Some((message, error)) = &self.status {
                content = content.push(status_text(message, *error));
            }
            content = content.push(text("Нет данных для экспорта.").size(theme::BODY_SIZE));
            content = content.push(
                Preview3D::view(
                    &self.lines,
                    &self.material_colors,
                    self.azimuth,
                    self.elevation,
                )
                .map(|canvas_message| match canvas_message {
                    Canvas3DMessage::Rotated(azimuth, elevation) => {
                        Message::Step3DRotated(azimuth, elevation)
                    }
                }),
            );
            content = content.push(page_button_maybe(
                "Экспорт в STEP",
                theme::Category::Secondary,
                true,
                None,
            ));
            return content.into();
        }

        let mut view_buttons = column![text("Стандартный вид")
            .font(super::widgets::body_font(true))
            .size(theme::SMALL_SIZE)]
        .spacing(theme::PAGE_SPACING);
        for standard_row in preview3d::STANDARD_VIEWS {
            let mut button_row = iced::widget::Row::new().spacing(theme::SPACE_XS);
            for (view, label) in standard_row {
                button_row = button_row.push(page_button(
                    label,
                    theme::Category::Secondary,
                    false,
                    Message::Step3DView(view),
                ));
            }
            view_buttons = view_buttons.push(button_row);
        }
        let preview = Preview3D::view(
            &self.lines,
            &self.material_colors,
            self.azimuth,
            self.elevation,
        );
        content = content
            .push(
                container(view_buttons)
                    .width(Length::Fill)
                    .padding(theme::SPACE_XS),
            )
            .push(preview.map(|canvas_message| match canvas_message {
                Canvas3DMessage::Rotated(azimuth, elevation) => {
                    Message::Step3DRotated(azimuth, elevation)
                }
            }));

        content = content.push(page_button(
            "Экспорт в STEP",
            theme::Category::Secondary,
            true,
            Message::StepExport,
        ));
        if let Some((message, error)) = &self.status {
            content = content.push(status_text(message, *error));
        }
        content.into()
    }
}

pub fn count_boxes(script: &str) -> usize {
    parse_script(script)
        .iter()
        .filter(|line| line.segment.is_some())
        .count()
}
pub fn export_script(script: &str) -> Result<String, String> {
    let boxes = parse_script(script)
        .iter()
        .filter_map(|line| line.segment.map(|segment| segment.as_tuple()))
        .collect::<Vec<_>>();
    if boxes.is_empty() {
        return Err("Нет данных для экспорта.".to_owned());
    }
    let path = rfd::FileDialog::new()
        .add_filter("STEP files", &["step", "stp"])
        .set_file_name("model.step")
        .save_file()
        .ok_or_else(|| "Экспорт отменён.".to_owned())?;
    write_step(&path, &boxes)
        .map(|_| format!("Модель экспортирована: {}", path.display()))
        .map_err(|error| format!("Ошибка экспорта STEP: {error}"))
}
pub fn paste() -> Result<String, String> {
    clipboard::read_text()
}
