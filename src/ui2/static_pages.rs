use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{Element, Length, Theme};

use crate::air_cavities::{
    build_air_cavity_materials, parse_air_cavities_info_log, upsert_air_cavity_materials,
    AirCavityMaterialSpec,
};
use crate::clipboard;
use crate::material_sort::{
    extract_material_entries, materials_by_normalized_name, normalize_material_name,
    parse_mtl_file, sort_material_boxes_by_order, sort_material_names_by_conductivity, MtlMaterial,
};
use crate::parser::ScriptLine;

use super::app::Message;
use super::color_picker::{hsv_to_rgb, rgb_to_hsv, ColorPicker};
use super::preview3d::{self, Canvas3DMessage, Preview3D, StandardView};
use super::theme;
use super::widgets::{
    hero_metric, meta_label, page_button, page_button_maybe, page_title, section_title, separator,
    status_text,
};

fn parse_color_draft(channels: &[String; 3]) -> Result<(u8, u8, u8), String> {
    let parse = |label: &str, value: &str| {
        value
            .trim()
            .parse::<u8>()
            .map_err(|_| format!("Канал {label}: введите целое число от 0 до 255."))
    };

    Ok((
        parse("R", &channels[0])?,
        parse("G", &channels[1])?,
        parse("B", &channels[2])?,
    ))
}

fn rgb_input<'a>(
    label: &'static str,
    draft: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message> {
    column![
        meta_label(label),
        iced::widget::text_input("0–255", draft)
            .style(theme::text_input_style)
            .size(theme::BODY_SIZE)
            .padding([theme::SPACE_XS, theme::SPACE_SM])
            .on_input(on_input),
    ]
    .spacing(theme::SPACE_2XS)
    .width(Length::Fill)
    .into()
}

#[derive(Default)]
pub struct MaterialSortPage {
    pub names: Vec<String>,
    pub status: Option<(String, bool)>,
    pub mtl_materials: HashMap<String, MtlMaterial>,
    pub mtl_path: Option<PathBuf>,
    cached_script: String,
}

impl MaterialSortPage {
    pub fn sync_script(&mut self, script: &str) {
        if self.cached_script == script {
            return;
        }
        self.cached_script = script.to_owned();
        self.names = extract_material_entries(script)
            .into_iter()
            .map(|entry| entry.name)
            .collect();
    }

    pub fn move_item(&mut self, index: usize, delta: isize) {
        let target = index as isize + delta;
        if target >= 0 && (target as usize) < self.names.len() {
            self.names.swap(index, target as usize);
        }
    }

    pub fn lambda_for(&self, name: &str) -> Option<f64> {
        self.mtl_materials
            .get(&normalize_material_name(name))
            .map(|material| material.thermal_x)
    }

