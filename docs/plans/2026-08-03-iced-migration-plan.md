# План миграции UI — egui → iced

**Дата:** 2026-08-03
**Состав:** Transform + Software and systems (контролируемый переход визуального/прикладного слоя с сохранёнными инвариантами).
**Состояние:** план (готов к исполнению после утверждения владельцем; один отложенный decision gate по 3D — см. D-open[3D]).

---

## Goal

**Source:** решение пользователя — «полностью перейти с egui на iced»; текущая кодовая база `rust/src/ui/**` (egui/eframe 0.29 + glow + three-d 0.18) + действующая `docs/reviews`/`docs/plans`.
**Outcome:** Рабочее Windows-приложение HEAT3 Поворотник v2.0.0 на iced 0.14 (wgpu), повторяющее поведение и Swiss-дизайн текущей egui-версии (9 страниц, 2D/3D-превью, лицензирование, dev-license, буфер обмена/файл-диалоги, тёмная тема titlebar) — со **удалением egui/eframe/glow/three-d/glutin** из зависимости к концу миграции.
**Success:** iced-бинарь — единственный релиз; все 9 страниц parity (light + dark) по итогам обзора владельца; 2D- и 3D-превью функционируют; лицензирование (реальный серверный поток + dev-license режим) работает; CI зелёный (`cargo fmt/clippy --all-targets -D warnings/test/audit`, hygiene, secret-scan, sign); `egui`/`eframe`/`glow`/`three-d`/`glutin` удалены из `Cargo.toml`; откат-тег egui сохранён до стабилизации.
**Scope:** прикладной/визуальный слой `rust/src/ui/**` → новый модуль `rust/src/ui2/**` (iced); `main.rs`; `Cargo.toml`; CI `rust.yml`; дизайн-токены и шрифты (пере-выражение в iced).
**Guardrails (инварианты):** набор из 9 страниц; семантика и **положения кнопок** (см. `docs/plans/2026-08-03-swiss-design-plan.md` guardrails); нав-лист слева 140 px + CentralPanel; поведение страниц (повороты/отражения/перестановка осей, проверка, экспорт STEP, сортировка материалов, воздух/угол, отчёт/шкала, лицензирование); бизнес-логика (`transforms`, `parser`, `model_check`, `material_sort`, `step_export`, `corner`, `air_cavities`, `turner2d`, `licensing`); `tools/license-admin`; формат данных; финальный размер окна 460×600, `resizable=false`, `always_on_top`, `windows_subsystem="windows"`.
**Priority:** parity и reversibility поверх скорости; раннее выявление рисков (3D, wgpu-тулчейн, titlebar) дёшево.
**Horizon:** не ограничен датой; критический путь — W1→W2→W3→W8(пилот)→{W9,W10,W11}→W14→W15.
**Authority:** пользователь (владелец); `docs/plans/2026-08-03-swiss-design-plan.md` — источник истинности по дизайн-токенам; текущий egui-бинарь — эталон parity.

### Composition
Transform (контролируемый переход с инвариантами и откатом) + Software and systems.

---

## Planning frame

### Current state (gap)
- UI на egui/eframe 0.29 (glow). 9 страниц в `rust/src/ui/*_page.rs` + `mod.rs` (App/eframe::App) + `preview.rs` (Preview2D painter + Preview3D) + `renderer3d.rs` (three-d на общем GL-контексте eframe). Шрифты + Swiss-токены + тема реализованы только для egui (`rust/src/ui/design/{fonts,tokens,theme}.rs`).
- Связывание с eframe: `frame.gl()` (mod.rs:295) передаёт GL-контекст в 3D; кастомный `recover_gl_surface_after_move` восстанавливает Windows/OpenGL-поверхность после перемещения окна. Лиценз-менеджер (`licensing/`) — отдельный, без egui, поллинг через `mpsc`-канал каждый кадр (`App::update`).
- egui-поверхность (immediate-mode): `SidePanel`/`CentralPanel`/`Window`/`ScrollArea`, `ComboBox`, `TextEdit`(singleline/password), `button`/`label`/`colored_label`/`checkbox`/`spinner`/`separator`, `RichText`(.strong/.color/.monospace/.small/.heading/.truncate), `on_hover_text`, `Frame::group/none`(fill/stroke/margin/rounding), `Layout::top_down/right_to_left`, `allocate_ui_with_layout`, `add/add_enabled/add_enabled_ui/add_sized`, `ctx.set_style/set_visuals` + `Visuals::dark/light` + text_styles + `FontDefinitions`, `painter`(rect_filled/rect_stroke/line_segment/text/circle_filled/image), drag (`resp.dragged`, `pointer.delta`, `modifiers.ctrl/shift`), `request_repaint_after`, `ViewportBuilder`+`IconData`.
- Cargo: `egui`/`eframe`(glow)/`three-d` 0.18 (glow-only, wgpu-бэкенда нет)/`glow`/`glutin`(dev). iced/wgpu отсутствуют.
- Локально собрано: GNU-тулчейн (rustup stable 1.97 + MinGW WinLibs); MSVC Build Tools без C++-компонента (не удалось elevated-установить). CI — `windows-latest` MSVC (`dtolnay/rust-toolchain`, default = msvc).

