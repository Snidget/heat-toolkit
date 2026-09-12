use std::collections::{BTreeSet, HashMap};

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Border, Color, Element, Length};

use crate::clipboard;
use crate::material_sort::{
    extract_material_entries, normalize_material_name, MaterialEntry, MtlMaterial,
};

use super::app::Message;
use super::theme;
use super::widgets::{
    hero_metric, page_button, page_button_maybe, selectable_button, selectable_data_button,
    separator, status_text,
};

const SCALE_WIDTH: f32 = 36.0;
const SCALE_WIDTH_PX: u32 = 36;
const ROW_HEIGHT_PX: u32 = 22;
const UI_ROW_HEIGHT: f32 = 28.0;
const LAMBDA_COLUMN_WIDTH: f32 = 84.0;

#[derive(Clone, Debug)]
pub struct ReportItem {
    pub name: String,
    pub lambda: Option<f64>,
    pub color: Color,
}

#[derive(Default)]
pub struct ReportPage {
    pub cached_script: String,
    pub entries: Vec<MaterialEntry>,
    pub items: Vec<ReportItem>,
    pub mtl_materials: HashMap<String, MtlMaterial>,
    pub selected_cells: BTreeSet<(usize, usize)>,
    pub selection_anchor: Option<(usize, usize)>,
    pub status: Option<(String, bool)>,
}

fn lambda_text(lambda: Option<f64>) -> String {
    lambda
        .map(|value| crate::text::format_g(value, 6))
        .unwrap_or_else(|| "—".to_owned())
}

fn compact_cell_label(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let mut compact: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    compact.push('…');
    compact
}

fn rgba_color(red: u8, green: u8, blue: u8) -> [u8; 4] {
    [red, green, blue, 255]
}

fn build_scale_rgba(items: &[ReportItem]) -> (u32, u32, Vec<u8>) {
    let height = (items.len() as u32 * ROW_HEIGHT_PX).max(1);
    let mut rgba = vec![255; (SCALE_WIDTH_PX * height * 4) as usize];

    for (row, item) in items.iter().enumerate() {
        let (red, green, blue, _) = (
            (item.color.r * 255.0).round() as u8,
            (item.color.g * 255.0).round() as u8,
            (item.color.b * 255.0).round() as u8,
            (item.color.a * 255.0).round() as u8,
        );
        for local_y in 0..ROW_HEIGHT_PX {
            let y = row as u32 * ROW_HEIGHT_PX + local_y;
            for x in 0..SCALE_WIDTH_PX {
                let border = x == 0
                    || x == SCALE_WIDTH_PX - 1
                    || local_y == 0
                    || local_y == ROW_HEIGHT_PX - 1;
                let pixel = if border {
                    [0, 0, 0, 255]
                } else {
                    rgba_color(red, green, blue)
                };
                let offset = ((y * SCALE_WIDTH_PX + x) * 4) as usize;
                rgba[offset..offset + 4].copy_from_slice(&pixel);
            }
        }
    }

    (SCALE_WIDTH_PX, height, rgba)
}

impl ReportPage {
    pub fn sync_script(&mut self, script: &str) {
        if self.cached_script == script {
            return;
        }
        self.cached_script = script.to_owned();
        self.entries = extract_material_entries(script);
        self.refresh_items();
        self.selected_cells
            .retain(|(row, column)| *row < self.entries.len() && *column < 2);
    }

    fn build_items(&self) -> Vec<ReportItem> {
        self.entries
            .iter()
            .map(|entry| {
                let key = normalize_material_name(&entry.name);
                let material = self.mtl_materials.get(&key);
                let color = material
                    .filter(|m| m.rgb_r != 0 || m.rgb_g != 0 || m.rgb_b != 0)
                    .map(|m| {
                        Color::from_rgb(
                            m.rgb_r as f32 / 255.0,
                            m.rgb_g as f32 / 255.0,
                            m.rgb_b as f32 / 255.0,
                        )
                    })
                    .unwrap_or(theme::MUTED);
                let lambda = material.map(|m| m.thermal_x);
                ReportItem {
                    name: entry.name.clone(),
                    lambda,
                    color,
                }
            })
            .collect()
    }

    /// Пересобрать элементы шкалы (например, после загрузки MTL).
    pub fn refresh_items(&mut self) {
        self.items = self.build_items();
        self.selected_cells
            .retain(|(row, column)| *row < self.items.len() && *column < 2);
    }

    /// Цвета материалов MTL для 3D-страниц (аналог egui mod.rs material_color_map).
    pub fn material_color_map(&self) -> HashMap<String, Color> {
        self.mtl_materials
            .iter()
            .map(|(key, material)| {
                (
                    key.clone(),
                    Color::from_rgb(
                        material.rgb_r as f32 / 255.0,
                        material.rgb_g as f32 / 255.0,
                        material.rgb_b as f32 / 255.0,
                    ),
                )
            })
            .collect()
    }