    pub fn color_map(&self) -> HashMap<String, iced::Color> {
        let materials: Vec<MtlMaterial> = self.mtl_materials.values().cloned().collect();
        materials_by_normalized_name(&materials)
            .into_iter()
            .map(|(name, material)| {
                (
                    name,
                    iced::Color::from_rgb(
                        material.rgb_r as f32 / 255.0,
                        material.rgb_g as f32 / 255.0,
                        material.rgb_b as f32 / 255.0,
                    ),
                )
            })
            .collect()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let can_sort = !self.names.is_empty() && !self.mtl_materials.is_empty();
        let matched = self
            .names
            .iter()
            .filter(|name| self.lambda_for(name).is_some())
            .count();
        let names = self.names.clone();
        let total = names.len();

        let list: Element<'_, Message> = if names.is_empty() {
            iced::widget::space::vertical().into()
        } else {
            let items: Vec<Element<'_, Message>> = names
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    let conductivity = self.lambda_for(&name);
                    let content: Element<'_, Message> = container(
                        row![
                            column![
                                text(name)
                                    .size(theme::BODY_SIZE)
                                    .style(move |theme| text::Style {
                                        color: Some(theme::ink(theme::is_dark(theme))),
                                    }),
                                match conductivity {
                                    Some(value) => {
                                        let t: iced::widget::Text<'_, iced::Theme, iced::Renderer> =
                                            text(format!(
                                                "Теплопроводность: {}",
                                                crate::text::format_g(value, 6)
                                            ))
                                            .font(super::widgets::mono_font())
                                            .size(theme::BODY_SIZE);
                                        Element::from(t)
                                    }
                                    None => {
                                        let t: iced::widget::Text<'_, iced::Theme, iced::Renderer> =
                                            text("Теплопроводность: нет данных")
                                                .size(theme::BODY_SIZE)
                                                .style(move |theme| text::Style {
                                                    color: Some(theme::muted(theme::is_dark(
                                                        theme,
                                                    ))),
                                                });
                                        Element::from(t)
                                    }
                                }
                            ]
                            .spacing(theme::SPACE_2XS)
                            .width(Length::Fill),
                            column![
                                super::widgets::with_tooltip(
                                    super::widgets::square_button(
                                        "↑",
                                        if index > 0 {
                                            Some(Message::MaterialMove(index, -1))
                                        } else {
                                            None
                                        }
                                    ),
                                    "Переместить материал выше",
                                ),
                                super::widgets::with_tooltip(
                                    super::widgets::square_button(
                                        "↓",
                                        if index + 1 < total {
                                            Some(Message::MaterialMove(index, 1))
                                        } else {
                                            None
                                        }
                                    ),
                                    "Переместить материал ниже",
                                ),
                            ]
                            .spacing(theme::SPACE_2XS),
                        ]
                        .spacing(theme::SPACE_XS),
                    )
                    .padding(theme::SPACE_SM)
                    .width(Length::Fill)
                    .style(theme::card)
                    .into();
                    content
                })
                .collect();
            scrollable(column(items))
                .height(Length::Fixed(300.0))
                .into()
        };

        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(hero_metric(
            "МАТЕРИАЛОВ В СКРИПТЕ",
            total.to_string(),
            if total == 0 {
                theme::Status::Muted
            } else {
                theme::Status::Ok
            },
        ));
        content = content.push(page_button(
            "Вставить скрипт",
            theme::Category::Primary,
            true,
            Message::MaterialPaste,
        ));
        content = content.push(page_button(
            "Открыть файл материалов",
            theme::Category::Secondary,
            true,
            Message::MaterialOpen,
        ));
        content = content.push(page_button_maybe(
            "Сортировать по теплопроводности",
            theme::Category::Secondary,
            true,
            if can_sort {
                Some(Message::MaterialSort)
            } else {
                None
            },
        ));
        content = content.push(section_title("СПИСОК"));
        content = content.push(list);
        let state_text = if self.cached_script.trim().is_empty() {
            "Скрипт не вставлен".to_owned()
        } else if total == 0 {
            "В скрипте не найдены material box".to_owned()
        } else if self.mtl_materials.is_empty() {
            "Файл MTL не открыт".to_owned()
        } else {
            format!("Совпадений в MTL: {matched}")
        };
        content = content.push(text(state_text).size(theme::BODY_SIZE));
        if let Some((message, error)) = &self.status {
            content = content.push(status_text(message, *error));
        }
        content = content.push(page_button(
            "Скопировать результат",
            theme::Category::Secondary,
            true,
            Message::MaterialCopy,
        ));
        content.into()
    }
}

pub struct CornerPage {
    pub direction: usize,
    pub result: Option<String>,
    pub lines: Vec<ScriptLine>,
    pub result_lines: Vec<ScriptLine>,
    pub status: Option<(String, bool)>,
    pub azimuth: f64,
    pub elevation: f64,
    pub active_view: Option<StandardView>,
    pub material_colors: HashMap<String, iced::Color>,
    cached_script: String,
    cached_pair: Option<crate::corner::ConstantPair>,
}

