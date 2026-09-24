use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::widget::{checkbox, column, container, row, scrollable, stack, text};
use iced::{Element, Length, Subscription, Task, Theme};

use super::interactive_pages::{CheckPage, Turner2DPage, TurnerPage};
use super::license::LicensePage;
use super::report::ReportPage;
use super::static_pages::{AirCavitiesPage, CornerPage, MaterialSortPage};
use super::step3d::Step3DPage;

use crate::licensing::LicenseOperation;

use super::theme;
use super::widgets::{nav_button, page_button, separator, spacer};

const SIDEBAR_WIDTH: f32 = 140.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Turner,
    Turner2D,
    MaterialSort,
    AirCavities,
    Check,
    Corner,
    Step3D,
    Report,
    License,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Turner => "Поворотник",
            Self::Turner2D => "2D Поворотник",
            Self::MaterialSort => "Сортировка",
            Self::AirCavities => "Прослойки",
            Self::Check => "Проверка",
            Self::Corner => "Угол окна",
            Self::Step3D => "3D → STEP",
            Self::Report => "Шкала",
            Self::License => "Лицензия",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Page),
    About,
    CloseAbout,
    Settings,
    CloseSettings,
    SettingsTheme(AppTheme),
    SettingsGrid(bool),
    SystemThemeTick,
    ModalBackdrop,
    CanvasHover,
    ReportSelect(usize, usize, bool, bool),
    ReportPaste,
    ReportOpenMtl,
    ReportSavePng,
    ReportCopySelected,
    ReportCopyAll,
    ReportCopyScale,
    ReportCopyShortcut,
    ModifiersChanged(iced::keyboard::Modifiers),
    LicenseKeyChanged(String),
    LicenseToggleReveal,
    LicensePaste,
    LicenseActivate,
    LicenseRefresh,
    LicenseConfirmDeactivate,
    LicenseCancelDeactivate,
    LicenseDeactivate,
    LicensePoll,
    MaterialPaste,
    MaterialOpen,
    MaterialSort,
    MaterialMove(usize, isize),
    MaterialCopy,
    CornerPaste,
    CornerDirection(isize),
    CornerCreate,
    CornerCopy,
    AirPaste,
    AirOpen,
    AirNameChanged(String),
    AirToggleNumber(bool),
    AirToggleLambda(bool),
    AirPickColor,
    AirColorRedChanged(String),
    AirColorGreenChanged(String),
    AirColorBlueChanged(String),
    AirColorHsvChanged(f32, f32, f32),
    AirApplyColor,
    AirCloseColor,
    AirHatch(u8),
    AirApply,
    AirConfirmApply,
    AirCancelApply,
    CheckPaste,
    CheckTolerance(String),
    CheckMaxChange(String),
    CheckTick,
    CheckAnalysisCompleted(super::interactive_pages::CheckAnalysisResult),
    CheckProposalToggle(usize),
    CheckProposalCollapse(usize),
    CheckApplySelected,
    CheckSimplifyAll,
    CheckCopy,
    TurnerPaste,
    TurnerProjection(super::preview2d::Projection),
    TurnerInstruction,
    TurnerTransform(&'static str),
    TurnerCopy,
    Turner2DPaste,
    Turner2DInstruction,
    Turner2DTransform(&'static str),
    Turner2DCopy,
    StepPaste,
    StepExport,
    StepRotate(f32, f32),
    Step3DView(super::preview3d::StandardView),
    Step3DRotated(f64, f64),
    CornerView(super::preview3d::StandardView),
    CornerRotated(f64, f64),
    WindowOpened,
}

pub struct App {
    page: Page,
    show_about: bool,
    show_settings: bool,
    theme_mode: AppTheme,
    show_grid: bool,
    report: ReportPage,
    license: LicensePage,
    license_manager: crate::licensing::LicenseManager,
    material_sort: MaterialSortPage,
    corner: CornerPage,
    air_cavities: AirCavitiesPage,
    check: CheckPage,
    turner: TurnerPage,
    turner_2d: Turner2DPage,
    step_3d: Step3DPage,
    script_text: String,
    script_text_2d: String,
    /// Single explicit source of 3D material colors shared by STEP/Corner.
    active_mtl_path: Option<PathBuf>,
    active_mtl_colors: HashMap<String, iced::Color>,
    modifiers: iced::keyboard::Modifiers,
    last_system_dark: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTheme {
    #[default]
    Light,
    System,
    Dark,
}

impl AppTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Системная",
            Self::Light => "Светлая",
            Self::Dark => "Тёмная",
        }
    }
}

impl std::fmt::Display for AppTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Page::Turner,
            show_about: false,
            show_settings: false,
            theme_mode: AppTheme::default(),
            show_grid: true,
            report: ReportPage::default(),
            license: LicensePage::default(),
            license_manager: crate::licensing::LicenseManager::new(),
            material_sort: MaterialSortPage::default(),
            corner: CornerPage::default(),
            air_cavities: AirCavitiesPage::default(),
            check: CheckPage::default(),
            turner: TurnerPage::default(),
            turner_2d: Turner2DPage::default(),
            step_3d: Step3DPage::default(),
            script_text: String::new(),
            script_text_2d: String::new(),
            active_mtl_path: None,
            active_mtl_colors: HashMap::new(),
            modifiers: iced::keyboard::Modifiers::default(),
            last_system_dark: None,
        }
    }
}

fn active_mtl_label(app: &App) -> Option<String> {
    app.active_mtl_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
}

/// Publishes the single shared 3D material source to every consumer so STEP and
/// Corner can neither diverge nor present an unclear, order-dependent origin.
fn publish_active_mtl(app: &mut App) {
    let label = active_mtl_label(app);
    app.step_3d
        .set_material_colors(app.active_mtl_colors.clone());
    app.step_3d.set_mtl_source(label.clone());
    app.corner
        .set_material_colors(app.active_mtl_colors.clone());
    app.corner.set_mtl_source(label);
}

fn set_active_mtl(app: &mut App, path: &Path, colors: HashMap<String, iced::Color>) {
    app.active_mtl_path = Some(path.to_path_buf());
    app.active_mtl_colors = colors;
    publish_active_mtl(app);
}

