use std::time::{Duration, Instant};

use iced::widget::{
    button, checkbox, column, container, row, scrollable, text, text_input, Column,
};
use iced::{Element, Length, Theme};

use crate::clipboard;
use crate::model_check::{
    analyze_simplify, count_model_planes, detect_internal_cavities, format_cavity_check,
    format_plane_usage, CavityCheckResult, MergeProposal, PlaneUsage, SimplifyAnalysis,
};
use crate::parser::{parse_script, serialize_script};
use crate::transforms::{apply_transform, transform_enable_flags};
use crate::turner2d::{parse_2d_script, transform_2d_script};

use super::app::Message;
use super::preview2d::{Preview2D, Projection};
use super::theme;
use super::widgets::{hero_metric, page_button, page_button_maybe, section_title, status_text};

const DEBOUNCE_MS: u64 = 300;
const TURNER_PASTE_LABEL: &str = "Вставить данные из буфера обмена";
const TURNER_MIRROR_XY_X_LABEL: &str = "Отражение XY по X";
const TURNER_MIRROR_XY_Y_LABEL: &str = "Отражение XY по Y";
const TURNER_MIRROR_XZ_X_LABEL: &str = "Отражение XZ по X";
const TURNER_MIRROR_XZ_Z_LABEL: &str = "Отражение XZ по Z";
const TURNER_COPY_LABEL: &str = "Копировать данные в буфер обмена";

#[derive(Clone, Debug)]
pub struct CheckAnalysisRequest {
    generation: u64,
    script: String,
    tolerance: f64,
    max_change: f64,
    include_cavity: bool,
}

#[derive(Clone, Debug)]
pub struct CheckAnalysisResult {
    generation: u64,
    cavity: Option<CavityCheckResult>,
    analysis: SimplifyAnalysis,
}

pub fn run_check_analysis(request: CheckAnalysisRequest) -> CheckAnalysisResult {
    let cavity = request
        .include_cavity
        .then(|| detect_internal_cavities(&request.script, crate::model_check::CAVITY_CELL_LIMIT));
    let analysis = analyze_simplify(&request.script, request.tolerance, request.max_change);
    CheckAnalysisResult {
        generation: request.generation,
        cavity,
        analysis,
    }
}

pub struct CheckPage {
    pub tolerance: String,
    pub max_change: String,
    pub status: Option<(String, bool)>,
    pub usage: Option<PlaneUsage>,
    pub cavity: Option<CavityCheckResult>,
    pub analysis: Option<SimplifyAnalysis>,
    pub proposals: Vec<MergeProposal>,
    pub checked: Vec<bool>,
    pub collapsed: Vec<bool>,
    pub simplified: Option<String>,
    pub cached_script: String,
    analysis_pending_since: Option<Instant>,
    generation: u64,
}

impl Default for CheckPage {
    fn default() -> Self {
        Self {
            tolerance: "1.0".to_owned(),
            max_change: "5.0".to_owned(),
            status: None,
            usage: None,
            cavity: None,
            analysis: None,
            proposals: Vec::new(),
            checked: Vec::new(),
            collapsed: Vec::new(),
            simplified: None,
            cached_script: String::new(),
            analysis_pending_since: None,
            generation: 0,
        }
    }
}

impl CheckPage {
    pub fn sync_script(&mut self, script: &str) -> Option<CheckAnalysisRequest> {
        if self.cached_script == script {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.cached_script = script.to_owned();
        self.usage = Some(count_model_planes(script));
        self.analysis = None;
        self.cavity = None;
        self.proposals.clear();
        self.checked.clear();
        self.collapsed.clear();
        self.simplified = None;
        if script.trim().is_empty() {
            self.analysis_pending_since = None;
            return None;
        }
        self.analysis_pending_since = None;
        self.analysis_request(true)
    }

    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.cached_script.clear();
        self.usage = None;
        self.analysis = None;
        self.cavity = None;
        self.proposals.clear();
        self.checked.clear();
        self.collapsed.clear();
        self.simplified = None;
        self.analysis_pending_since = None;
    }