### Constraints and priorities
- egui и iced не могут сосуществовать в одном окне/процессе (оба владеют event loop + рендер-контекстом) → стратегия «двойная страница за фичей» НЕ применима; только параллельная сборка нового iced-приложения + big-bang cutover с откатом.
- Инварианты (выше) неизменны; бизнес-логика не переписывается.
- Сетевой диск `\\server\...` не поддерживает атомарный rename `.rlib` → собирать с локальным `--target-dir` (факт 2026-08-03).
- wgpu на Windows: CI (MSVC) — ожидаемо нормально (D3D12/DXIL); локальный MinGW GNU — риск (может требовать DxCompiler/stratum). См. R1 / D-open[toolchain].
- Дизайн-токены (значения) — framework-agnostic, переиспользуются один-в-один (Color32 → iced::Color).

### Scope boundaries
**In:** новый модуль `rust/src/ui2/**` (iced); `main.rs` (переключение/переподключение к iced); `Cargo.toml` (добавить iced/lyon, удалить egui/glow/three-d на cutover); `rust.yml` CI (iced-джобы); пере-выражение дизайн-системы и шрифтов в iced; 2D-канвас (Preview2D, шкала отчёта); 3D-превью (по D-open[3D]); платформенные интеграции (titlebar, иконка, окно, dev-license, буфер обмена, файл-диалоги).
**Out:** `tools/license-admin` (отдельный бинарь, не трогаем); бизнес-логика домена; форматы данных; серверная часть лицензирования; дизайн-ревизия beyond parity (без изменения Swiss-тона).

### Assumptions and open decisions
- **A1 — iced 0.14 wgpu собирается на MSVC CI.** Подтверждено docs.rs (iced 0.14.0, wgpu 27, 06/2026). Gate: W2 (пустой iced-бинарь собирается и запускается).
- **A2 — `licensing/` framework-agnostic.** Проверено: чистый Rust + потоки, без egui-зависимостей → переиспользуется как есть; поллинг канала → `Subscription::run`, асинхронные операции → `Task::perform`.
- **A3 — `rfd` + `arboard` framework-agnostic.** Проверено → переиспользуются без изменений.
- **A4 — значения дизайн-токенов переносятся.** Проверено (Color32-константы в `design/tokens.rs`) → iced::Color те же RGB; механизм привязки (egui Visuals → iced Theme/style) переписывается в W3.
- **D-open[3D] (deferred, LRD = до необратимой реализации W11):** подход к 3D-превью в iced. Recommended **A**. Альтернативы B/C/D — см. раздел «3D decision».
- **D-open[2D-hover] (execution-time):** parity hover/tooltip сегментов в iced Canvas — проверить в W6.
- **D-open[titlebar-dark] (execution-time):** тёмная titlebar в iced (DwmSetWindowAttribute) — проверить в W12; fallback — сырой вызов DWM через HWND winit.
- **D-open[toolchain] (execution-time, LRD=W2):** локальная среда сборки для wgpu — MSVC предпочтителен (требуется C++-компонент VS Build Tools), либо принять, что iced-дев-сборка идёт на MSVC, а локальный MinGW оставлен для egui-этапа.

---

## 3D decision (D-open[3D])

three-d 0.18 — glow-only; текущий `Renderer3D` делит GL-контекст eframe и читает пиксели в egui-текстуру. В iced (wgpu) этот путь ломается. Варианты до начала необратимой реализации W11:

| Path | Подход | Плюсы | Минусы | Риск/объём |
|---|---|---|---|---|
| **A (рекоменд.)** | Переписать 3D-рендерер на wgpu (боксы + плоское Lambert-затенение, как в `renderer3d.rs::face_triangles`) и встроить как iced custom widget (`iced::advanced`) на общем wgpu-устройстве | Нативная интеграция; retains drag-to-rotate; wgpu surface managed iced'ом | Новый wgpu-код (~300–500 LOC); знание `iced_wgpu` custom-primitive | M |
| B | Оставить three-d/glow; рендерить offscreen в **рабочем потоке** с собственным GL-контекстом (`glutin with_any_thread`, pbuffer/surfaceless, НЕ второй EventLoop на main), слать `ColorImage` в iced через канал → `iced::widget::image` | Точные текущие визуалы; меньше нового кода рендера | GL-контекст на потоке на Windows хрупок; поток + GPU + перекладывание пикселей; отдельный glutin (event-loop конфликт) | L |
| C | CPU-софт-рендер боксов (painter's algorithm / z-буфер; `face_triangles` уже считает Lambert CPU) → растровое изображение → iced Image | Нет GPU-зависимости совсем; простая растеризация | Теряет z-буфер-корректность (painter's — достаточно для box-моделей); нужен растеризатор (~150–250 LOC) или крейт | M |
| D | Отложить 3D (отключить затенённый превью; «Экспорт в STEP» работает; 2D-проекции остаются) | Наименьший объём; ранний релиз | Теряет feature (затенённый 3D-превью); против «полностью перейти» | S |