fn clear_active_mtl(app: &mut App) {
    app.active_mtl_path = None;
    app.active_mtl_colors.clear();
    publish_active_mtl(app);
}

fn start_check_analysis(request: super::interactive_pages::CheckAnalysisRequest) -> Task<Message> {
    Task::perform(
        async move { super::interactive_pages::run_check_analysis(request) },
        Message::CheckAnalysisCompleted,
    )
}

fn sync_script_fields(app: &mut App) -> Task<Message> {
    app.turner.preview.show_grid = app.show_grid;
    app.turner_2d.preview.show_grid = app.show_grid;
    app.step_3d.sync_lines(&app.script_text);
    app.turner.sync(&app.script_text);
    app.corner.sync_script(&app.script_text);
    app.report.sync_script(&app.script_text);
    app.material_sort.sync_script(&app.script_text);
    app.check
        .sync_script(&app.script_text)
        .map(start_check_analysis)
        .unwrap_or_else(Task::none)
}

fn replace_shared_script(app: &mut App, script: String) -> Task<Message> {
    app.script_text = script;
    sync_script_fields(app)
}

fn same_mtl_path(left: &Path, right: &Path) -> bool {
    let normalized_left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let normalized_right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    normalized_left == normalized_right
}

fn reload_material_cache(
    source_path: Option<&PathBuf>,
    cache: &mut HashMap<String, crate::material_sort::MtlMaterial>,
    mutated_path: &Path,
) -> Result<bool, String> {
    let Some(source_path) = source_path else {
        return Ok(false);
    };
    if !same_mtl_path(source_path, mutated_path) {
        return Ok(false);
    }
    let materials = crate::material_sort::parse_mtl_file(mutated_path, true)?;
    *cache = crate::material_sort::materials_by_normalized_name_checked(&materials)?;
    Ok(true)
}