impl Default for CornerPage {
    fn default() -> Self {
        Self {
            direction: 0,
            result: None,
            lines: Vec::new(),
            result_lines: Vec::new(),
            status: None,
            azimuth: 30.0f64.to_radians(),
            elevation: 25.0f64.to_radians(),
            active_view: None,
            material_colors: HashMap::new(),
            cached_script: String::new(),
            cached_pair: None,
        }
    }
}

impl CornerPage {
    pub fn set_material_colors(&mut self, colors: HashMap<String, iced::Color>) {
        self.material_colors = colors;
    }

    pub fn sync_script(&mut self, script: &str) {
        self.lines = crate::parser::parse_script(script);
        if self.cached_script != script {
            self.cached_script = script.to_owned();
            self.cached_pair = crate::corner::detect_constant_pair(script);
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let directions = ["Вверх", "Вниз", "Влево", "Вправо"];
        let dir_lower = ["вверх", "вниз", "влево", "вправо"];
        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(page_button(
            "Вставить из буфера",
            theme::Category::Primary,
            true,
            Message::CornerPaste,
        ));

        if let Some(pair) = &self.cached_pair {
            let free: Vec<&str> = ["X", "Y", "Z"]
                .iter()
                .filter(|axis| ***axis != pair.axis)
                .copied()
                .collect();
            content = content.push(
                text(format!(
                    "Постоянная ось: {} ({}..{})\nПлоскость сечения: {}{}",
                    pair.axis, pair.min_val, pair.max_val, free[0], free[1]
                ))
                .size(theme::BODY_SIZE),
            );
            content = content.push(
                text(format!("Направление: {}", directions[self.direction])).size(theme::BODY_SIZE),
            );
            content = content.push(
                row![
                    super::widgets::square_button("←", Some(Message::CornerDirection(-1))),
                    super::widgets::square_button("→", Some(Message::CornerDirection(1)))
                ]
                .spacing(theme::SPACE_XS),
            );
            content = content.push(page_button(
                "Создать угол",
                theme::Category::Secondary,
                true,
                Message::CornerCreate,
            ));
        } else if !self.cached_script.is_empty() {
            content = content.push(
                text("Модель не является псевдо-2D: ни одна пара координат не постоянна для всех элементов.")
                    .size(theme::BODY_SIZE)
                    .style(|theme| text::Style {
                        color: Some(theme::status_color(theme::Status::Error, theme::is_dark(theme))),
                    }),
            );
        }
        if let Some(result) = &self.result {
            let before = crate::model_check::count_model_planes(&self.cached_script);
            let after = crate::model_check::count_model_planes(result);
            content = content.push(
                text(format!(
                    "Направление: {}\nБыло элементов: {}\nСтало элементов: {}",
                    dir_lower[self.direction], before.total_objects, after.total_objects
                ))
                .size(theme::BODY_SIZE)
                .style(|theme| text::Style {
                    color: Some(theme::status_color(
                        theme::Status::Ok,
                        theme::is_dark(theme),
                    )),
                }),
            );
        }
        if !self.lines.is_empty() {
            let mut view_buttons = iced::widget::Column::new().spacing(theme::SPACE_XS).push(
                text("Стандартный вид")
                    .font(super::widgets::body_font(true))
                    .size(theme::SMALL_SIZE),
            );
            for standard_row in preview3d::STANDARD_VIEWS {
                let mut button_row = iced::widget::Row::new().spacing(theme::SPACE_XS);
                for (view, label) in standard_row {
                    button_row = button_row.push(page_button(
                        label,
                        theme::Category::Secondary,
                        false,
                        Message::CornerView(view),
                    ));
                }
                view_buttons = view_buttons.push(button_row);
            }
            let preview_lines = if self.result.is_some() {
                &self.result_lines
            } else {
                &self.lines
            };
            content = content.push(view_buttons).push(
                Preview3D::view(
                    preview_lines,
                    &self.material_colors,
                    self.azimuth,
                    self.elevation,
                )
                .map(|canvas_message| match canvas_message {
                    Canvas3DMessage::Rotated(azimuth, elevation) => {
                        Message::CornerRotated(azimuth, elevation)
                    }
                }),
            );
            if self.result.is_some() {
                content = content.push(page_button(
                    "Скопировать результат",
                    theme::Category::Secondary,
                    true,
                    Message::CornerCopy,
                ));
            }
        }
        if let Some((message, error)) = &self.status {
            content = content.push(status_text(message, *error));
        }
        content.into()
    }
}

pub struct AirCavitiesPage {
    pub log_text: String,
    pub name_mask: String,
    pub add_number: bool,
    pub add_lambda: bool,
    pub special_value: u8,
    pub status: Option<(String, bool)>,
    pub mtl_path: Option<PathBuf>,
    pub color_rgb: (u8, u8, u8),
    pub show_color_picker: bool,
    pub color_draft: [String; 3],
    pub color_hsv: (f32, f32, f32),
    pub color_error: Option<String>,
    pub specs: Vec<AirCavityMaterialSpec>,
    pub specs_error: Option<String>,
    cache_key: String,
}

impl Default for AirCavitiesPage {
    fn default() -> Self {
        Self {
            log_text: String::new(),
            name_mask: "Прослойка".to_owned(),
            add_number: true,
            add_lambda: false,
            special_value: 0,
            status: Some(("Лог не вставлен.".to_owned(), false)),
            mtl_path: None,
            color_rgb: (180, 220, 255),
            show_color_picker: false,
            color_draft: ["180".to_owned(), "220".to_owned(), "255".to_owned()],
            color_hsv: rgb_to_hsv((180, 220, 255)),
            color_error: None,
            specs: Vec::new(),
            specs_error: None,
            cache_key: String::new(),
        }
    }
}

impl AirCavitiesPage {
    pub fn open_color_picker(&mut self) {
        let (r, g, b) = self.color_rgb;
        self.color_draft = [r.to_string(), g.to_string(), b.to_string()];
        self.color_hsv = rgb_to_hsv(self.color_rgb);
        self.color_error = None;
        self.show_color_picker = true;
    }