    pub fn select_cell(&mut self, cell: (usize, usize), shift: bool, command: bool) {
        if shift {
            let anchor = self.selection_anchor.unwrap_or(cell);
            self.selected_cells.clear();
            for row in anchor.0.min(cell.0)..=anchor.0.max(cell.0) {
                for column in anchor.1.min(cell.1)..=anchor.1.max(cell.1) {
                    self.selected_cells.insert((row, column));
                }
            }
        } else if command {
            if !self.selected_cells.remove(&cell) {
                self.selected_cells.insert(cell);
            }
            self.selection_anchor = Some(cell);
        } else {
            self.selected_cells.clear();
            self.selected_cells.insert(cell);
            self.selection_anchor = Some(cell);
        }
    }

    fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = Some((message.into(), is_error));
    }

    pub fn copy_selected(&mut self) {
        let Some(text) = selected_table_text(&self.items, &self.selected_cells) else {
            self.set_status("Сначала выделите ячейки таблицы.", true);
            return;
        };
        match clipboard::write_text(&text) {
            Ok(()) => self.set_status(
                "Выделенное скопировано. Вставьте (Ctrl+V) в Word — получится таблица.",
                false,
            ),
            Err(error) => {
                self.set_status(format!("Не удалось записать в буфер обмена: {error}"), true)
            }
        }
    }

    pub fn copy_all(&mut self) {
        match clipboard::write_text(&full_table_text(&self.items)) {
            Ok(()) => self.set_status("Таблица скопирована в буфер обмена.", false),
            Err(error) => {
                self.set_status(format!("Не удалось записать в буфер обмена: {error}"), true)
            }
        }
    }

    pub fn copy_scale_image(&mut self) {
        let (width, height, rgba) = build_scale_rgba(&self.items);
        match clipboard::write_image_rgba(width, height, &rgba) {
            Ok(()) => self.set_status("Шкала скопирована как изображение.", false),
            Err(error) => self.set_status(format!("Не удалось скопировать шкалу: {error}"), true),
        }
    }

    pub fn save_scale_png(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("material_scale.png")
            .save_file()
        else {
            return;
        };
        let (width, height, rgba) = build_scale_rgba(&self.items);
        match image::save_buffer_with_format(
            &path,
            &rgba,
            width,
            height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        ) {
            Ok(()) => self.set_status(format!("Шкала сохранена: {}", path.display()), false),
            Err(error) => self.set_status(format!("Не удалось сохранить PNG: {error}"), true),
        }
    }

    pub fn view(&self, modifiers: iced::keyboard::Modifiers) -> Element<'_, Message> {
        let items = &self.items;
        let matched = items
            .iter()
            .filter(|item| {
                item.lambda.is_some()
                    && self
                        .mtl_materials
                        .contains_key(&normalize_material_name(&item.name))
            })
            .count();
        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(hero_metric(
            "СОВПАДЕНИЙ В MTL",
            format!("{matched}/{}", items.len()),
            if matched == items.len() && !items.is_empty() {
                theme::Status::Ok
            } else if matched == 0 {
                theme::Status::Muted
            } else {
                theme::Status::Warn
            },
        ));
        content = content.push(page_button(
            "Вставить скрипт",
            theme::Category::Primary,
            true,
            Message::ReportPaste,
        ));
        content = content.push(page_button(
            "Вставить mtl файл",
            theme::Category::Secondary,
            true,
            Message::ReportOpenMtl,
        ));

        if let Some((message, error)) = &self.status {
            content = content.push(status_text(message, *error));
        }
        if self.cached_script.trim().is_empty() {
            content = content.push(text("Скрипт не вставлен.").size(theme::BODY_SIZE));
        } else if items.is_empty() {
            content =
                content.push(text("В скрипте не найдены material box.").size(theme::BODY_SIZE));
        } else {
            let summary = if self.mtl_materials.is_empty() {
                format!("Материалов: {}. Файл MTL не загружен.", items.len())
            } else {
                format!("Материалов: {}, совпадений в MTL: {matched}.", items.len())
            };
            content = content.push(text(summary).size(theme::BODY_SIZE));

            let scale_strip = column(
                std::iter::once(
                    container(text(" "))
                        .width(Length::Fixed(SCALE_WIDTH))
                        .height(Length::Fixed(UI_ROW_HEIGHT))
                        .into(),
                )
                .chain(items.iter().map(|item| {
                    container(text(" "))
                        .width(Length::Fixed(SCALE_WIDTH))
                        .height(Length::Fixed(UI_ROW_HEIGHT))
                        .style(move |theme: &iced::Theme| container::Style {
                            background: Some(item.color.into()),
                            border: Border {
                                color: theme::ink(theme::is_dark(theme)),
                                width: 1.0,
                                radius: 0.0.into(),
                            },
                            ..Default::default()
                        })
                        .into()
                })),
            )
            .spacing(0);
            let scale = button(container(scale_strip).padding(1))
                .padding(0)
                .style(theme::data_button_style)
                .on_press(Message::ReportCopyScale);

            let header = row![
                text("Материал")
                    .size(theme::SMALL_SIZE)
                    .font(super::widgets::body_font(true))
                    .width(Length::Fill),
                text("λ, Вт/(м·К)")
                    .size(theme::SMALL_SIZE)
                    .font(super::widgets::body_font(true))
                    .width(Length::Fixed(LAMBDA_COLUMN_WIDTH))
            ]
            .height(Length::Fixed(UI_ROW_HEIGHT))
            .align_y(iced::Alignment::Center);
            let rows = items.iter().enumerate().map(|(row_index, item)| {
                row![
                        selectable_button(
                            compact_cell_label(&item.name, 20),
                            self.selected_cells.contains(&(row_index, 0)),
                            Message::ReportSelect(
                                row_index,
                                0,
                                modifiers.shift(),
                                modifiers.command(),
                            ),
                            UI_ROW_HEIGHT,
                            Length::Fill,
                        ),
                        selectable_data_button(
                            lambda_text(item.lambda),
                            self.selected_cells.contains(&(row_index, 1)),
                            Message::ReportSelect(
                                row_index,
                                1,
                                modifiers.shift(),
                                modifiers.command(),
                            ),
                            UI_ROW_HEIGHT,
                            Length::Fixed(LAMBDA_COLUMN_WIDTH),
                        ),
                    ]
                    .spacing(theme::SPACE_XS)
                    .height(Length::Fixed(UI_ROW_HEIGHT))
            });
            let table =
                column(std::iter::once(header.into()).chain(rows.map(Into::into))).spacing(0);

            // One scroll surface keeps the color scale and table rows aligned.
            content = content.push(
                scrollable(row![scale, table])
                    .height(Length::Fixed(200.0))
                    .width(Length::Fill),
            );
        }

        content = content.push(separator());
        content = content.push(page_button_maybe(
            "Сохранить шкалу PNG",
            theme::Category::Secondary,
            true,
            if items.is_empty() {
                None
            } else {
                Some(Message::ReportSavePng)
            },
        ));
        content = content.push(page_button_maybe(
            "Копировать выделенное",
            theme::Category::Secondary,
            true,
            if self.selected_cells.is_empty() {
                None
            } else {
                Some(Message::ReportCopySelected)
            },
        ));
        content = content.push(page_button_maybe(
            "Копировать таблицу",
            theme::Category::Secondary,
            true,
            if items.is_empty() {
                None
            } else {
                Some(Message::ReportCopyAll)
            },
        ));
        content.into()
    }
}