Recommended **A** (нативный wgpu-виджет): лучше всего соответствует «полной миграции», сохраняет drag-to-rotate и скрытие граней, интегрируется с wgpu-бэкендом iced. Gate: до реализации W11 — выбор пути; path C — fallback если A окажется дороже пилота. W10 (страница Step3D) может быть собрана без 3D-части (STEP-экспорт, состояния «нет данных»), пока W11 решает превью.

---

## egui → iced mapping (grounding)

| egui (текущее) | iced 0.14 |
|---|---|
| `eframe::App::update(&mut self, ctx, frame)` | `iced::application(new, update, view)` + `theme` + `subscription`; `update` возвращает `Task<Message>` |
| `ctx.set_style/set_visuals`, `Visuals::dark/light` | `theme(&State) -> Theme` (+ `Theme::custom`) и\или `widget.style(|theme,status| …)` |
| `SidePanel::left(140)` + `CentralPanel` + `ScrollArea` | `row[sidebar(column<nav>), content(scrollable)]` |
| `page_button` (category fill, bold) | `button(text).style(кложюр → category_fill из токенов)` |
| `label`/`colored_label`/`RichText`(.strong/.monospace/.small/.color/.heading/.truncate) | `text`(.font/.size/.style; `text::danger` и т.п.); mono — `Font` (JetBrains Mono) |
| `section_title` (Heading display) | `text` с `Font` Inter-SemiBold + size 16 |
| `TextEdit::singleline(.password/.hint_text)` | `text_input(.secure()/.placeholder())` |
| `ComboBox` | `picklist`/`combobox` |
| `ScrollArea::vertical` | `scrollable` |
| `checkbox` | `checkbox` |
| `add_enabled`/`add_enabled_ui` | условный `.on_press` + disabled-стиль (iced 0.14) |
| `allocate_ui_with_layout`, `Layout::right_to_left` | `row`/`column` + `Container` + alignment |
| `horizontal`/`vertical` | `row`/`column` |
| `add_sized([w,h])` | `.width`/`.height` (`Length::Pixels`/`Fill`) |
| `Frame::group/none`(fill/stroke/margin/rounding=0) | `container.style` (background, `Border { radius: 0, .. }`, padding) |
| `separator` | `Rule` (horizontal/vertical) |
| `spinner` | `widget::spinner` |
| `on_hover_text` | `widget::tooltip` (overlay) |
| `egui::Window` (about, instruction) | in-app overlay (`Pane`/контейнер с backdrop) или отдельное iced-окно (`window` multi-window) |
| `ctx.request_repaint_after(...)` (license poll, gl recovery) | `Subscription` (`time::every` / `Subscription::run` поток канала); gl-recovery **исчезает** (wgpu сам управляет surface) |
| `recover_gl_surface_after_move` | **удаляется** (wgpu surface) — net win |
| Painter 2D (`rect_filled`/`line_segment`/`text`/`circle_filled`/`image`, hover/drag) | `iced::widget::canvas::Program` (lyon `Path::new`/`fill`/`stroke`, `canvas::Text`, `canvas::Image` cache, `mouse_interaction`) |
| Preview3D (three-d/glow render-to-texture + drag) | 3D — D-open[3D] (рекоменд. A: wgpu custom widget) |
| Windows titlebar dark (`DwmSetWindowAttribute` via `windows_dark_mode`) | `iced::window` platform_specific (Windows) + fallback — сырой DWM через HWND winit |
| `IconData` | `iced::window::settings::Icon` |
| `ViewportBuilder` (460×600, resizable=false, always_on_top, title) | `iced::application(...).window(Settings { size, resizable:false, level:AlwaysOnTop, title, .. })` — проверить level/поля в W2/W12 |
| `#![windows_subsystem = "windows"]` | переносится в `main.rs` (framework-agnostic) |
| `HEAT3_DEV_LICENSE` (dev-режим) | переносится (`licensing/` без изменений) |
| `egui`/`eframe`/`glow`/`three-d`/`glutin` (deps) | удаляются на cutover (W14); добавляются `iced`/`iced_wgpu`/`lyon` (W2) |

---

## Strategy and key decisions