    pub fn close_color_picker(&mut self) {
        self.show_color_picker = false;
        self.color_error = None;
    }

    pub fn set_color_text(&mut self, channel: usize, value: String) {
        if let Some(draft) = self.color_draft.get_mut(channel) {
            *draft = value;
        }
        if let Ok(color) = parse_color_draft(&self.color_draft) {
            self.color_hsv = rgb_to_hsv(color);
        }
        self.color_error = None;
    }

    pub fn set_color_hsv(&mut self, hue: f32, saturation: f32, value: f32) {
        self.color_hsv = (
            hue.rem_euclid(360.0),
            saturation.clamp(0.0, 1.0),
            value.clamp(0.0, 1.0),
        );
        let color = hsv_to_rgb(self.color_hsv.0, self.color_hsv.1, self.color_hsv.2);
        self.color_draft = [
            color.0.to_string(),
            color.1.to_string(),
            color.2.to_string(),
        ];
        self.color_error = None;
    }

    pub fn apply_color_draft(&mut self) -> bool {
        match parse_color_draft(&self.color_draft) {
            Ok(color) => {
                self.color_rgb = color;
                self.show_color_picker = false;
                self.color_error = None;
                true
            }
            Err(error) => {
                self.color_error = Some(error);
                false
            }
        }
    }

    pub fn sync_specs(&mut self) {
        let name_mask = self.effective_name_mask();
        let key = format!(
            "{}\u{1}{}\u{1}{:?}\u{1}{}",
            self.log_text, name_mask, self.color_rgb, self.special_value
        );
        if self.cache_key == key {
            return;
        }
        self.cache_key = key;
        if self.log_text.trim().is_empty() {
            self.specs.clear();
            self.specs_error = None;
            self.status = Some(("Лог не вставлен.".to_owned(), false));
            return;
        }
        match parse_air_cavities_info_log(&self.log_text).and_then(|cavities| {
            build_air_cavity_materials(&cavities, &name_mask, self.color_rgb, self.special_value)
        }) {
            Ok(specs) => {
                self.specs = specs;
                self.specs_error = None;
                let suffix = if self.mtl_path.is_some() {
                    " MTL выбран."
                } else {
                    " Выберите MTL файл для записи."
                };
                self.status = Some((
                    format!("Найдено прослоек: {}.{suffix}", self.specs.len()),
                    false,
                ));
            }
            Err(error) => {
                self.specs.clear();
                self.specs_error = Some(error);
                self.status = None;
            }
        }
    }