    pub fn mark_pending(&mut self) {
        if !self.cached_script.trim().is_empty() {
            self.generation = self.generation.wrapping_add(1);
            self.analysis = None;
            self.proposals.clear();
            self.checked.clear();
            self.collapsed.clear();
            self.simplified = None;
            self.analysis_pending_since = Some(Instant::now());
        }
    }

    pub fn invalidate_for_invalid_input(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.analysis = None;
        self.proposals.clear();
        self.checked.clear();
        self.collapsed.clear();
        self.simplified = None;
        self.analysis_pending_since = None;
    }

    pub fn is_pending(&self) -> bool {
        self.analysis_pending_since.is_some()
    }

    pub fn tick(&mut self) -> Option<CheckAnalysisRequest> {
        if let Some(started) = self.analysis_pending_since {
            if started.elapsed() >= Duration::from_millis(DEBOUNCE_MS) {
                self.analysis_pending_since = None;
                return self.analysis_request(false);
            }
        }
        None
    }

    fn analysis_request(&self, include_cavity: bool) -> Option<CheckAnalysisRequest> {
        let Ok(tolerance) = self.tolerance.trim().parse::<f64>() else {
            return None;
        };
        let Ok(max_change) = self.max_change.trim().parse::<f64>() else {
            return None;
        };
        if !tolerance.is_finite() || !max_change.is_finite() {
            return None;
        }
        if !(0.1..=100.0).contains(&tolerance) || !(0.1..=50.0).contains(&max_change) {
            return None;
        }
        Some(CheckAnalysisRequest {
            generation: self.generation,
            script: self.cached_script.clone(),
            tolerance,
            max_change,
            include_cavity,
        })
    }

    pub fn apply_analysis(&mut self, result: CheckAnalysisResult) -> bool {
        if result.generation != self.generation {
            return false;
        }
        if let Some(cavity) = result.cavity {
            self.cavity = Some(cavity);
        }
        self.analysis = Some(result.analysis);
        self.proposals = self
            .analysis
            .as_ref()
            .map(|analysis| analysis.proposals.clone())
            .unwrap_or_default();
        self.checked = vec![true; self.proposals.len()];
        self.collapsed = vec![false; self.proposals.len()];
        true
    }