fn message_allowed_while_unlicensed(message: &Message) -> bool {
    matches!(
        message,
        Message::Navigate(_)
            | Message::About
            | Message::CloseAbout
            | Message::Settings
            | Message::CloseSettings
            | Message::SettingsTheme(_)
            | Message::SettingsGrid(_)
            | Message::ModalBackdrop
            | Message::CanvasHover
            | Message::ModifiersChanged(_)
            | Message::LicenseKeyChanged(_)
            | Message::LicenseToggleReveal
            | Message::LicensePaste
            | Message::LicenseActivate
            | Message::LicenseRefresh
            | Message::LicenseConfirmDeactivate
            | Message::LicenseCancelDeactivate
            | Message::LicenseDeactivate
            | Message::LicensePoll
            | Message::WindowOpened
    )
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    let snapshot = app.license_manager.snapshot();
    if !snapshot.dev_mode
        && !snapshot.access.is_allowed()
        && !message_allowed_while_unlicensed(&message)
    {
        // A background analysis that completes while access is denied must not
        // be lost: mark the check analysis dirty so it is rescheduled once
        // access is restored. The authorization boundary itself is preserved.
        if matches!(message, Message::CheckAnalysisCompleted(_)) {
            app.check.mark_pending();
        }
        return Task::none();
    }
    let mut task = Task::none();
    match message {
        Message::Navigate(page) => {
            app.page = page;
            app.show_settings = false;
            app.air_cavities.close_color_picker();
        }
        Message::About => app.show_about = true,
        Message::CloseAbout => app.show_about = false,
        Message::Settings => app.show_settings = true,
        Message::CloseSettings => app.show_settings = false,
        Message::SettingsTheme(theme_mode) => {
            app.theme_mode = theme_mode;
            if theme_mode == AppTheme::System {
                app.last_system_dark = Some(super::platform::windows_dark_mode());
            } else {
                app.last_system_dark = None;
            }
            super::platform::apply_titlebar_theme(is_dark(app));
            // Theme change affects canvas colors; invalidate preview caches.
            app.turner.preview.cache.clear();
            app.turner_2d.preview.cache.clear();
        }
        Message::SettingsGrid(show) => {
            app.show_grid = show;
            app.turner.preview.cache.clear();
            app.turner_2d.preview.cache.clear();
        }
        Message::SystemThemeTick => {
            if app.theme_mode == AppTheme::System {
                let dark = super::platform::windows_dark_mode();
                if app.last_system_dark != Some(dark) {
                    app.last_system_dark = Some(dark);
                    super::platform::apply_titlebar_theme(dark);
                    app.turner.preview.cache.clear();
                    app.turner_2d.preview.cache.clear();
                }
            }
        }
        Message::ModalBackdrop => {}
        Message::CanvasHover => {}
        Message::ReportSelect(row, column, shift, command) => {
            app.report.select_cell((row, column), shift, command)
        }
        Message::ReportPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.report.status = Some((
                    "Буфер обмена пуст или содержит некорректные данные.".to_owned(),
                    true,
                ));
            }
            Ok(text) => {
                task = replace_shared_script(app, text);
                app.report.selected_cells.clear();
                app.report.selection_anchor = None;
                app.report.status = Some(("Скрипт вставлен из буфера обмена.".to_owned(), false));
            }
            Err(error) => {
                app.report.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::ReportOpenMtl => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Material files", &["mtl", "MTL"])
                .pick_file()
            {
                match crate::material_sort::parse_mtl_file(&path, true).and_then(|materials| {
                    crate::material_sort::materials_by_normalized_name_checked(&materials)
                        .map(|map| (map, materials.len()))
                }) {
                    Ok((map, count)) => {
                        app.report.mtl_materials = map;
                        app.report.mtl_path = Some(path.clone());
                        app.report.refresh_items();
                        let colors = app.report.material_color_map();
                        set_active_mtl(app, path.as_path(), colors);
                        app.report.status =
                            Some((format!("Материалов загружено из MTL: {count}."), false));
                    }
                    Err(error) => {
                        app.report.status = Some((format!("Ошибка MTL: {error}"), true));
                    }
                }
            }
        }
        Message::ReportSavePng => app.report.save_scale_png(),
        Message::ReportCopySelected => app.report.copy_selected(),
        Message::ReportCopyAll => app.report.copy_all(),
        Message::ReportCopyScale => app.report.copy_scale_image(),
        Message::ReportCopyShortcut => {
            if app.page == Page::Report {
                app.report.copy_selected();
            }
        }
        Message::ModifiersChanged(modifiers) => app.modifiers = modifiers,
        Message::LicenseKeyChanged(value) => app.license.set_key(value),
        Message::LicenseToggleReveal => app.license.reveal_key = !app.license.reveal_key,
        Message::LicensePaste => {
            if let Ok(value) = crate::clipboard::read_text() {
                app.license.set_key(value.trim().to_owned());
            }
        }
        Message::LicenseActivate => {
            let key = std::mem::take(&mut app.license.key);
            let _ = app.license_manager.activate(key);
        }
        Message::LicenseRefresh => {
            let _ = app.license_manager.refresh();
        }
        Message::LicenseConfirmDeactivate => app.license.confirm_deactivation = true,
        Message::LicenseCancelDeactivate => app.license.confirm_deactivation = false,
        Message::LicenseDeactivate => {
            app.license.confirm_deactivation = false;
            let _ = app.license_manager.deactivate();
        }
        Message::LicensePoll => {
            let _ = app.license_manager.poll();
        }
        Message::MaterialPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.material_sort.status = Some(("Буфер обмена пуст.".to_owned(), true));
            }
            Ok(text) => {
                app.script_text = text;
                task = sync_script_fields(app);
                app.material_sort.status =
                    Some(("Скрипт вставлен из буфера обмена.".to_owned(), false));
            }
            Err(error) => {
                app.material_sort.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::MaterialOpen => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Material files", &["mtl", "MTL"])
                .pick_file()
            {
                match crate::material_sort::parse_mtl_file(&path, true).and_then(|materials| {
                    crate::material_sort::materials_by_normalized_name_checked(&materials)
                        .map(|map| (map, materials.len()))
                }) {
                    Ok((map, count)) => {
                        app.material_sort.mtl_materials = map;
                        app.material_sort.mtl_path = Some(path.clone());
                        let colors = app.material_sort.color_map();
                        set_active_mtl(app, path.as_path(), colors);
                        app.material_sort.status =
                            Some((format!("Материалов загружено из MTL: {count}."), false));
                    }
                    Err(error) => {
                        app.material_sort.status = Some((format!("Ошибка MTL: {error}"), true));
                    }
                }
            }
        }
        Message::MaterialSort => {
            app.material_sort.sync_script(&app.script_text);
            super::static_pages::sort_materials(
                &mut app.material_sort.names,
                &app.material_sort.mtl_materials,
            );
            let new_script =
                super::static_pages::reorder_script(&app.script_text, &app.material_sort.names);
            if new_script != app.script_text
                && !crate::material_sort::is_material_reorder_safe(&app.script_text)
            {
                app.material_sort.status = Some((
                    "Сортировка заблокирована: перекрывающиеся боксы — изменение порядка изменит геометрию. Порядок оставлен без изменений."
                        .to_owned(),
                    true,
                ));
            } else {
                app.script_text = new_script;
                task = sync_script_fields(app);
                app.material_sort.status = Some((
                    "Материалы отсортированы по теплопроводности.".to_owned(),
                    false,
                ));
            }
        }
        Message::MaterialCopy => {
            if app.script_text.is_empty() {
                app.material_sort.status = Some(("Нет данных для копирования.".to_owned(), true));
            } else {
                match crate::clipboard::write_text(&app.script_text) {
                    Ok(()) => {
                        app.material_sort.status =
                            Some(("Скрипт скопирован в буфер обмена.".to_owned(), false))
                    }
                    Err(error) => {
                        app.material_sort.status =
                            Some((format!("Не удалось записать в буфер обмена: {error}"), true))
                    }
                }
            }
        }
        Message::MaterialMove(index, delta) => {
            app.material_sort.move_item(index, delta);
            let new_script =
                super::static_pages::reorder_script(&app.script_text, &app.material_sort.names);
            if new_script != app.script_text
                && !crate::material_sort::is_material_reorder_safe(&app.script_text)
            {
                // revert move
                app.material_sort
                    .move_item((index as isize + delta) as usize, -delta);
                app.material_sort.status = Some((
                    "Перемещение заблокировано: перекрывающиеся боксы — изменение порядка изменит геометрию."
                        .to_owned(),
                    true,
                ));
            } else {
                app.script_text = new_script;
                task = sync_script_fields(app);
                app.material_sort.status = Some(("Порядок материалов изменён.".to_owned(), false));
            }
        }
        Message::CornerPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.corner.status = Some(("Буфер обмена пуст.".to_owned(), true));
            }
            Ok(text) => {
                app.corner.result = None;
                app.corner.result_lines.clear();
                app.script_text = text;
                task = sync_script_fields(app);
                app.corner.status = Some(("Скрипт вставлен из буфера обмена.".to_owned(), false));
            }
            Err(error) => {
                app.corner.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::CornerCreate => {
            // Generate from the stable shared base and keep the variant local.
            // The shared script must not silently become the generated result,
            // otherwise direction exploration is cumulative and self-invalidating.
            let base = app.corner.base_script().to_owned();
            if let Some(result) =
                crate::ui2::static_pages::create_corner_script(&base, app.corner.direction)
            {
                app.corner.set_result(result);
            }
        }
        Message::CornerView(view) => {
            let (azimuth, elevation) = super::preview3d::standard_view_angles(view);
            app.corner.azimuth = azimuth;
            app.corner.elevation = elevation;
            app.corner.active_view = Some(view);
        }
        Message::CornerRotated(azimuth, elevation) => {
            app.corner.azimuth = azimuth;
            app.corner.elevation = elevation;
            app.corner.active_view = None;
        }
        Message::CornerCopy => {
            if let Some(text) = &app.corner.result {
                match crate::clipboard::write_text(text) {
                    Ok(()) => {
                        app.corner.status =
                            Some(("Результат скопирован в буфер обмена.".to_owned(), false))
                    }
                    Err(error) => {
                        app.corner.status =
                            Some((format!("Не удалось записать в буфер обмена: {error}"), true))
                    }
                }
            }
        }
        Message::CornerDirection(delta) => {
            let new_direction = ((app.corner.direction as isize + delta).rem_euclid(4)) as usize;
            if new_direction != app.corner.direction {
                app.corner.direction = new_direction;
                app.corner.clear_result();
            }
        }
        Message::AirApply => {
            if app.air_cavities.mtl_path.is_none() {
                app.air_cavities.status = Some(("Файл MTL не выбран.".to_owned(), true));
            } else if let Some(path) = app.air_cavities.mtl_path.clone() {
                match crate::air_cavities::preflight_air_cavity_upsert(
                    path.as_path(),
                    &app.air_cavities.specs,
                ) {
                    Ok(preflight) if preflight.has_replacements() => {
                        app.air_cavities.pending_replacements = Some(preflight.replaced.clone());
                        app.air_cavities.status = Some((
                            format!(
                                "Внимание: будут перезаписаны существующие материалы: {}.",
                                preflight.replaced.join(", ")
                            ),
                            true,
                        ));
                    }
                    Ok(_) => {
                        app.air_cavities.pending_replacements = None;
                        apply_air_cavity_upsert(app);
                    }
                    Err(error) => {
                        app.air_cavities.status = Some((format!("Ошибка: {error}"), true));
                    }
                }
            }
        }
        Message::AirCancelApply => {
            app.air_cavities.pending_replacements = None;
            app.air_cavities.status = Some(("Запись отменена.".to_owned(), false));
        }
        Message::AirConfirmApply => {
            app.air_cavities.pending_replacements = None;
            apply_air_cavity_upsert(app);
        }
        Message::AirOpen => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Material files", &["mtl", "MTL"])
                .pick_file()
            {
                app.air_cavities.mtl_path = Some(path);
            }
        }
        Message::AirPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.air_cavities.status = Some((
                    "Буфер обмена пуст или содержит некорректные данные.".to_owned(),
                    true,
                ));
            }
            Ok(text) => {
                app.air_cavities.log_text = text;
                app.air_cavities.status = None;
                app.air_cavities.sync_specs();
            }
            Err(error) => {
                app.air_cavities.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::AirNameChanged(value) => {
            app.air_cavities.name_mask = value;
            app.air_cavities.sync_specs();
        }
        Message::AirToggleNumber(value) => {
            app.air_cavities.add_number = value;
            app.air_cavities.sync_specs();
        }
        Message::AirToggleLambda(value) => {
            app.air_cavities.add_lambda = value;
            app.air_cavities.sync_specs();
        }
        Message::AirPickColor => app.air_cavities.open_color_picker(),
        Message::AirColorRedChanged(value) => {
            app.air_cavities.set_color_text(0, value);
        }
        Message::AirColorGreenChanged(value) => {
            app.air_cavities.set_color_text(1, value);
        }
        Message::AirColorBlueChanged(value) => {
            app.air_cavities.set_color_text(2, value);
        }
        Message::AirColorHsvChanged(hue, saturation, value) => {
            app.air_cavities.set_color_hsv(hue, saturation, value);
        }
        Message::AirApplyColor => {
            if app.air_cavities.apply_color_draft() {
                app.air_cavities.sync_specs();
            }
        }
        Message::AirCloseColor => app.air_cavities.close_color_picker(),
        Message::AirHatch(value) => {
            app.air_cavities.special_value = value;
            app.air_cavities.sync_specs();
        }
        Message::CheckPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.check.clear();
                app.check.status = Some(("Буфер обмена пуст.".to_owned(), true));
            }
            Ok(text) => {
                app.script_text = text;
                task = sync_script_fields(app);
                app.check.status = Some(("Скрипт вставлен из буфера обмена.".to_owned(), false));
            }
            Err(error) => {
                app.check.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::CheckTolerance(value) => {
            app.check.tolerance = value.clone();
            match value.trim().parse::<f64>() {
                Ok(v) if v.is_finite() => {
                    let clamped = v.clamp(0.1, 100.0);
                    app.check.tolerance = format!("{clamped:.1}");
                    app.check.status = None;
                    app.check.mark_pending();
                }
                _ => {
                    app.check.invalidate_for_invalid_input();
                    app.check.status = Some((
                        "Толерантность: введите конечное число 0.1…100".to_owned(),
                        true,
                    ));
                }
            }
        }
        Message::CheckMaxChange(value) => {
            app.check.max_change = value.clone();
            match value.trim().parse::<f64>() {
                Ok(v) if v.is_finite() => {
                    let clamped = v.clamp(0.1, 50.0);
                    app.check.max_change = format!("{clamped:.1}");
                    app.check.status = None;
                    app.check.mark_pending();
                }
                _ => {
                    app.check.invalidate_for_invalid_input();
                    app.check.status = Some((
                        "Макс. изменение: введите конечное число 0.1…50".to_owned(),
                        true,
                    ));
                }
            }
        }
        Message::CheckTick => {
            if let Some(request) = app.check.tick() {
                task = start_check_analysis(request);
            }
        }
        Message::CheckAnalysisCompleted(result) => {
            app.check.apply_analysis(result);
        }
        Message::CheckProposalToggle(index) => {
            if let Some(value) = app.check.checked.get_mut(index) {
                *value = !*value;
            }
        }
        Message::CheckProposalCollapse(index) => {
            if let Some(value) = app.check.collapsed.get_mut(index) {
                *value = !*value;
            }
        }
        Message::CheckApplySelected => {
            if !app.script_text.trim().is_empty() && !app.check.proposals.is_empty() {
                let selected: Vec<_> = app
                    .check
                    .proposals
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| app.check.checked.get(*index).copied().unwrap_or(false))
                    .map(|(_, proposal)| proposal.clone())
                    .collect();
                if selected.is_empty() {
                    app.check.status = Some(("Не выбрано ни одного упрощения.".to_owned(), true));
                } else {
                    let result = crate::model_check::simplify_proposals(
                        &app.script_text,
                        &selected,
                        app.check.tolerance.trim().parse().unwrap_or_default(),
                        app.check.max_change.trim().parse().unwrap_or_default(),
                    );
                    if result.rejected.is_empty() {
                        app.check.status = Some((
                            format!("Применено упрощений: {}.", result.applied_count),
                            false,
                        ));
                    } else {
                        let rejected = result
                            .rejected
                            .iter()
                            .map(|proposal| {
                                format!(
                                    "{} {} → {}",
                                    proposal.axis, proposal.coord_from, proposal.coord_to
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        app.check.status = Some((
                            format!(
                                "Применено: {}. Пропущено как небезопасные: {rejected}.",
                                result.applied_count
                            ),
                            true,
                        ));
                    }
                    app.check.simplified = Some(result.script);
                }
            }
        }
        Message::CheckSimplifyAll => {
            if !app.script_text.trim().is_empty() && !app.check.proposals.is_empty() {
                let result = crate::model_check::simplify_proposals(
                    &app.script_text,
                    &app.check.proposals,
                    app.check.tolerance.trim().parse().unwrap_or_default(),
                    app.check.max_change.trim().parse().unwrap_or_default(),
                );
                app.check.status = Some((
                    format!("Применено упрощений: {}.", result.applied_count),
                    !result.rejected.is_empty(),
                ));
                app.check.simplified = Some(result.script);
            }
        }
        Message::CheckCopy => {
            if let Some(result) = &app.check.simplified {
                match crate::clipboard::write_text(result) {
                    Ok(()) => {
                        app.check.status =
                            Some(("Результат скопирован в буфер обмена.".to_owned(), false))
                    }
                    Err(error) => app.check.status = Some((error, true)),
                }
            }
        }
        Message::TurnerPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.turner.status = Some(("Буфер обмена пуст.".to_owned(), true));
            }
            Ok(text) => {
                app.script_text = text;
                task = sync_script_fields(app);
                app.turner.status = Some(("Скрипт вставлен из буфера обмена.".to_owned(), false));
            }
            Err(error) => {
                app.turner.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::TurnerCopy => {
            if app.script_text.is_empty() {
                app.turner.status = Some(("Нет данных для копирования.".to_owned(), true));
            } else {
                match crate::clipboard::write_text(&app.script_text) {
                    Ok(()) => {
                        app.turner.status =
                            Some(("Скрипт скопирован в буфер обмена.".to_owned(), false))
                    }
                    Err(error) => {
                        app.turner.status =
                            Some((format!("Не удалось записать в буфер обмена: {error}"), true));
                    }
                }
            }
        }
        Message::TurnerProjection(projection) => {
            app.turner.preview.set_projection(projection);
            app.turner.projection = projection;
        }
        Message::TurnerTransform(name) => {
            if app.script_text.is_empty() {
                app.turner.status = Some(("Нет данных для преобразования.".to_owned(), true));
            } else {
                let unsupported =
                    crate::transforms::unsupported_geometry_commands(&app.script_text);
                if !unsupported.is_empty() {
                    app.turner.status = Some((
                        format!(
                            "Преобразование отменено: скрипт содержит неподдерживаемые геометрические команды ({}). Такие команды остались бы в старых координатах.",
                            unsupported.join(", ")
                        ),
                        true,
                    ));
                } else if let Some(result) =
                    super::interactive_pages::transform_script(&app.script_text, name)
                {
                    app.script_text = result;
                    task = sync_script_fields(app);
                    app.turner.status = Some(("Преобразование выполнено.".to_owned(), false));
                } else {
                    app.turner.status = Some((
                        "В скрипте не найдены геометрические элементы.".to_owned(),
                        true,
                    ));
                }
            }
        }
        Message::TurnerInstruction => app.turner.show_instruction = !app.turner.show_instruction,
        Message::Turner2DPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.turner_2d.status = Some(("Буфер обмена пуст.".to_owned(), true));
            }
            Ok(text) => {
                app.script_text_2d = text;
                app.turner_2d.sync(&app.script_text_2d);
                app.turner_2d.status =
                    Some(("2D-скрипт вставлен из буфера обмена.".to_owned(), false));
            }
            Err(error) => {
                app.turner_2d.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::Turner2DCopy => {
            if app.script_text_2d.is_empty() {
                app.turner_2d.status = Some(("Нет данных для копирования.".to_owned(), true));
            } else {
                match crate::clipboard::write_text(&app.script_text_2d) {
                    Ok(()) => {
                        app.turner_2d.status =
                            Some(("2D-скрипт скопирован в буфер обмена.".to_owned(), false))
                    }
                    Err(error) => {
                        app.turner_2d.status =
                            Some((format!("Не удалось записать в буфер обмена: {error}"), true));
                    }
                }
            }
        }
        Message::Turner2DInstruction => {
            app.turner_2d.show_instruction = !app.turner_2d.show_instruction
        }
        Message::Turner2DTransform(name) => {
            if app.script_text_2d.is_empty() {
                app.turner_2d.status = Some(("Нет данных для преобразования.".to_owned(), true));
            } else {
                let unsupported =
                    crate::turner2d::unsupported_geometry_commands(&app.script_text_2d);
                if !unsupported.is_empty() {
                    app.turner_2d.status = Some((
                        format!(
                            "Преобразование отменено: 2D-скрипт содержит неподдерживаемые пространственные команды ({}). Они остались бы в старых координатах.",
                            unsupported.join(", ")
                        ),
                        true,
                    ));
                } else if let Some(result) =
                    super::interactive_pages::transform_2d(&app.script_text_2d, name)
                {
                    app.script_text_2d = result;
                    app.turner_2d.sync(&app.script_text_2d);
                    app.turner_2d.status = Some(("Преобразование выполнено.".to_owned(), false));
                } else {
                    app.turner_2d.status =
                        Some(("Не удалось выполнить преобразование.".to_owned(), true));
                }
            }
        }
        Message::StepPaste => match crate::clipboard::read_text() {
            Ok(text) if text.trim().is_empty() => {
                app.step_3d.status = Some(("Буфер обмена пуст.".to_owned(), true));
            }
            Ok(text) => {
                app.script_text = text;
                task = sync_script_fields(app);
                app.step_3d.status = Some(("Скрипт вставлен из буфера обмена.".to_owned(), false));
            }
            Err(error) => {
                app.step_3d.status =
                    Some((format!("Не удалось прочитать буфер обмена: {error}"), true));
            }
        },
        Message::StepExport => match super::step3d::export_script(&app.script_text) {
            Ok(message) => app.step_3d.status = Some((message, false)),
            Err(error) => app.step_3d.status = Some((error, true)),
        },
        Message::StepRotate(azimuth, elevation) => {
            app.step_3d.azimuth += azimuth as f64;
            app.step_3d.elevation += elevation as f64;
            app.step_3d.active_view = None;
        }
        Message::Step3DView(view) => {
            let (azimuth, elevation) = super::preview3d::standard_view_angles(view);
            app.step_3d.azimuth = azimuth;
            app.step_3d.elevation = elevation;
            app.step_3d.active_view = Some(view);
        }
        Message::Step3DRotated(azimuth, elevation) => {
            app.step_3d.azimuth = azimuth;
            app.step_3d.elevation = elevation;
            app.step_3d.active_view = None;
        }
        Message::WindowOpened => super::platform::apply_titlebar_theme(is_dark(app)),
    }
    task
}

/// Writes the generated Air Cavities materials into the selected MTL and
/// refreshes every derived cache. Callers must have already confirmed any
/// intentional overwrite of existing materials.
fn apply_air_cavity_upsert(app: &mut App) {
    let Some(path) = app.air_cavities.mtl_path.clone() else {
        app.air_cavities.status = Some(("Файл MTL не выбран.".to_owned(), true));
        return;
    };
    match super::static_pages::apply_air_cavity_upsert(path.as_path(), &app.air_cavities) {
        Ok(result) => {
            let report_reload = reload_material_cache(
                app.report.mtl_path.as_ref(),
                &mut app.report.mtl_materials,
                path.as_path(),
            );
            let material_reload = reload_material_cache(
                app.material_sort.mtl_path.as_ref(),
                &mut app.material_sort.mtl_materials,
                path.as_path(),
            );
            let mut reload_errors = Vec::new();
            // Only a reload that produced a usable map from the active source
            // may refresh the shared 3D colors; otherwise the source is cleared.
            let active_mutated = app
                .active_mtl_path
                .as_ref()
                .is_some_and(|active| same_mtl_path(active, path.as_path()));
            let mut refreshed_colors = None;
            match report_reload {
                Ok(true) => {
                    app.report.refresh_items();
                    refreshed_colors = Some(app.report.material_color_map());
                }
                Ok(false) => {}
                Err(error) => {
                    app.report.mtl_materials.clear();
                    app.report.refresh_items();
                    app.report.status =
                        Some((format!("MTL требуется открыть повторно: {error}"), true));
                    reload_errors.push("шкала".to_owned());
                }
            }
            match material_reload {
                Ok(true) => {
                    refreshed_colors.get_or_insert_with(|| app.material_sort.color_map());
                }
                Ok(false) => {}
                Err(error) => {
                    app.material_sort.mtl_materials.clear();
                    app.material_sort.status =
                        Some((format!("MTL требуется открыть повторно: {error}"), true));
                    reload_errors.push("сортировка".to_owned());
                }
            }
            if active_mutated {
                match refreshed_colors {
                    Some(colors) if reload_errors.is_empty() => {
                        set_active_mtl(app, path.as_path(), colors);
                    }
                    _ => clear_active_mtl(app),
                }
            }
            let status = format!(
                "Готово. Обновлено: {}, добавлено: {}",
                result.updated_count(),
                result.added_count()
            );
            app.air_cavities.status = Some((
                if reload_errors.is_empty() {
                    status
                } else {
                    format!(
                        "{status}. Не удалось обновить: {}.",
                        reload_errors.join(", ")
                    )
                },
                !reload_errors.is_empty(),
            ));
        }
        Err(error) => {
            app.air_cavities.status = Some((format!("Ошибка: {error}"), true));
        }
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let snapshot = app.license_manager.snapshot();
    let licensed = snapshot.dev_mode || snapshot.access.is_allowed();

    let max_w = if !licensed {
        theme::LICENSE_CONTENT_WIDTH
    } else if matches!(app.page, Page::Turner | Page::Turner2D) {
        theme::TURNER_CONTENT_WIDTH
    } else {
        theme::CONTENT_WIDTH
    };
    let content: Element<'_, Message> = if !licensed {
        app.license.view(snapshot)
    } else {
        match app.page {
            Page::Turner => app.turner.view(),
            Page::Turner2D => app.turner_2d.view(),
            Page::MaterialSort => app.material_sort.view(),
            Page::AirCavities => app.air_cavities.view(),
            Page::Check => app.check.view(),
            Page::Corner => app.corner.view(),
            Page::Step3D => app.step_3d.view(),
            Page::Report => app.report.view(app.modifiers),
            Page::License => app.license.view(snapshot),
        }
    };
    // Width constraints include their own padding. Keeping the fixed width on
    // an already-padded child used to make the 140 + 320 shell overflow the
    // 460 px client area and clip the right edge of every page.
    let clamped: Element<'_, Message> = container(
        container(content)
            .width(Length::Fill)
            .max_width(max_w - 2.0 * theme::SPACE_LG)
            .padding(theme::SPACE_LG),
    )
    .width(Length::Fill)
    .align_x(iced::Alignment::Center)
    .into();

    let main: Element<'_, Message> = if licensed {
        let nav_items = [
            Page::Turner,
            Page::Turner2D,
            Page::MaterialSort,
            Page::AirCavities,
            Page::Check,
            Page::Corner,
            Page::Step3D,
            Page::Report,
        ];
        let nav_top: Element<'_, Message> = nav_items
            .into_iter()
            .map(|page| nav_button(page.label(), app.page == page, Message::Navigate(page)))
            .fold(column![], |column, item| {
                column.push(item).spacing(theme::PAGE_SPACING)
            })
            .into();

        let about_button: Element<'_, Message> = nav_button("О программе", false, Message::About);

        let settings_button: Element<'_, Message> =
            nav_button("Настройки", false, Message::Settings);

        let nav_panel: Element<'_, Message> = column![
            nav_top,
            spacer(),
            separator(),
            settings_button,
            about_button,
        ]
        .spacing(theme::PAGE_SPACING)
        .into();

        let sidebar: Element<'_, Message> = container(nav_panel)
            .width(Length::Fixed(SIDEBAR_WIDTH))
            .height(Length::Fill)
            .padding(theme::SPACE_SM)
            .style(theme::side_panel_rail)
            .into();

        let shell: Element<'_, Message> = row![
            sidebar,
            scrollable(clamped).width(Length::Fill).height(Length::Fill),
        ]
        .spacing(0)
        .into();

        container(shell)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::panel)
            .into()
    } else {
        container(column![spacer(), scrollable(clamped).height(Length::Fill),].height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::panel)
            .into()
    };

    let mut overlays: Vec<Element<'_, Message>> = Vec::new();
    if app.show_about {
        overlays.push(about_card());
    }
    if app.show_settings {
        overlays.push(settings_card(app.theme_mode, app.show_grid));
    }
    if app.page == Page::Turner {
        if let Some(card) = app.turner.instruction_overlay() {
            overlays.push(card);
        }
    }
    if app.page == Page::Turner2D {
        if let Some(card) = app.turner_2d.instruction_overlay() {
            overlays.push(card);
        }
    }
    if app.page == Page::AirCavities {
        if let Some(card) = app.air_cavities.color_overlay() {
            overlays.push(card);
        }
    }

    if overlays.is_empty() {
        main
    } else {
        let mut layers: Vec<Element<'_, Message>> = vec![main];
        layers.extend(overlays);
        stack(layers).into()
    }
}