- **D1 — Параллельная сборка нового iced-приложения (`ui2/`) + big-bang cutover с откатом.** egui и iced не делят окно/процесс → двойное сосуществование невозможно. **Why:** сохраняет egui-бинарь как эталон/откат до parity; cutover — одной точкой (`main.rs`/default bin) после sign-off. **Revisit if:** cutover затягивается >2× оценки →分段 stratification ( нескольких страниц на iced с fallback к egui-бинарю для остальных) не применима архитектурно — лечится только ранним пилотом (W8) и cohort-расширением (W9/W10).
- **D2 — Пилот = страница «Лицензия».** Самая богатая лестница статусов + async + overlays + text input + dark/light — доказывает Elm-инверсию, Subscription/Task, темизацию и dev-badge за один слайс. **Why:** low blast radius (одна страница, бизнес-логика в `licensing/` уже изолирована). **Revisit if:** найдётся более представительная страница (Check —CheckBox/collapse/ScrollArea, но он требует Canvas/hover, что пилот не покрывает; License наоборот изолирует риски).
- **D3 — Дизайн-значения переносятся, механизм привязки переписывается.** `design/tokens.rs` Color32-константы → iced::Color (те же RGB); `theme.rs`(egui Visuals) и `fonts.rs`(egui FontDefinitions) → iced `Theme::custom`/style-кложуры и `iced::font`. **Why:** повторно использовать результат `2026-08-03-swiss-design-plan.md`. **Revisit if:** iced-Theme не вытягивает нужную гранулярность (статус-лестница, category-fill per-state) → кложуры `widget.style` + `extended_palette` (подтверждено docs.rs).
- **D4 — 3D: recommended path A (wgpu-native), gate до W11.** **Why:** «полностью перейти»; сохраняет drag-to-rotate и z-корректность; нативен для wgpu-бэкенда iced. **Revisit if:** пилот A (W11-try) дороже C → переход на C (CPU-софт). Откат D — только если владелец согласен потерять feature.
- **D5 — CI сначала двойной (egui + iced), затем iced-only.** На время перехода `rust.yml` проверяет оба; после cutover (W14) egui-джобы удаляются. **Why:** сохраняет эталон-бинарь зелёным как откат. **Revisit if:** двойные джобы ломают покрытие/secret-scan → разделить на имена джобов (`rust-client-egui` / `rust-client-iced`) с одним `release-sign` на текущий бинарь.
- **D6 — `licensing/` без изменений; интеграция через `Subscription`/`Task`.** Поллинг канала (egui `poll()`) → `Subscription::run`(`mpsc::Receiver` как поток); `activate`/`refresh`/`deactivate` → `Task::perform`. **Why:** фреймворк-нейтральное ядро переиспользуется (A2). **Revisit if:** Subscription-семантика (декларативный поток, lifecycle) ломает UX текущего poll-on-frame (idle/busy) → выявится в W8.

---

## Work units

### W1. Baseline-инвентарь, инварианты и regression-скелет
**Outcome:** Документированы текущая egui-поверхность, инвентарь функций по 9 страницам, поведенческие инварианты и критерии приёмки parity; создан skeleton ручной + (где применимо) автоматической регрессии для сравнения egui↔iced.
**Advances:** готовность к пилоту; страховка parity (R3).
**Inputs / dependencies:** None.
**Actions:**
- Составить `docs/plans/ui-inventory.md`: per-page список виджетов, взаимодействий (hover/drag/модификаторы), дверей (`page_button`/`section_title`/`show_status`), платформенных хаков (`recover_gl_surface_after_move`, `windows_dark_mode`, titlebar, иконка, окно).
- Зафиксировать инварианты (9 страниц, положения кнопок, размеры, dev-license, буфер/файл, 2D/3D).
- Список behavioral-сценариев по страницам: happy path / boundary / failure (permission/security) / integration / recovery — основа parity-чек-листа.
**Owner:** Unassigned
**Evidence:** `ui-inventory.md` принят владельцем; parity-чек-лист существует.

### W2. iced-scaffold + зависимости + toolchain-gate
**Outcome:** Пустой iced-бинарь (окно 460×600, title, иконка `logocube.ico`, `resizable=false`, always-on-top) собирается и запускается; грузит шрифты Inter/JetBrains Mono; темизирован Swiss-Theme (paper/ink/accent, Rounding::ZERO); feature-flag `ui-iced` переключает `main.rs` между egui и iced; CI имеет iced-джобу.
**Advances:** базовая способность + проверка toolchain/зависимостей (A1, R1).
**Inputs / dependencies:** W1 (инвентарь).
**Actions:**
- `Cargo.toml`: добавить `iced = { version = "0.14", features = ["wgpu"] }` (+ `lyon` через `iced::widget::canvas`), временно сохранить egui/glow/three-d/glutin.
- Создать `rust/src/ui2/mod.rs` + `rust/src/ui2/app.rs` (`iced::application`), `theme/`, `widgets/` (скелеты); `main.rs` — `#[cfg(feature = "ui-iced")]` запуск iced, иначе egui.
- Загрузка шрифтов (`iced::font` из `assets/fonts/*`), включая emoji-fallback (NotoEmoji) — проверить рендеринг.
- `rust.yml`: добавить джобу `rust-client-iced` (fmt/clippy/test/audit) — двойное покрытие.
- Проверить wgpu на CI (MSVC) и локально (MSVC предпочтит.; MinGW → риск R1, зафиксировать D-open[toolchain]).
**Owner:** Unassigned
**Evidence:** iced-окно открывается со шрифтами/темой; визуальное сравнение egui-shell ↔ iced-shell (владелец); CI iced-джоба зелёная.