    fn colored(content: String, status: theme::Status) -> Element<'static, Message> {
        text(content)
            .size(theme::BODY_SIZE)
            .style(move |theme| text::Style {
                color: Some(theme::status_color(status, theme::is_dark(theme))),
            })
            .into()
    }

    fn colored_mono(content: String, status: theme::Status) -> Element<'static, Message> {
        text(content)
            .size(theme::BODY_SIZE)
            .font(super::widgets::mono_font())
            .style(move |theme| text::Style {
                color: Some(theme::status_color(status, theme::is_dark(theme))),
            })
            .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(page_button(
            "Вставить из буфера",
            theme::Category::Primary,
            true,
            Message::CheckPaste,
        ));

        content = content.push(section_title("Использование плоскостей"));
        let usage = self
            .usage
            .as_ref()
            .cloned()
            .unwrap_or_else(|| count_model_planes(""));
        let usage_text = format_plane_usage(&usage);
        if usage.any_exceeded() {
            content = content.push(Self::colored(usage_text, theme::Status::Error));
        } else {
            content = content.push(text(usage_text).size(theme::BODY_SIZE));
        }

        content = content.push(section_title("Внутренние пустоты"));
        match &self.cavity {
            Some(cavity) => {
                let status = if cavity.cavities_found() {
                    theme::Status::Error
                } else if cavity.skipped {
                    theme::Status::Warn
                } else {
                    theme::Status::Ok
                };
                content = content.push(Self::colored(format_cavity_check(cavity), status));
            }
            None => {
                content = content.push(
                    text("Вставьте скрипт для проверки внутренних пустот.").size(theme::BODY_SIZE),
                )
            }
        }

        content = content.push(section_title("Упрощение модели"));
        content = content.push(
            row![
                text("Порог схождения (мм):").size(theme::BODY_SIZE),
                text_input("1.0", &self.tolerance)
                    .on_input(Message::CheckTolerance)
                    .style(theme::text_input_style)
                    .size(theme::BODY_SIZE)
                    .padding([theme::SPACE_XS, theme::SPACE_SM])
                    .width(Length::Fixed(72.0)),
            ]
            .spacing(theme::SPACE_XS),
        );
        content = content.push(
            row![
                text("Макс. изменение (%):").size(theme::BODY_SIZE),
                text_input("5.0", &self.max_change)
                    .on_input(Message::CheckMaxChange)
                    .style(theme::text_input_style)
                    .size(theme::BODY_SIZE)
                    .padding([theme::SPACE_XS, theme::SPACE_SM])
                    .width(Length::Fixed(72.0)),
            ]
            .spacing(theme::SPACE_XS),
        );

        if let Some(analysis) = &self.analysis {
            if let Some(after) = &analysis.after_planes {
                let before = &analysis.before_planes;
                let dx = before.x as i32 - after.x as i32;
                let dy = before.y as i32 - after.y as i32;
                let dz = before.z as i32 - after.z as i32;
                if dx > 0 || dy > 0 || dz > 0 {
                    let total = dx + dy + dz;
                    content = content.push(hero_metric(
                        "СОКРАЩЕНИЕ ПЛОСКОСТЕЙ",
                        format!("{total} пл."),
                        theme::Status::Ok,
                    ));
                    content = content.push(Self::colored_mono(
                        format!(
                            "X: {} → {}  ({:+})\nY: {} → {}  ({:+})\nZ: {} → {}  ({:+})",
                            before.x, after.x, dx, before.y, after.y, dy, before.z, after.z, dz,
                        ),
                        theme::Status::Ok,
                    ));
                } else {
                    content = content.push(Self::colored(
                        "Нет возможностей для упрощения".to_owned(),
                        theme::Status::Muted,
                    ));
                }
            }
        }

        if !self.proposals.is_empty() {
            content = content.push(
                scrollable(Column::with_children(
                    self.proposals
                        .iter()
                        .enumerate()
                        .map(|(index, proposal)| {
                            let checked = self.checked.get(index).copied().unwrap_or(false);
                            let collapsed = self.collapsed.get(index).copied().unwrap_or(false);
                            let has_children = !proposal.changes.is_empty();
                            let header_text = format!(
                                "{}: {} → {}  (зазор {:.1} мм, {} эл.)",
                                proposal.axis,
                                proposal.coord_from,
                                proposal.coord_to,
                                proposal.gap * 1000.0,
                                proposal.affected_count(),
                            );
                            let toggle_label = if collapsed { "[+]" } else { "[-]" };
                            let toggle: Element<'_, Message> = if has_children {
                                super::widgets::square_button(
                                    toggle_label,
                                    Some(Message::CheckProposalCollapse(index)),
                                )
                            } else {
                                text("  ").into()
                            };
                            let header: Element<'_, Message> = row![
                                checkbox(checked)
                                    .style(theme::checkbox_style)
                                    .size(theme::BODY_SIZE)
                                    .on_toggle(move |_| { Message::CheckProposalToggle(index) }),
                                toggle,
                                text(header_text).size(theme::BODY_SIZE),
                            ]
                            .spacing(theme::SPACE_XS)
                            .into();
                            if !collapsed && has_children {
                                let mut children = Column::with_children(vec![header]);
                                for change in &proposal.changes {
                                    let name = if change.material.is_empty() {
                                        "(без материала)".to_owned()
                                    } else {
                                        change.material.clone()
                                    };
                                    children = children.push(
                                        container(
                                            text(format!(
                                                "{}: {} → {}",
                                                name, change.old_val, change.new_val
                                            ))
                                            .size(theme::BODY_SIZE),
                                        )
                                        .padding([0.0, theme::SPACE_XL]),
                                    );
                                }
                                children.into()
                            } else {
                                header
                            }
                        })
                        .collect::<Vec<_>>(),
                ))
                .height(Length::Fixed(140.0)),
            );
            content = content.push(
                column![
                    page_button_maybe(
                        "Упростить выделенное",
                        theme::Category::Secondary,
                        false,
                        if self.proposals.is_empty() {
                            None
                        } else {
                            Some(Message::CheckApplySelected)
                        },
                    ),
                    page_button_maybe(
                        "Упростить всё",
                        theme::Category::Primary,
                        false,
                        if self.proposals.is_empty() {
                            None
                        } else {
                            Some(Message::CheckSimplifyAll)
                        },
                    ),
                ]
                .spacing(theme::PAGE_SPACING),
            );
        } else {
            content = content.push(
                column![
                    page_button_maybe(
                        "Упростить выделенное",
                        theme::Category::Secondary,
                        false,
                        None,
                    ),
                    page_button_maybe("Упростить всё", theme::Category::Primary, false, None,),
                ]
                .spacing(theme::PAGE_SPACING),
            );
        }

        if let Some(result) = &self.simplified {
            let before = count_model_planes(&self.cached_script);
            let after = count_model_planes(result);
            content = content.push(Self::colored_mono(
                format!(
                    "Готово. Плоскости:\nX: {} → {}\nY: {} → {}\nZ: {} → {}",
                    before.x, after.x, before.y, after.y, before.z, after.z,
                ),
                theme::Status::Ok,
            ));
            content = content.push(page_button(
                "Скопировать результат",
                theme::Category::Secondary,
                true,
                Message::CheckCopy,
            ));
        }

        if let Some((message, error)) = &self.status {
            content = content.push(status_text(message, *error));
        }

        content.into()
    }
}