fn about_card() -> Element<'static, Message> {
    let card = column![
        super::widgets::page_title("HEAT3 ПОВОРОТНИК"),
        super::widgets::meta_label(format!("ВЕРСИЯ {}", env!("CARGO_PKG_VERSION"))),
        separator(),
        text("Утилита поворота, отражения и перестановки осей скриптов HEAT3.")
            .size(theme::BODY_SIZE)
            .style(move |theme| text::Style {
                color: Some(theme::ink(theme::is_dark(theme))),
            }),
        text(" ").size(theme::SPACE_XS),
        super::widgets::meta_label("РАЗРАБОТЧИК"),
        text("Михаил Трусов")
            .size(theme::BODY_SIZE)
            .style(move |theme| text::Style {
                color: Some(theme::ink(theme::is_dark(theme))),
            }),
        super::widgets::meta_label("ОРГАНИЗАЦИЯ"),
        text("Институт пассивного дома")
            .size(theme::BODY_SIZE)
            .style(move |theme| text::Style {
                color: Some(theme::ink(theme::is_dark(theme))),
            }),
        text(" ").size(theme::SPACE_XS),
        page_button(
            "Закрыть",
            theme::Category::Secondary,
            true,
            Message::CloseAbout,
        ),
    ]
    .spacing(theme::SPACE_SM);

    super::widgets::modal(card.into(), 280.0)
}