    pub fn effective_name_mask(&self) -> String {
        let base = self.name_mask.replace("{", "{{").replace("}", "}}");
        let mut parts = vec![base];
        if self.add_number {
            parts.push("{n:03d}".to_owned());
        }
        if self.add_lambda {
            parts.push("{lambda}".to_owned());
        }
        parts.join(" ")
    }

    pub fn view(&self) -> Element<'_, Message> {
        let hatch = ["-", "///", "\\\\\\", "|||", "===", "+++", "xxx"];
        let mtl_label = self
            .mtl_path
            .as_ref()
            .and_then(|path| path.file_name().and_then(|name| name.to_str()))
            .map(|value| value.to_owned())
            .unwrap_or_else(|| "Файл MTL не выбран".to_owned());
        let hatch_buttons: Vec<Element<'_, Message>> = hatch
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let selected = self.special_value == index as u8;
                let style_fn = theme::selectable_button_style(selected);
                let content = container(
                    text(*label)
                        .size(theme::BODY_SIZE)
                        .font(super::widgets::button_font())
                        .width(Length::Fill)
                        .align_x(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center);
                button(content)
                    .width(Length::Fixed(theme::NAV_HEIGHT))
                    .height(Length::Fixed(theme::NAV_HEIGHT))
                    .padding(0)
                    .style(move |theme: &Theme, status| style_fn(theme, status))
                    .on_press(Message::AirHatch(index as u8))
                    .into()
            })
            .collect();
        let cavity_count = self.specs.len();
        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(hero_metric(
            "НАЙДЕНО ПРОСЛОЕК",
            cavity_count.to_string(),
            if cavity_count == 0 {
                theme::Status::Muted
            } else {
                theme::Status::Ok
            },
        ));
        content = content.push(page_button(
            "Вставить HEAT 2 info log",
            theme::Category::Primary,
            true,
            Message::AirPaste,
        ));
        content = content.push(page_button(
            "Выбрать .MTL файл",
            theme::Category::Secondary,
            true,
            Message::AirOpen,
        ));
        content = content.push(meta_label(format!("MTL: {mtl_label}")));
        content = content.push(section_title("ИМЯ МАТЕРИАЛА"));
        content = content.push(
            iced::widget::text_input("Прослойка", &self.name_mask)
                .style(theme::text_input_style)
                .size(theme::BODY_SIZE)
                .padding([theme::SPACE_XS, theme::SPACE_SM])
                .on_input(Message::AirNameChanged),
        );
        content = content.push(
            column![
                row![
                    checkbox(self.add_number)
                        .style(theme::checkbox_style)
                        .size(theme::BODY_SIZE)
                        .on_toggle(Message::AirToggleNumber),
                    text("Добавить порядковый номер").size(theme::BODY_SIZE),
                ]
                .spacing(theme::SPACE_XS),
                row![
                    checkbox(self.add_lambda)
                        .style(theme::checkbox_style)
                        .size(theme::BODY_SIZE)
                        .on_toggle(Message::AirToggleLambda),
                    text("Добавить теплопроводность").size(theme::BODY_SIZE),
                ]
                .spacing(theme::SPACE_XS),
            ]
            .spacing(theme::SPACE_XS),
        );
        content = content.push(section_title("ЦВЕТ"));