pub struct TurnerPage {
    pub projection: Projection,
    pub show_instruction: bool,
    pub status: Option<(String, bool)>,
    pub preview: Preview2D,
}

impl Default for TurnerPage {
    fn default() -> Self {
        Self {
            projection: Projection::XY,
            show_instruction: false,
            status: None,
            preview: Preview2D::default(),
        }
    }
}

impl TurnerPage {
    pub fn sync(&mut self, script: &str) {
        self.preview.set_segments(&parse_script(script));
    }

    pub fn view(&self) -> Element<'_, Message> {
        let preview: Element<'_, Message> =
            container(self.preview.view().map(|_| Message::CanvasHover))
                .width(Length::Fill)
                .align_x(iced::Alignment::Center)
                .into();
        let element_count = self.preview.segment_count();
        let content = column![
            preview,
            hero_metric(
                "ЭЛЕМЕНТОВ В СКРИПТЕ",
                element_count.to_string(),
                if element_count == 0 {
                    theme::Status::Muted
                } else {
                    theme::Status::Ok
                },
            ),
            row![
                text("Проекция:").size(theme::BODY_SIZE),
                iced::widget::pick_list(
                    vec![Projection::XY, Projection::XZ],
                    Some(self.projection),
                    Message::TurnerProjection,
                )
                .style(theme::pick_list_style)
                .menu_style(theme::pick_list_menu_style)
                .text_size(theme::BODY_SIZE)
                .width(Length::Fixed(96.0)),
                page_button(
                    "Инструкция",
                    theme::Category::Ghost,
                    false,
                    Message::TurnerInstruction,
                ),
            ]
            .spacing(theme::PAGE_SPACING),
            page_button(
                TURNER_PASTE_LABEL,
                theme::Category::Primary,
                true,
                Message::TurnerPaste,
            ),
            // Rotations: counter-clockwise first, clockwise second (2 cols).
            row![
                rotation_button(
                    "Повернуть влево",
                    Message::TurnerTransform("rotate_counterclockwise")
                ),
                rotation_button(
                    "Повернуть вправо",
                    Message::TurnerTransform("rotate_clockwise")
                ),
            ]
            .spacing(theme::PAGE_SPACING),
            // Plane swap: full width.
            page_button(
                "Смена XY ↔ XZ",
                theme::Category::Secondary,
                false,
                Message::TurnerTransform("swap_xy_xz"),
            ),
            // Mirrors: 2 columns.
            row![
                page_button(
                    TURNER_MIRROR_XY_X_LABEL,
                    theme::Category::Secondary,
                    false,
                    Message::TurnerTransform("mirror_xy_x"),
                ),
                page_button(
                    TURNER_MIRROR_XY_Y_LABEL,
                    theme::Category::Secondary,
                    false,
                    Message::TurnerTransform("mirror_xy_y"),
                ),
            ]
            .spacing(theme::PAGE_SPACING),
            row![
                page_button(
                    TURNER_MIRROR_XZ_X_LABEL,
                    theme::Category::Secondary,
                    false,
                    Message::TurnerTransform("mirror_xz_x"),
                ),
                page_button(
                    TURNER_MIRROR_XZ_Z_LABEL,
                    theme::Category::Secondary,
                    false,
                    Message::TurnerTransform("mirror_xz_z"),
                ),
            ]
            .spacing(theme::PAGE_SPACING),
            page_button(
                TURNER_COPY_LABEL,
                theme::Category::Secondary,
                true,
                Message::TurnerCopy,
            ),
        ]
        .spacing(theme::PAGE_SPACING);
        if let Some((message, error)) = &self.status {
            return column![content, status_text(message, *error)]
                .spacing(theme::PAGE_SPACING)
                .into();
        }
        content.into()
    }

    pub fn instruction_overlay(&self) -> Option<Element<'static, Message>> {
        if !self.show_instruction {
            return None;
        }
        let steps: &[&str] = &[
            "1. В окне Pre-processor открыть Script",
            "2. Нажать Import [pre-prossesor => script]",
            "3. Кликнуть мышкой на текст",
            "4. Выделить весь скрипт нажатием Ctrl+A и скопировать",
            "5. В программе нажать «Вставить данные из буфера обмена»",
            "6. Выполнить необходимые преобразования",
            "7. Нажать «Копировать данные в буфер обмена»",
            "8. В HEAT3 удалить старый скрипт и вставить новый",
            "9. Нажать Run [script => pre-processor]",
            "10. Отказаться от изменения масштаба",
        ];
        Some(instruction_modal(
            "ИНСТРУКЦИЯ",
            steps,
            Message::TurnerInstruction,
        ))
    }
}