fn settings_card(theme_mode: AppTheme, show_grid: bool) -> Element<'static, Message> {
    let theme_row: Element<'static, Message> = row![
        text("Тема:").size(theme::BODY_SIZE),
        iced::widget::pick_list(
            vec![AppTheme::System, AppTheme::Light, AppTheme::Dark],
            Some(theme_mode),
            Message::SettingsTheme,
        )
        .style(theme::pick_list_style)
        .menu_style(theme::pick_list_menu_style)
        .text_size(theme::BODY_SIZE)
        .width(Length::Fixed(132.0)),
    ]
    .spacing(theme::SPACE_SM)
    .into();

    let grid_row: Element<'static, Message> = row![
        text("Сетка в предпросмотре:").size(theme::BODY_SIZE),
        checkbox(show_grid)
            .style(theme::checkbox_style)
            .size(theme::BODY_SIZE)
            .on_toggle(Message::SettingsGrid),
    ]
    .spacing(theme::SPACE_SM)
    .into();

    let card = column![
        super::widgets::page_title("НАСТРОЙКИ"),
        separator(),
        theme_row,
        grid_row,
        separator(),
        page_button(
            "Лицензия",
            theme::Category::Secondary,
            true,
            Message::Navigate(Page::License),
        ),
        page_button(
            "Закрыть",
            theme::Category::Ghost,
            true,
            Message::CloseSettings,
        ),
    ]
    .spacing(theme::SPACE_SM);

    super::widgets::modal(card.into(), 280.0)
}