### W3. Дизайн-токены → iced (сохранение значений)
**Outcome:** `ui2/theme/tokens.rs` (`iced::Color` Palette light/dark, идентичные RGB текущим `design/tokens.rs`), `ui2/theme/mod.rs` (`Theme::custom`/`extended_palette` привязки, status ladder ok/warn/error/info/muted, category fills Paste/Open/Action/Rotate/Mirror/Copy с dark/light), `Rounding::ZERO` (`Border { radius: 0 }`), текстовые стили (heading=Inter-SemiBold 16, body, mono=JetBrains Mono, small).
**Advances:** C2 (design-system parity); переиспользование `2026-08-03-swiss-design-plan.md`.
**Inputs / dependencies:** W2.
**Actions:**
- Перенести значения `LIGHT`/`DARK` Palette из `design/tokens.rs` → iced `Color`; сохранить `category_fill(category, dark)` семантику.
- Определить helper'ы `button_style(category)`, `status_color(state)`, `panel_style`, `section_title`, `status_panel_frame` (background+border+padding).
- Шрифтовые `Font`-константы (Inter Regular, Inter SemiBold=display, JetBrains Mono) + emoji-fallback.
- Проверить контраст status vs paper в dark (WCAG ≥3:1) как в swiss-плане.
**Owner:** Unassigned
**Evidence:** окно-ним из кнопок/текста/статусов side-by-side с egui-версией; владелец: «тон/акцент/статусы совпадают».

### W4. Общие iced-виджеты
**Outcome:** `ui2/widgets/{page_button, section_title, status_panel, nav_button, nav_panel}` повторяют поведение egui-хелперов 1:1.
**Advances:** блок для всех страниц.
**Inputs / dependencies:** W3.
**Actions:**
- `page_button(ui, label, category, bold)` → iced (full-width, category-fill, bold-text); `section_title` (text display 16); `status_panel` (group-frame + статус-цвет + опциональный spinner/message); `nav_panel` (column кнопок, 140 px, panel fill, item spacing 8).
- Согласовать disabled/enabled, hover-стили, spacing 4pt-шкалу (PAGE_SPACING=8).
**Owner:** Unassigned
**Evidence:** diff только по форме/семантике против egui-хелперов (W1); визуальная parity.

### W5. Канвас-тулкит 2D (Canvas/lyon helpers)
**Outcome:** Переиспользуемые примитивы для обновляемых (`Preview2D`, шкала отчёта): fill/stroke rect/path, line, circle, text, image-with-cache; mouse_interaction (hover/drag) → `Message`.
**Advances:** base для W6/W7.
**Inputs / dependencies:** W3.
**Actions:**
- Обёртки над `iced::widget::canvas::Program`: кэш `Geometry`, заливки/обводки (lyon `Path`), `canvas::Text`, `canvas::Image`.
- Сигнальный контракт: drag/hover оборачиваются в `Message` (для rotate-3DTranspose в W11 и hover-меток в W6).
**Owner:** Unassigned
**Evidence:** демо-канвас отрисовывает прямоугольники + текст + реагирует на drag; coverage hover.

### W6. 2D-превью (Preview2D) — Turner, Turner2D
**Outcome:** Canvas-предпросмотр сегментов/rects с проекцией XY/XZ, fills/strokes из категорий материалов, hover-метки имён сегментов, переключатель проекции — parity с `preview.rs::Preview2D`.
**Advances:** W10 (Cohort B).
**Inputs / dependencies:** W5, W4.
**Actions:**
- Переписать `Preview2D::show_segments`/`show_rects` в `ui2/preview2d.rs` (Canvas Program); цвета материалов/тестов — из `tokens`/data (exempt-цвета переносятся как данные).
- Hover → tooltip/overlay (D-open[2D-hover]); drag для future 3D-expose不吃.
**Owner:** Unassigned
**Evidence:** side-by-side turner/turner2d превью; hover parity.

### W7. Отчёт/шкала — Canvas + таблица + PNG
**Outcome:** Цветная шкала материалов (Canvas), таблица с selectable-ячейками, сохранение PNG (`image`), буфер обмена таблицей/выделенным — parity с `report_page.rs`.
**Advances:** завершение отчётной страницы.
**Inputs / dependencies:** W5, W3.
**Actions:**
- Canvas шкалы (цвета материалов из `.mtl` RGB — данные, exempt); `table`/строки-řej-таблица через row/column+кликабельные ячейки (iced 0.14 без встроенной table — построить).
- PNG-экспорт (`image` crate — без изменений); clipboard (`iced::clipboard`/arboard).
**Owner:** Unassigned
**Evidence:** PNG сравним с egui; выбор ячейки + Ctrl+C работает; parity.

### W8. Пилот-страница — Лицензия (Elm-инверсия + async)
**Outcome:** `ui2/license_page.rs` повторяет `license_page.rs`: status_panel (все состояния `LicenseState`), text input ключа (password + reveal), кнопка активации (accent_fill + контрастный текст), деактивация с confirm-frame, overlays, dev-license бейдж; `licensing/` подключён через `Subscription`(поллинг канала) + `Task`(activate/refresh/deactivate).
**Advances:** D2 (пилот); доказательство архитектуры; A2.
**Inputs / dependencies:** W3, W4.
**Actions:**
- Интегрировать `LicenseManager` как состояние приложения; `subscription` испускает `Message::LicensePoll` пока есть работа (или `Subscription::run` по `mpsc::Receiver`); `update` возвращаєт `Task::perform` для активации/refresh/deactivation.
- Confirm-deactivation → overlay (Pane/backdrop); masked key, message, spinner для `!= Idle`.
- Dev-license (`HEAT3_DEV_LICENSE`): отображение бейджа/скрытие форм — parity с egui.
- Поведение idle/busy без busy-spin (D6 revisit).
**Owner:** Unassigned
**Evidence:** owner side-by-side egui↔iced (light + dark) — parity активности/цветов/async; exit gate пилота = «мигрируй остальные».