        // The palette lives inside the app so opening it never blocks the UI thread.
        let (r, g, b) = (
            self.color_rgb.0 as f32 / 255.0,
            self.color_rgb.1 as f32 / 255.0,
            self.color_rgb.2 as f32 / 255.0,
        );
        let swatch = button(
            container(text(" "))
                .width(Length::Fixed(44.0))
                .height(Length::Fixed(44.0))
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center),
        )
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(44.0))
        .style(move |_theme: &Theme, _status| button::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgb(r, g, b))),
            text_color: iced::Color::TRANSPARENT,
            border: iced::Border {
                color: theme::line_visible(theme::is_dark(_theme)),
                width: 1.0,
                radius: theme::RADIUS_MD.into(),
            },
            shadow: Default::default(),
            snap: true,
        })
        .on_press(Message::AirPickColor);
        let rgb_text = format!(
            "rgb({}, {}, {})",
            self.color_rgb.0, self.color_rgb.1, self.color_rgb.2
        );
        content = content.push(
            row![swatch, text(rgb_text).size(theme::BODY_SIZE),]
                .spacing(theme::SPACE_MD)
                .align_y(iced::Alignment::Center),
        );

        content = content.push(section_title("ШТРИХОВКА"));
        content = content.push(row(hatch_buttons).spacing(theme::SPACE_XS));
        content = content.push(section_title("СПИСОК ПРОСЛОЕК"));

        if let Some(error) = &self.specs_error {
            content = content.push(status_text(error, true));
        } else if !self.specs.is_empty() {
            let items: Vec<Element<'_, Message>> = self
                .specs
                .iter()
                .map(|spec| {
                    container(
                        text(format!(
                            "{}: {}x{} мм, lambda {} -> {}",
                            spec.cavity.number,
                            crate::text::format_g(spec.cavity.b_mm, 6),
                            crate::text::format_g(spec.cavity.d_mm, 6),
                            crate::text::format_g(spec.cavity.lambda_value, 6),
                            spec.material.name,
                        ))
                        .size(theme::BODY_SIZE),
                    )
                    .padding([theme::SPACE_2XS, 0.0])
                    .into()
                })
                .collect();
            content = content.push(
                scrollable(iced::widget::Column::with_children(items)).height(Length::Fixed(140.0)),
            );
        }

        let apply_enabled = self.mtl_path.is_some() && !self.specs.is_empty();
        content = content.push(page_button_maybe(
            "Добавить/обновить материалы",
            theme::Category::Secondary,
            false,
            if apply_enabled {
                Some(Message::AirApply)
            } else {
                None
            },
        ));

        if let Some((message, _)) = &self.status {
            content =
                content.push(
                    text(message)
                        .size(theme::BODY_SIZE)
                        .style(|theme| text::Style {
                            color: Some(theme::ink(theme::is_dark(theme))),
                        }),
                );
        }
        content.into()
    }

    pub fn color_overlay(&self) -> Option<Element<'_, Message>> {
        if !self.show_color_picker {
            return None;
        }

        let (hue, saturation, value) = self.color_hsv;
        let (red, green, blue) = hsv_to_rgb(hue, saturation, value);
        let preview = container(text(""))
            .width(Length::Fixed(56.0))
            .height(Length::Fixed(48.0))
            .style(move |theme: &Theme| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb8(
                    red, green, blue,
                ))),
                border: iced::Border {
                    color: theme::line_visible(theme::is_dark(theme)),
                    width: 1.0,
                    radius: theme::RADIUS_MD.into(),
                },
                ..Default::default()
            });

        let mut card = column![
            page_title("ЦВЕТ МАТЕРИАЛА"),
            separator(),
            container(
                ColorPicker {
                    hue,
                    saturation,
                    value,
                }
                .view(),
            )
            .width(Length::Fill)
            .align_x(iced::Alignment::Center),
            row![
                preview,
                column![
                    text(format!("rgb({red}, {green}, {blue})")).size(theme::BODY_SIZE),
                    meta_label(format!("#{red:02X}{green:02X}{blue:02X}")),
                ]
                .spacing(theme::SPACE_2XS),
            ]
            .spacing(theme::SPACE_MD)
            .align_y(iced::Alignment::Center),
            meta_label("ТОЧНЫЕ ЗНАЧЕНИЯ RGB · 0–255"),
            row![
                rgb_input("R", &self.color_draft[0], Message::AirColorRedChanged),
                rgb_input("G", &self.color_draft[1], Message::AirColorGreenChanged),
                rgb_input("B", &self.color_draft[2], Message::AirColorBlueChanged),
            ]
            .spacing(theme::SPACE_SM),
        ]
        .spacing(theme::SPACE_SM);

        if let Some(error) = &self.color_error {
            card = card.push(status_text(error, true));
        }

        card = card.push(
            row![
                page_button(
                    "Отмена",
                    theme::Category::Ghost,
                    false,
                    Message::AirCloseColor,
                ),
                page_button(
                    "Применить",
                    theme::Category::Primary,
                    false,
                    Message::AirApplyColor,
                ),
            ]
            .spacing(theme::SPACE_SM),
        );

        Some(super::widgets::modal(card.into(), 300.0))
    }
}