/// Modal with a step list + close button, shared by Turner/Turner2D.
fn instruction_modal<'a>(
    title: &'a str,
    steps: &[&'a str],
    close: Message,
) -> Element<'a, Message> {
    use super::widgets::{meta_label, page_button, page_title};
    let mut body = column![page_title(title), meta_label("ШАГИ"),].spacing(theme::PAGE_SPACING);
    for step in steps {
        body = body.push(
            text(*step)
                .size(theme::BODY_SIZE)
                .style(|theme| text::Style {
                    color: Some(theme::ink(theme::is_dark(theme))),
                }),
        );
    }
    body = body.push(page_button(
        "Закрыть",
        theme::Category::Secondary,
        true,
        close,
    ));
    let content: Element<'a, Message> = column![body].into();
    super::widgets::modal(content, 300.0)
}

#[derive(Default)]
pub struct Turner2DPage {
    pub show_instruction: bool,
    pub status: Option<(String, bool)>,
    pub preview: Preview2D,
}

impl Turner2DPage {
    pub fn sync(&mut self, script: &str) {
        let rects = parse_2d_script(script)
            .into_iter()
            .filter_map(|line| line.rect)
            .collect::<Vec<_>>();
        self.preview.set_rects(&rects);
    }

    pub fn view(&self) -> Element<'_, Message> {
        let preview: Element<'_, Message> =
            container(self.preview.view().map(|_| Message::CanvasHover))
                .width(Length::Fill)
                .align_x(iced::Alignment::Center)
                .into();
        let element_count = self.preview.rects.len();
        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(preview);
        content = content.push(hero_metric(
            "ПРЯМОУГОЛЬНИКОВ",
            element_count.to_string(),
            if element_count == 0 {
                theme::Status::Muted
            } else {
                theme::Status::Ok
            },
        ));
        content = content.push(page_button(
            "Инструкция",
            theme::Category::Ghost,
            false,
            Message::Turner2DInstruction,
        ));
        content = content.push(page_button(
            "Вставить из буфера",
            theme::Category::Primary,
            true,
            Message::Turner2DPaste,
        ));
        content = content.push(
            row![
                rotation_button(
                    "Повернуть влево",
                    Message::Turner2DTransform("rotate_counterclockwise"),
                ),
                rotation_button(
                    "Повернуть вправо",
                    Message::Turner2DTransform("rotate_clockwise"),
                ),
            ]
            .spacing(theme::PAGE_SPACING),
        );
        content = content.push(
            row![
                page_button(
                    "Отражение по X",
                    theme::Category::Secondary,
                    false,
                    Message::Turner2DTransform("mirror_x"),
                ),
                page_button(
                    "Отражение по Y",
                    theme::Category::Secondary,
                    false,
                    Message::Turner2DTransform("mirror_y"),
                ),
            ]
            .spacing(theme::PAGE_SPACING),
        );
        content = content.push(page_button(
            "Скопировать результат",
            theme::Category::Secondary,
            true,
            Message::Turner2DCopy,
        ));
        if let Some((message, error)) = &self.status {
            content = content.push(status_text(message, *error));
        }
        content.into()
    }

    pub fn instruction_overlay(&self) -> Option<Element<'static, Message>> {
        if !self.show_instruction {
            return None;
        }
        let steps: &[&str] = &[
            "1. Скопируйте 2D-скрипт с прямоугольниками r x1 y1 x2 y2 material.",
            "2. Нажмите «Вставить из буфера».",
            "3. Выполните нужные повороты или отражения.",
            "4. Нажмите «Скопировать результат» и вставьте результат обратно.",
        ];
        Some(instruction_modal(
            "ИНСТРУКЦИЯ 2D",
            steps,
            Message::Turner2DInstruction,
        ))
    }
}