### W9. Cohort A — статичные страницы (Corner, MaterialSort, AirCavities)
**Outcome:** Три страницы iced parity: масштаб-переход файла/буфера, переупорядочивание ↑↓ материалов, conductivity labels (mono), свотчи штриховки (selected fill = category), name-mask, разбор лога.
**Advances:** расширение (cohort) после пилота.
**Inputs / dependencies:** W4, W8 (паттерн).
**Actions:**
- Per-page: `page_button` calls, `show_status`, `section_title`, файл-диалоги (`rfd`), буфер (`arboard`/iced clipboard), ScrollArea (MaterialSort список переупорядочивания), colored_label stati.
- AirCavities — свотчи штриховки = `button.style` (selected → category_fill + stroke; иначе paper + stroke_soft).
**Owner:** Unassigned
**Evidence:** parity-чек-лист (W1) по трём страницам green; owner визуальная сверка.

### W10. Cohort B — preview/list страницы (Check, Turner, Turner2D)
**Outcome:** Проверка плоскостей/пустот, предложения упрощения (checkbox + collapse + ScrollArea), повороты/отражения + 2D-превью (W6), instruction overlay.
**Advances:** основной объём страниц.
**Inputs / dependencies:** W4, W6.
**Actions:**
- Check: plane-usage (colored error/warn/ok + mono X/Y/Z), cavity, simplify-list (checkbox + collapse via toggle), ScrollArea, action-buttons (category fill), копирование результата.
- Turner/Turner2D: transform-массив (Category::Rotate/Mirror/...), Preview2D (W6), instruction `Window`→overlay, буфер обмена.
**Owner:** Unassigned
**Evidence:** parity-чек-лист green; owner визуальная сверка (light+dark).

### W11. 3D-превью + страница Step3D (gated by D-open[3D])
**Outcome:** Затенённый 3D-превью (drag-to-rotate, z-корректность) + STEP-экспорт (`rfd` save, `step_export::write_step` без изменений) — по выбранному пути D-open[3D] (рекоменд. A).
**Advances:** D4; закрытие feature.
**Inputs / dependencies:** W2, D-open[3D] resolved; часть STEP-экспорта может быть реализована раньше (без 3D).
**Actions (path A, recommended):**
- `ui2/renderer3d_wgpu.rs`: wgpu-рендерер боксов ( Lambert из `renderer3d.rs::face_triangles` — CPU-математика переиспользуется без изменений) + iced custom widget (`iced::advanced`, общий wgpu device из `iced_wgpu`); render target → текстура/примитив; drag (Ctrl=азимут/Shift=элевация) через `Message`.
- page `ui2/step_3d_page.rs`: parse → кол-во boxes → 3D preview + «Экспорт в STEP» (file dialog + status).
- (Path C fallback) если A дороже пилота: CPU-растеризатор боксов → `iced::widget::image`.
**Owner:** Unassigned
**Evidence:** 3D превью вращается/отрисовывает боксы корректно; STEP parity; owner sign-off на 3D-визуал.

### W12. Платформенная интеграция (Windows)
**Outcome:** Тёмная titlebar (по реестру `AppsUseLightTheme`), иконка, always-on-top, fixed-size/non-resizable, `windows_subsystem`, dev-license, буфер обмена, файл-диалоги — parity на Windows.
**Advances:** platform parity (R4, R5).
**Inputs / dependencies:** W2.
**Actions:**
- Titlebar dark: `iced::window` platform_specific (Windows) — предпочтит.; fallback — сырой `DwmSetWindowAttribute` через HWND winit (переиспользовать `windows_dark_mode` логику как утилиту); D-open[titlebar-dark] решается здесь.
- Иконка `logocube.ico` → `iced::window::settings::Icon`; окно 460×600, resizable=false, level=AlwaysOnTop, title «HEAT3 Поворотник v2.0.0».
- `HEAT3_DEV_LICENSE` threading в iced (поведение `licensing/` без изменений).
- Аудит платформ-parity (W1).
**Owner:** Unassigned
**Evidence:** окно с тёмной titlebar в dark/light Windows; иконка; размеры/поведение окно паритетно egui.

### W13. CI + release (iced)
**Outcome:** `rust.yml` проверяет iced (fmt/clippy --all-targets -D warnings/test/audit) + `release-secret-scan` (iced-бинарь без секретов, dev-license sentinel не течёт) + `release-sign`/package; `audit-windows.ps1` покрывает новый граф.
**Advances:** D5; release-readiness.
**Inputs / dependencies:** W2 (iced-джоба есть), W11+ (полное покрытие).
**Actions:**
- Дополнить/переименовать джобы; прод-бинарь собирается БЕЗ `HEAT3_DEV_LICENSE` (secret-scan sentinel); dev-бинарь — с ним (опционально в release-артефакт или локально).
- `audit-windows.ps1` — обновить пути/граф под iced.
- Подпись/упаковка (`tools/release/sign-and-package.ps1`) для iced-бинаря.
**Owner:** Unassigned
**Evidence:** CI iced зелёный; secret-scan pass; sign/package отработан (или отчёт unsigned).