pub fn selected_table_text(
    items: &[ReportItem],
    selected: &BTreeSet<(usize, usize)>,
) -> Option<String> {
    let rows: BTreeSet<usize> = selected.iter().map(|(row, _)| *row).collect();
    let columns: BTreeSet<usize> = selected.iter().map(|(_, column)| *column).collect();
    if rows.is_empty() || columns.is_empty() {
        return None;
    }
    let lines = rows
        .into_iter()
        .filter_map(|row| items.get(row))
        .map(|item| {
            columns
                .iter()
                .map(|column| match column {
                    0 => item.name.clone(),
                    1 => lambda_text(item.lambda),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub fn full_table_text(items: &[ReportItem]) -> String {
    let mut text = String::from("Материал\tλ, Вт/(м·К)\n");
    for item in items {
        text.push_str(&item.name);
        text.push('\t');
        text.push_str(&lambda_text(item.lambda));
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_items() -> Vec<ReportItem> {
        vec![
            ReportItem {
                name: "Brick".to_owned(),
                lambda: Some(0.72),
                color: Color::from_rgb(10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0),
            },
            ReportItem {
                name: "Unknown".to_owned(),
                lambda: None,
                color: Color::from_rgb(40.0 / 255.0, 50.0 / 255.0, 60.0 / 255.0),
            },
        ]
    }

    #[test]
    fn scale_image_has_expected_dimensions_and_border() {
        let (width, height, rgba) = build_scale_rgba(&sample_items());
        assert_eq!((width, height), (36, 44));
        assert_eq!(&rgba[0..4], &[0, 0, 0, 255]);
        let interior = ((width + 1) * 4) as usize;
        assert_eq!(&rgba[interior..interior + 4], &[10, 20, 30, 255]);
    }

    #[test]
    fn selected_table_copy_preserves_selected_rectangle() {
        let selected = BTreeSet::from([(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(
            selected_table_text(&sample_items(), &selected).as_deref(),
            Some("Brick\t0.72\nUnknown\t—")
        );
    }

    #[test]
    fn full_table_includes_header_and_dash() {
        let text = full_table_text(&sample_items());
        assert!(text.starts_with("Материал\tλ, Вт/(м·К)\n"));
        assert!(text.contains("Unknown\t—\n"));
    }

    #[test]
    fn compact_cell_label_preserves_short_values_and_marks_truncation() {
        assert_eq!(compact_cell_label("Кирпич", 8), "Кирпич");
        assert_eq!(compact_cell_label("Стеклопакет", 8), "Стеклоп…");
    }
}