pub fn transform_script(script: &str, name: &str) -> Option<String> {
    let mut lines = parse_script(script);
    if !lines.iter().any(|line| line.segment.is_some()) {
        return None;
    }
    for line in &mut lines {
        if let Some(segment) = line.segment {
            if let Some(new_segment) = apply_transform(name, &segment) {
                line.segment = Some(new_segment);
                if line.label.as_deref() == Some("b") {
                    line.trailing = transform_enable_flags(&line.trailing, name);
                }
            }
        }
    }
    Some(serialize_script(&lines, crate::config::LINE_BREAK))
}

pub fn transform_2d(script: &str, name: &str) -> Option<String> {
    transform_2d_script(script, name)
}
pub fn copy(value: &str) -> Result<(), String> {
    clipboard::write_text(value)
}

/// Full-width rotation button with an explicit action label.
fn rotation_button<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    let style_fn = theme::button_style(theme::Category::Secondary);
    let content = container(
        text(label)
            .size(theme::BODY_SIZE)
            .font(super::widgets::button_font())
            .width(Length::Fill)
            .align_x(iced::Alignment::Center)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center);
    button(content)
        .width(Length::Fill)
        .height(Length::Fixed(theme::CONTROL_HEIGHT))
        .padding([0.0, theme::SPACE_SM])
        .style(move |theme: &Theme, status| style_fn(theme, status))
        .on_press(message)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turner_button_labels_preserve_user_facing_contract() {
        assert_eq!(
            [
                TURNER_PASTE_LABEL,
                TURNER_MIRROR_XY_X_LABEL,
                TURNER_MIRROR_XY_Y_LABEL,
                TURNER_MIRROR_XZ_X_LABEL,
                TURNER_MIRROR_XZ_Z_LABEL,
                TURNER_COPY_LABEL,
            ],
            [
                "Вставить данные из буфера обмена",
                "Отражение XY по X",
                "Отражение XY по Y",
                "Отражение XZ по X",
                "Отражение XZ по Z",
                "Копировать данные в буфер обмена",
            ]
        );
    }

    #[test]
    fn stale_analysis_result_cannot_replace_newer_script_state() {
        let mut page = CheckPage::default();
        let request = page
            .sync_script("p 0 0 0 1 1 1 material")
            .expect("non-empty script creates an analysis request");
        let stale_result = run_check_analysis(request);

        page.sync_script("p 0 0 0 2 2 2 material");

        assert!(!page.apply_analysis(stale_result));
        assert!(page.analysis.is_none());
        assert!(page.cavity.is_none());
    }
}