### W14. Cutover — переключение на iced, удаление egui/glow/three-d (rollback-tag)
**Outcome:** `main.rs`/default-bin запускают iced по умолчанию; `ui/` (egui) удалён; `egui`/`eframe`/`glow`/`three-d`/`glutin` убраны из `Cargo.toml`; `recover_gl_surface_after_move` удалён; тег `rollback/egui` сохранён.
**Advances:** точка невозврата (после parity sign-off).
**Inputs / dependencies:** W3–W12 + owner parity sign-off (все 9 страниц, 2D/3D, license, платформы) + CI green.
**Actions:**
- Удалить фичу-переключатель; `main.rs` — только iced.
- Удалить `rust/src/ui/**` (egui) и `src/ui/design/{theme,fonts}.rs` (egui-привязки); токены уже в `ui2/theme` (W3).
- `Cargo.toml`: убрать egui/eframe/glow/three-d/glutin; оставить iced/lyon/wgpu (через iced).
- git-тег `rollback/egui-last` (или backup-бинарь) как откат до W15.
**Owner:** Owner (cutover authority)
**Evidence:** единственный бинарь — iced; `cargo tree` без egui/glow/three-d; regression green.

### W15. Стабилизация + завершение legacy + docs
**Outcome:** Финальная регрессия green; egui-путь архивирован/retired; README/планы/инвентарь актуализированы; `ui-inventory.md` закрыт; слежение стабильности.
**Advances:** готовность к эксплуатации.
**Inputs / dependencies:** W14.
**Actions:**
- Полная регрессия по parity-чек-листу (W1): 9 страниц light+dark, 2D/3D, лицензирование (real + dev), буфер/файл, titlebar, окно (move/resize/always-on-top), CI/hygiene green.
- Документы: обновить README (`iced`), закрыть `ui-inventory.md`, добавить заметку в `2026-08-03-swiss-design-plan.md` о переносе токенов в iced.
- Снять rollback-тег после согласованного периода стабилизации (владелец).
**Owner:** Owner
**Evidence:** regression-отчёт green; owner accepts iced-release; тег снят/retired.

---

## Sequence and milestones

| Milestone | Evidence-backed state | Requires | Decision / exit gate |
|---|---|---|---|
| M1 | iced-оболочка запускается (шрифты, Swiss-тема, окно 460×600) | W1, W2 | Owner: «iced-оболочка работает, wgpu-тулчейн ОК» (D-open[toolchain] resolved) |
| M2 | Дизайн-токены/шрифты + общие виджеты parity в iced | W3, W4 | Owner side-by-side: «тон/статусы/категории совпадают» |
| M3 | Пилот (Лицензия) parity в iced (light+dark, async, dev-badge) | W8 | Owner: «пилот принят, мигрируй остальные» |
| M4 | 2D-канвасы parity (Preview2D, отчёт/шкала) | W5–W7 | Owner визуальная сверка |
| M5 | Все 9 страниц parity | W9, W10, W11 | CI iced зелёный + owner parity по чек-листу |
| M6 | Платформы + CI/release parity | W12, W13 | CI green; sign/secret-scan pass |
| M7 | Cutover: iced — единственный бинарь; egui удалён | W14 | Owner cutover sign-off; rollback-tag сохранён |
| M8 | Стабилизация green; legacy retired | W15 | Owner accepts; rollback-tag снят |

Критический путь: W1→W2→W3→W8(пилот)→{W9,W10}→W14→W15. W11 (3D) и W6/W7 (Canvas) подведите в M4/M5, но не блокируют пилот. W12/W13 параллельны после W2. **Точка невозврата** — W14 (только после owner parity sign-off M5/M6).

---

## Risks and contingencies