pub fn theme(app: &App) -> Theme {
    match app.theme_mode {
        AppTheme::Light => Theme::Light,
        AppTheme::Dark => Theme::Dark,
        AppTheme::System => {
            if super::platform::windows_dark_mode() {
                Theme::Dark
            } else {
                Theme::Light
            }
        }
    }
}

fn is_dark(app: &App) -> bool {
    match app.theme_mode {
        AppTheme::Light => false,
        AppTheme::Dark => true,
        AppTheme::System => super::platform::windows_dark_mode(),
    }
}

pub fn subscription(app: &App) -> Subscription<Message> {
    let window_events = iced::event::listen_with(|event, _status, _window| {
        if matches!(
            event,
            iced::Event::Window(iced::window::Event::Opened { .. })
        ) {
            Some(Message::WindowOpened)
        } else {
            None
        }
    });
    let license_poll = if app.license_manager.snapshot().dev_mode {
        // Dev build: licensing is simulated, nothing to poll.
        Subscription::<Message>::none()
    } else if app.license_manager.snapshot().operation != LicenseOperation::Idle {
        // Active activation/deactivation — poll fast to catch worker completion.
        iced::time::every(Duration::from_millis(80)).map(|_| Message::LicensePoll)
    } else {
        // Idle: slow heartbeat only for online-refresh due checks.
        iced::time::every(Duration::from_secs(30)).map(|_| Message::LicensePoll)
    };
    let check_tick = if app.check.is_pending() {
        iced::time::every(Duration::from_millis(100)).map(|_| Message::CheckTick)
    } else {
        Subscription::<Message>::none()
    };
    let system_theme_poll = if app.theme_mode == AppTheme::System {
        iced::time::every(Duration::from_secs(1)).map(|_| Message::SystemThemeTick)
    } else {
        Subscription::<Message>::none()
    };
    let keyboard_events = iced::event::listen_with(|event, _status, _window| match event {
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::ModifiersChanged(modifiers))
        }
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyC),
            modifiers,
            ..
        }) if modifiers.command() => Some(Message::ReportCopyShortcut),
        _ => None,
    });
    Subscription::batch([
        window_events,
        license_poll,
        check_tick,
        system_theme_poll,
        keyboard_events,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_script_sync_preserves_the_independent_2d_preview() {
        let mut app = App {
            script_text_2d: "r 0 0 2 3 cavity".to_owned(),
            ..Default::default()
        };
        app.turner_2d.sync(&app.script_text_2d);
        let expected_rectangles = app.turner_2d.preview.rects.len();

        let _ = replace_shared_script(&mut app, "p 0 0 0 1 1 1 material".to_owned());

        assert_eq!(app.turner_2d.preview.rects.len(), expected_rectangles);
        assert_eq!(app.check.cached_script, app.script_text);
    }

    #[test]
    fn corner_generation_keeps_result_local_and_stable_across_directions() {
        let mut app = App {
            license_manager: crate::licensing::LicenseManager::development_for_tests(),
            ..Default::default()
        };
        // Pseudo-2D material section: constant X, so a corner can be generated.
        let _ = replace_shared_script(
            &mut app,
            "p 0 0 0 0.1 1 1 material\np 0.1 0 0 0.2 1 1 material".to_owned(),
        );
        let base = app.script_text.clone();

        let _ = update(&mut app, Message::CornerCreate);
        assert!(
            app.corner.result.is_some(),
            "generated result survives sync"
        );
        assert_eq!(app.script_text, base, "shared source is not overwritten");

        let first = app.corner.result.clone().unwrap();
        let _ = update(&mut app, Message::CornerDirection(1));
        let _ = update(&mut app, Message::CornerCreate);
        let second = app.corner.result.clone().unwrap();
        assert_ne!(first, second, "each direction regenerates");
        assert_eq!(app.script_text, base, "base remains stable");

        // External source change invalidates the derived result.
        let _ = replace_shared_script(&mut app, "p 0 0 0 1 1 1 other".to_owned());
        assert!(
            app.corner.result.is_none(),
            "external change invalidates result"
        );
    }

    #[test]
    fn active_mtl_source_is_single_and_inspectable_for_step_and_corner() {
        let mut app = App::default();
        let a = Path::new("A.mtl");
        let b = Path::new("B.mtl");
        let red = HashMap::from([("brick".to_owned(), iced::Color::from_rgb(1.0, 0.0, 0.0))]);
        let blue = HashMap::from([("brick".to_owned(), iced::Color::from_rgb(0.0, 0.0, 1.0))]);

        set_active_mtl(&mut app, a, red.clone());
        assert_eq!(app.active_mtl_path.as_deref(), Some(a));
        assert_eq!(app.step_3d.mtl_source.as_deref(), Some("A.mtl"));
        assert_eq!(app.corner.mtl_source.as_deref(), Some("A.mtl"));
        assert_eq!(app.step_3d.material_colors["brick"], red["brick"]);

        // A later explicit load deterministically republishes to both consumers.
        set_active_mtl(&mut app, b, blue.clone());
        assert_eq!(app.active_mtl_path.as_deref(), Some(b));
        assert_eq!(app.step_3d.mtl_source.as_deref(), Some("B.mtl"));
        assert_eq!(app.corner.mtl_source.as_deref(), Some("B.mtl"));
        assert_eq!(app.corner.material_colors["brick"], blue["brick"]);

        clear_active_mtl(&mut app);
        assert!(app.active_mtl_path.is_none());
        assert!(app.step_3d.mtl_source.is_none());
        assert!(app.corner.mtl_source.is_none());
        assert!(app.step_3d.material_colors.is_empty());
    }

    #[test]
    fn denied_analysis_completion_reschedules_instead_of_losing_work() {
        // Default manager is unlicensed/fail-closed when no compile-time config exists.
        let mut app = App::default();
        let request = app
            .check
            .sync_script("p 0 0 0 1 1 1 material")
            .expect("non-empty script schedules analysis");
        assert!(!app.check.is_pending(), "sync launched immediate analysis");

        let result = crate::ui2::interactive_pages::run_check_analysis(request);
        let _ = update(&mut app, Message::CheckAnalysisCompleted(result));

        assert!(
            app.check.is_pending(),
            "dropped completion must mark analysis dirty for rescheduling"
        );
    }

    #[test]
    fn reload_material_cache_updates_only_the_mutated_source_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("heat3-cache-{unique}.mtl"));
        let other_path = std::env::temp_dir().join(format!("heat3-cache-other-{unique}.mtl"));
        let old = crate::material_sort::MtlMaterial {
            name: "Air".to_owned(),
            thermal_x: 0.1,
            thermal_y: 0.1,
            volume_heat: 0.0,
            rgb_r: 1,
            rgb_g: 2,
            rgb_b: 3,
            special_value: 0,
        };
        let mut updated = old.clone();
        updated.thermal_x = 0.2;
        crate::material_sort::write_mtl_file(&path, std::slice::from_ref(&old)).unwrap();
        crate::material_sort::write_mtl_file(&other_path, &[old]).unwrap();

        let source = Some(path.clone());
        let mut cache = HashMap::new();
        cache.insert("air".to_owned(), updated.clone());
        crate::material_sort::write_mtl_file(&path, &[updated]).unwrap();

        assert!(reload_material_cache(source.as_ref(), &mut cache, &path).unwrap());
        assert_eq!(cache["air"].thermal_x, 0.2);
        assert!(!reload_material_cache(source.as_ref(), &mut cache, &other_path).unwrap());
        assert_eq!(cache["air"].thermal_x, 0.2);

        std::fs::remove_file(path).ok();
        std::fs::remove_file(other_path).ok();
    }
}