pub fn copy_text(value: &str) -> Result<(), String> {
    clipboard::write_text(value)
}
pub fn reorder_script(script: &str, names: &[String]) -> String {
    sort_material_boxes_by_order(script, names)
}
pub fn normalize(name: &str) -> String {
    normalize_material_name(name)
}
pub fn sort_materials(names: &mut Vec<String>, materials: &HashMap<String, MtlMaterial>) {
    *names = sort_material_names_by_conductivity(names, materials);
}
pub fn parse_mtl(path: &std::path::Path) -> Result<Vec<MtlMaterial>, String> {
    parse_mtl_file(path, true).map_err(|error| error.to_string())
}
pub fn apply_air_cavity_upsert(
    path: &std::path::Path,
    page: &AirCavitiesPage,
) -> Result<crate::air_cavities::MtlUpsertResult, String> {
    if page.specs.is_empty() {
        return Err("Лог не вставлен или не удалось построить материалы.".to_owned());
    }
    upsert_air_cavity_materials(path, &page.specs).map_err(|error| error.to_string())
}

const CORNER_DIRECTIONS: [&str; 4] = ["up", "down", "left", "right"];

pub fn create_corner_script(script: &str, direction_index: usize) -> Option<String> {
    let direction = CORNER_DIRECTIONS.get(direction_index).copied()?;
    crate::corner::detect_constant_pair(script)?;
    Some(crate::corner::create_corner(script, direction))
}

#[cfg(test)]
mod color_tests {
    use super::{parse_color_draft, AirCavitiesPage};

    #[test]
    fn parses_valid_rgb_channels() {
        let channels = ["12".to_owned(), " 34 ".to_owned(), "255".to_owned()];
        assert_eq!(parse_color_draft(&channels), Ok((12, 34, 255)));
    }

    #[test]
    fn rejects_invalid_rgb_channels() {
        let too_large = ["256".to_owned(), "34".to_owned(), "56".to_owned()];
        let not_a_number = ["12".to_owned(), "blue".to_owned(), "56".to_owned()];

        assert!(parse_color_draft(&too_large).is_err());
        assert!(parse_color_draft(&not_a_number).is_err());
    }

    #[test]
    fn applying_a_valid_draft_updates_color_and_closes_palette() {
        let mut page = AirCavitiesPage {
            show_color_picker: true,
            color_draft: ["253".to_owned(), "186".to_owned(), "116".to_owned()],
            ..AirCavitiesPage::default()
        };

        assert!(page.apply_color_draft());
        assert_eq!(page.color_rgb, (253, 186, 116));
        assert!(!page.show_color_picker);
        assert!(page.color_error.is_none());
    }

    #[test]
    fn hsv_field_updates_exact_rgb_draft_across_the_full_gamut() {
        let mut page = AirCavitiesPage::default();
        page.set_color_hsv(120.0, 1.0, 1.0);

        assert_eq!(
            page.color_draft,
            ["0".to_owned(), "255".to_owned(), "0".to_owned()]
        );
        assert!(page.apply_color_draft());
        assert_eq!(page.color_rgb, (0, 255, 0));
    }
}