| Risk / trigger | Prevention | Detection | Response / recovery | Owner |
|---|---|---|---|---|
| wgpu на локальном MinGW GNU не собирается/не запускается (R1) | W2 проверяет первым; CI на MSVC — source of truth | W2 iced-бинарь не линкуется/не запускается локально | локальная разработка на MSVC (поставить C++-компонент VS Build Tools elevated или neighbor admin); MinGW оставить для egui-этапа | Executor |
| 3D в iced (path A) дороже пилота /wgpu custom-widget сложности (R2) | W11-try как изолированный спайк ДО cutover; D-open[3D] gate | спайк W11 не даёт вращаемого превью за бюджет | переключиться на path C (CPU-софт, painter/z) — parity визуала «достаточно»; или D (отложить) по решению владельца | Owner + Executor |
| Parity-дрифт immediate→Elm: hover/drag/модификаторы/shortcuts утрачены (R3) | W1 инвентарь + per-page parity-чек-лист; pilot gates | owner parity-сверка fail по странице | доведение конкретной страницы до parity; не cutover до M5 green | Owner |
| Тёмная titlebar в iced не first-class (R4) | W12 спайк ранний; fallback — сырой DWM через HWND winit | titlebar не тёмнеет в dark-Windows | оставить `windows_dark_mode` как утилиту + `DwmSetWindowAttribute` напрямую через winit HWND | Executor |
| always-on-top / fixed-size / non-resizable parity в iced (R5) | W2 проверяет `window::Settings` (level/resizable/size) сразу | окно ресайзится/не topmost | iced 0.14 поддерживает level + resizable — сверить; иначе platform-specific | Executor |
| Big-bang cutover без промежуточного parity (R6) | egui-бинарь остаётся рабочим/релизным до W14; rollback-tag | owner не подписывает M5/M6 | не cutover; доработать cohort; откат к egui-тегу релиза | Owner |
| Дизайн-fidelity дрейф (R7) — результат swiss-плана не переносится | W3 переносит ЗНАЧЕНИЯ 1:1; side-by-side при M2 | owner «тон не тот» | правка `ui2/theme` без изменения значений токенов | Owner |
| Производительность/редро (R8) — 3D-drag требует кадры; Canvas-редро | Subscription/animation для drag; Canvas cache | лаг/drag идёт рывками | `time::every`/animation module; geometry cache | Executor |
| Шрифтовой parity — emoji-fallback/Inter+JetBrains (R9) | W2/W3 грузят все TTF + NotoEmoji fallback | глиф □/не тот шрифт | cosmic-text `Family fallback`; до настроить `Font` fallback | Executor |
| License Subscription-семантика ломает UX (R10) — busy-spin/idle | W8 пилот проверяет poll/Task lifecycle | CPU/UI thermal или частые сообщения | `Subscription` с корректной декларативной lifecycle; `_subscription = …` only when operation != Idle | Executor |

---

## Validation and definition of done

- [ ] iced-бинарь (release) — единственный релиз; `egui`/`eframe`/`glow`/`three-d`/`glutin` удалены из `Cargo.toml` (`cargo tree` подтверждает).
- [ ] Все 9 страниц parity (свет + dark) по parity-чек-листу W1; положения кнопок/семантика не изменены (guardrails).
- [ ] 2D-превью (Preview2D) и шкала отчёта parity; hover/drag ведут как в egui.
- [ ] 3D-превью функционирует (path A/B/C/D из D-open[3D] реализован и принят).
- [ ] Лицензирование: реальный серверный поток (activate/refresh/deactivate/offline-lease) + dev-license режим работают parity.
- [ ] Платформы: тёмная titlebar, иконка, окно (460×600, resizable=false, always-on-top, windows_subsystem).
- [ ] CI зелёный по iced: `fmt`, `clippy --all-targets -D warnings`, `test`, `audit`; hygiene; `release-secret-scan` pass; `release-sign`/package отработан.
- [ ] Откат-тег `rollback/egui-last` сохранён до периода стабилизации; затем retired по решению владельца.
- [ ] Документы актуальны (README→iced, `ui-inventory.md` закрыт, swiss-план дополнен заметкой о переносе токенов).
- [ ] D-open[3D]/[2D-hover]/[titlebar-dark]/[toolchain] resolved или явно accept.

---

## Open decisions and re-planning triggers

- **D-open[3D] (deferred, LRD = до имплементации W11):** путь 3D. Recommended A. Trigger re-plan: спайк A не даёт вращаемого превью за бюджет → C; владелец согласен потерять feature → D.
- **D-open[2D-hover] (execution-time, LRD=W6):** parity hover/tooltip в Canvas.
- **D-open[titlebar-dark] (execution-time, LRD=W12):** iced platform-specific vs сырой DWM.
- **D-open[toolchain] (execution-time, LRD=W2):** локальная среда для wgpu (MSVC vs MinGW).

**Re-planning triggers:** wgpu не собирается на CI MSVC → A1 invalidated → fallback на MSVC-only/альтернативу; owner отклоняет parity конкретной страницы после M3/M5 → добавить work-unit правки (не откат всего); three-d/wgpu спайк A провален → path C; cutover затягивается >2× оценки → re-split cohort­ы (но не сосуществование egui+iced); secret-scan ловит dev-sentinel в прод-бинаре → fix gating флагов в W13.

---

## Handoff

**Start with:** W1 (baseline-инвентарь + инварианты + parity-чек-лист) — нет блокеров.
**Before starting W11:** resolve D-open[3D] (выбрать путь A/B/C/D).
**Plan owner:** Unassigned (исполнитель — AI/владелец).
**Next review:** M1 (iced-оболочка + toolchain-gate) — owner visual + W2 запускается.
**Done means:** iced-бинарь — единственный релиз; parity по 9 страницам + 2D/3D + license + платформы; CI/hygiene green; egui/glow/three-d удалены; rollback-tag до стабилизации (M8) — owner sign-off.
**Preserved contracts:** бизнес-логика домена, `licensing/` (без изменений), `rfd`/`arboard`, форматы данных, `tools/license-admin`; инварианты страницы/кнопки.
**Cutover authority:** Owner (W14) после M5/M6 sign-off; rollback = egui-тег/бинарь.