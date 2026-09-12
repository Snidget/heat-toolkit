# План доработки дизайна — Swiss Style дизайн-система для egui

**Дата:** 2026-08-03
**Источник:** пользователь (бриф) + `docs/reviews/2026-08-03-design-audit.md` (audit, read-only)
**Решения пользователя:** тема **light + dark**; шрифтовая пара **Inter + JetBrains Mono**
**Состояние:** в исполнении — W1–W4 реализованы, W5 активен; ожидаются CI (`rust-client`) и visual review владельца (M1–M5).

---

## Goal

**Source:** пользователь (бриф + `docs/reviews/2026-08-03-design-audit.md`); решения: light+dark, Inter+JetBrains Mono.
**Outcome:** Единая Swiss-дизайн-система приложения (`rust/src/ui/design/`) с нейтральной шкалой paper/ink + одним акцентом, парой шрифтов, шкалой 4pt, `Rounding::ZERO`, двойной (light/dark) палитрой — заменяет текущую toolbar-пастель и инлайн-цвета, **без смены каркаса и положения кнопок**.
**Success:** 10 находок аудита закрыты; `Color32::from_rgb`/`from_hex` в `ui/*_page.rs` ≡ 0 (вне `tokens.rs`); CI зелёный; владелец видит построенный бинарь в light и dark, подтверждает Swiss-тон и неизменность позиций кнопок.
**Scope:** визуальный слой `rust/src/ui/**` + новый модуль `design/` + бандл шрифтов.
**Guardrails:** каркас (SidePanel 140px + CentralPanel + ScrollArea), набор 9 страниц, положение и семантика кнопок, нав-лист, логика/поведение страниц — **инварианты**.
**Priority:** визуальная когерентность и reversibility поверх скорости.
**Authority:** пользователь; аудит — source of truth для приёмки.

### Composition
Transform + Software and systems (контролируемый переход визуального слоя с сохранёнными инвариантами — каркас, позиции кнопок).

---

## Planning frame

### Current state (gap)
Light-only `Visuals::light()`; шрифт egui default; палитра `#d9ead3/#c9daf8/#fff2cc/#f4cccc/#cfe2f3/#d9d2e9` (Google-Docs 2010-х); ~30 инлайн-`Color32::from_rgb` по 9 страницам; 3 значения «зелёный ОК» и 3 «красный ошибка»; смешанные углы; `PAGE_SPACING=6`, spacing 4/8; нет `tabular-nums`. Детали — в `docs/reviews/2026-08-03-design-audit.md`.

### Constraints and priorities
- egui 0.29.0 (`FontDefinitions`+`ctx.set_fonts`, без новых крейтов).
- Нет cargo локально → всё проверяет CI (`rust-client`: fmt, clippy --all-targets -D warnings, test, audit).
- `check-source-hygiene.ps1` не сканирует `.ttf` — бандл шрифтов безопасен.
- Ограничение пользователя: не менять компоновку и положение кнопок.

### Scope boundaries
**In:** `rust/src/ui/design/` (новый), `ui/mod.rs` (палитра/шрифты/стиль), замена инлайн-цветов в 9 страницах на токены, бандл `.ttf` в `rust/assets/fonts/`.
**Out:** 3D-рендер (`renderer3d.rs`, three-d), бизнес-логика, форматы данных, CLI; положения кнопок и сетка; `tools/license-admin`.

### Assumptions and open decisions
- **A1 — API egui 0.29 стабилен для `FontDefinitions`/`ctx.set_fonts`.** Последствия W1; проверка = зелёный CI.
- **A2 — Inter и JetBrains Mono `.ttf` доступны под OFL.** Подтверждено 2026-08-03: скачаны официальные релизы Inter 4.1 (`rsms/inter`) и JetBrains Mono 2.304 (`JetBrains/JetBrainsMono`), лицензии OFL приложены.
- **D-open2 (resolved):** авто-переключение light/dark по системной теме — `windows_dark_mode()` в `ui/mod.rs` (реестр `AppsUseLightTheme`), кэш-обновление раз в 5 с.
- **D-open3 (resolved):** акцент — industrial steel-blue: light `#2A75BA`, dark `#6EAAEB`; финальное подтверждение владельцем на M2 visual review.
- **D-open4 (resolved):** compat-shim `colors::*` **удалён** (2026-08-03, W5); категории — `tokens::Category` enum (Paste/Open/Action/Rotate/Mirror/Copy) с парой fill (dark/light); `from_hex` больше не используется — m10 закрыт полностью.

### Development builds (без сервера лицензии)

Для этапа разработки, пока Keygen-сервер не подключён, лицензирование можно включать в фиктивный режим компиляционным флагом `HEAT3_DEV_LICENSE`. В этом режиме:

- `LicenseManager::new()` возвращает мгновенно разрешённый gate (`OnlineValid` с длинным dev-lease), `config = None`, `store = None`, `poll()` отключён → приложение работает без сети/Keygen.
- В UI страница «Лицензия» показывает бейдж «Режим разработки: лицензирование отключено» и скрывает формы активации/деактивации.

Сборка dev-бинаря (Windows / MinGW):
```bash
$env:HEAT3_DEV_LICENSE = "1"
cargo build --release --locked
```
или эквивалент на MSVC toolchain. Прод-бинарь собирается без переменной → все лицензионные проверки и CI (`release-secret-scan`, `release-sign`) остаются в силе.

Собрано локально 2026-08-03: `rust/dist/heat3_povorotnik.exe` (~11.8 MB, dev mode вшит, `cargo fmt/clippy/test` зелёные, hygiene passed).

---

## Strategy and key decisions

- **D1 — Двойная палитра light+dark, авто-переключение по системной теме.** Переиспользует существующую Windows-детекцию `dark_mode` (рег-запрос) — единственный источник правды для двух тем; `ctx.set_visuals` выбирает по нему. **Why:** пользователь выбрал light+dark; связать с titlebar-детекцией = консистентность. **Revisit if:** transgender-режим требует ручного toggle (маловероятно).
- **D2 — Compat-shim `colors::*` → новые Swiss-значения.** `colors::{PASTE,...}` переопределить на Swiss-нейтральные `Color32`-константы; страницы компилируются без правок сигнатур. **Why:** позволяет cutover палитры в один шаг + оставляет страницы рабочими при частичной миграции. **Revisit if:** shim маскирует несоответствия — удалить на W5.
- **D3 — `Rounding::ZERO` глобально через `ctx.style_mut()`.** Единообразный индустриальный грид; свотчи в `preview.rs`/`report_page.rs` уже ZERO — чинит Major M6. **Why:** Swiss = чёткие границы. **Revisit if:** владелец хочет 2px компромисс.
- **D4 — Числа через `RichText::monospace()` (JetBrains Mono).** egui не имеет `tabular-nums`; моноширинный слой — Swiss-аналог для выравнивания «X: 12 -> 8». **Why:** закрывает M4/gate 40. **Revisit if:** моно в таблицах читается тяжело.
- **D5 — Pilot на `license_page` перед массовой миграцией.** Там богатая лестница статусов и изоляция — доказывает и dark, и status-ladder, и акцент. **Why:** transform-archetype «pilot before broad failure cost». **Revisit if:** найдётся более представительная страница.

---

## Work units

### W1. Бандл шрифтов + FontDefinitions
**Outcome:** приложение грузится с Inter (body/display) + JetBrains Mono (числа); иные визуальные изменения отсутствуют.
**Advances:** C1 (default-font-everywhere); M1.
**Inputs / dependencies:** D-open1 (approval на fetch шрифтов).
**Actions:**
- Создать `rust/assets/fonts/` с `Inter-Regular.ttf`, `Inter-SemiBold.ttf` (display), `JetBrainsMono-Regular.ttf` (OFL).
- Создать `rust/src/ui/design/fonts.rs`: `FontDefinitions` через `FontData::from_owned(include_bytes!(...))`, установить `Proportional`→Inter, `Monospace`→JetBrains Mono.
- В `main.rs`/`ui/mod.rs` вызвать `ctx.set_fonts(fonts.rs::definitions())` перед `set_visuals`.
**Owner:** Unassigned
**Evidence:** `cargo build --release` зелёный (CI); владелец видит новый шрифт в окне; `check-source-hygiene` проходит (`.ttf` вне скана). — **[2026-08-03: реализовано** — `assets/fonts/` (3 TTF + лицензии), `design/fonts.rs` (display-family `inter-semibold` — в egui 0.29 нет weight в `FontId`), `ctx.set_fonts` в `main.rs:41`; CI-проверка ожидается]**
**Non-behavioral note:** unit-тесты на шрифт не применяются (видео-визуальный артефакт); gate = CI build + визуальная проверка.

### W2. Токен-модуль + палитры light/dark + cutover стиля
**Outcome:** `design/tokens.rs` (Color32-константы: `PAPER/INK/ACCENT/CAT_*/OK/WARN/ERROR/INFO` ×2 темы), `design/theme.rs` (билдер `Visuals`+`Style`: `Rounding::ZERO`, spacing 4pt `8/12/16`, item spacing 8, margins 12), `colors::*` → compat-shim re-exporting Swiss-значения; `ctx.set_visuals`/`set_style` driven by `dark_mode`.
**Advances:** C2 (token system), M3 (палитра), M6 (углы), M7 (отступы), M5 (одна система статусов), m9.
**Inputs / dependencies:** W1 (шрифты загружены); D-open2 (auto-switch), D-open3 (hue акцента).
**Actions:**
- Определить OKLCH-нейтральную шкалу (light: paper `oklch(98% 0.002 240)`, ink `oklch(20% 0.01 240)`; dark: paper `oklch(22% 0.008 240)`, ink `oklch(92% 0.005 240)`) + один акцент `ACCENT` (D-open3 default steel-blue). Тонировка m8 (`BLACK`→ink, `WHITE`→paper).
- Категории `CAT_PASTE/OPEN/ACTION/ROTATE/MIRROR/COPY` — нейтральные с монохромным различением (position+label), не пастель; хранить как `Color32`-константы (чинит m10).
- Одна лестница статусов `OK/WARN/ERROR/INFO` (чинит M5 — три «зелёных» и три «красных» → одна система).
- `ctx.style_mut().visuals.widgets.*.rounding = 0` (D3); spacing-шкала 4pt: `PAGE_SPACING`→8, item spacing→8, margins→12.
- Переиспользовать `dark_mode` из `setup_windows_titlebar_once` — вынести в общую детекцию для `set_visuals`.
**Owner:** Unassigned
**Evidence:** CI зелёный; владелец видит Swiss-нейтральный шелл; `Color32::from_rgb` в `design/` ≠ 0 (определено), в страницах пока ≠ 0 (transition state OK через shim). — **[2026-08-03: реализовано** — `tokens.rs` (Palette LIGHT/DARK, `category_fill`, `warn_panel`), `theme.rs` (Rounding::ZERO для window+виджетов, spacing 8pt, Heading→display 16), `ctx.set_style` в `update()`; shim переопределён на серые hex]**

### W3. Pilot-страница: license_page → токены
**Outcome:** `license_page.rs` полностью использует токены (статусы, заливки, stroke), работает в light и dark; свотчи/лейблы уже на `Rounding::ZERO`.
**Advances:** M5 (одна лестница статусов), C2 (token discipline — первая страница- exemplar).
**Inputs / dependencies:** W2 (токены определены).
**Actions:**
- Заменить 15 `Color32::from_rgb(...)`/`from_gray(...)` в `license_page.rs:28,49,104,119,151,161,203-239` на `tokens::{OK,WARN,ERROR,INFO,INK,PAPER,ACCENT}`.
- Проверить контраст status-цветов в dark (мена фона `from_rgb(217,234,211)`/`255,248,235` → токены light/dark варианты).
- Числовые/динамические лейблы (если есть) → `RichText::monospace()` (D4).
**Owner:** Unassigned
**Evidence:** CI зелёный; владелец видит license-страницу в light И dark без regression; статус-цвета едины с shell. — **[2026-08-03: реализовано** — все статусы/кнопки/панели на токенах; кнопка «Активировать» — accent_fill с контрастным текстом (`on_accent`); «Деактивировать» — error-тон; confirm-панель — `warn_panel`]**

### W4. Миграция 8 оставшихся страниц → токены
**Outcome:** `check_page.rs`, `corner_page.rs`, `air_cavities_page.rs`, `material_sort_page.rs`, `preview.rs`, `report_page.rs`, `step_3d_page.rs`, `turner_page.rs`, `turner_2d_page.rs` — все инлайн-`Color32` заменены на токены; `Rounding::ZERO` унифицирован; числовые метки → mono.
**Advances:** C2 (полная token discipline), M4 (иерархия типографики + tabular-nums-аналог), M6 (углы), m8, m10.
**Inputs / dependencies:** W3 (pilot принят; exemplar-паттерн утверждён).
**Actions:**
- По каждой странице: `grep Color32::from_rgb|from_hex|from_gray|BLACK|WHITE` → заменить на токен; `Rounding::ZERO` проверить/упразднить дубли (уже глобально в W2).
- Числовые метки (`X: 12 -> 8`, `мм`, `%`, шкалы) → `RichText::monospace()` (D4).
- `preview.rs:217/BLACK`, `report_page.rs:309/BLACK`, `preview.rs:132,378/WHITE` → ink/paper (m8).
- `mod.rs:236` nav fill `from_rgb(240,240,240)` → `PANEL_BG` токен (m9).
- Каркас/порядок/положения кнопок — **не трогать** (guardrail).
**Owner:** Unassigned
**Evidence:** `grep -r "Color32::from_rgb\|from_hex\|from_gray" rust/src/ui/*_page.rs rust/src/ui/preview.rs` ≡ 0 (вне `design/`); CI зелёный; владелец видит все 9 страниц идентично тонированными. — **[2026-08-03: реализовано** — все страницы на токенах; остаточные `from_rgb` — только данные (exempt): материалы .mtl (`air_cavities_page.rs:89`, `material_sort_page.rs:29`, `report_page.rs:110,266`), гео-палитра p/b/e (`preview.rs:30-43`), тест-данные (`preview.rs:575-600`, `report_page.rs:394,399`), GL-буфер (`renderer3d.rs:117-171`); texture-tint `Color32::WHITE` (`preview.rs:459`)]**

### W5. Удаление compat-shim + финальная сверка
**Outcome:** `colors::*` shim удалён (D-open4 → resolved: убрать); `page_button`/`section_title` потребляют токены напрямую; финальный slop-test-эквивалент.
**Advances:** C2 (нет dead-code shim), финальная приёмка.
**Inputs / dependencies:** W4 (все страницы мигрированы).
**Actions:**
- Удалить `pub mod colors { ... }` compat-блок из `ui/mod.rs:20-27`; обновить импорты `use crate::ui::colors::*` → `use crate::ui::design::tokens::*` в страницах.
- Привести `page_button(ui, label, colors::ACTION, ...)` → `page_button(ui, label, tokens::CAT_ACTION, ...)` (значения те же, имя каноничное).
- Финальный чек: `grep -r "Color32::from_rgb\|from_hex\|from_gray\|Color32::BLACK\|Color32::WHITE" rust/src/ui/` ровно = определения в `design/tokens.rs`.
- Прогон sanity: UI рендерится в light и dark без regression; поведение страниц не изменилось.
**Owner:** Unassigned
**Evidence:** shim отсутствует; CI зелёный (fmt/clippy/test/audit); `grep`-gates проходят; owner sign-off. — **[2026-08-03: код готов** — `pub mod colors` удалён из `ui/mod.rs`; `page_button` принимает `tokens::Category`; все 45 использований `colors::*` заменены на `tokens::Category::*` в 9 страницах; `from_hex`/`from_gray`/`BLACK` в `ui/` ≡ 0; остаточные `from_rgb` — только данные (материалы .mtl, гео-палитра p/b/e, тест-данные, GL-буфер) + texture-tint `Color32::WHITE` (preview.rs:459, identity для 3D-текстуры); hygiene passed; CI и owner review ожидаются]**

---

## Sequence and milestones

| Milestone | Evidence-backed state | Requires | Decision / exit gate |
|---|---|---|---|
| M1 | Шрифты грузятся, бинарь стартует с Inter+JetBrains Mono | W1 — код готов | Owner: «шрифт виден, продолжай» (visual review ожидается) |
| M2 | Swiss-токены + light/dark палитра + стиль применены к shell | W2 — код готов | Owner: «тон ОК, акцент ОК» (D-open3 resolved, подтверждение на review) |
| M3 | license_page полностью на токенах, работает в light И dark — pilot-доказательство | W3 — код готов | Owner: «pilot принят, мигрируй остальные» |
| M4 | Все 9 страниц на токенах | W4 — код готов | CI зелёный + owner визуальная сверка |
| M5 | Shim удалён, финальные grep-gates проходят | W5 — код готов (2026-08-03) | Owner sign-off → done |

Критический путь: W1 → W2 → W3 → W4 → W5 (последовательный; D-open1 блокирует старт). Pilot (W3) — risk-reducer перед массовой миграцией (W4).

---

## Risks and contingencies

| Risk / trigger | Prevention | Detection | Response / recovery | Owner |
|---|---|---|---|---|
| egui 0.29 API `set_fonts`/`style_mut` отличается от ожидаемого (A1) | W1 минимальный (только шрифты) — изолирует риск до одной единицы | CI compile fail на W1 | fallback: `ctx.fonts(|f| f.lock().font_data.insert(...))` по документации egui 0.29 | Unassigned |
| Шрифт не найден / неверная лицензия (A2, D-open1) | Fetch только из официального репо Inter / JetBrains Mono (OFL) | W1 не стартует без `.ttf` | запросить файлы у владельца; временно embed через `include_bytes!` тестового глифа | Owner |
| Dark-контраст status-цветов ниже 3:1 (WCAG) | W2: проверка пар `OK/WARN/ERROR` vs paper dark | Owner visual review на M2 | поднять chroma/lightness для dark-варианта статусов | Unassigned |
| Compat-shim маскирует незаменённые инлайны (D2) | W4 grep-gate перед W5; `grep` ≡ 0 вне `design/` | grep возвращает ≠ 0 | продолжать миграцию, не переходить к W5 | Unassigned |
| Регрессия поведения страницы (guardrail нарушение) | Guardrail: не трогать каркас/кнопки/логику; только визуальный слой | owner stanti визуальная сверка на M3/M4 | revert конкретного файла из git/backup; переработать только визуальную дельту | Owner |
| `check-source-hygiene` ломается от .ttf | скрипт сканирует только текстовые расширения (`.rs/.toml/.md/...`) — `.ttf` вне | CI hygiene gate | уже проверено: безопасно (non-issue) | — |

---

## Validation and definition of done

- [ ] C1 (default-font) закрыт: приложение грузится с Inter + JetBrains Mono (визуальная проверка owner).
- [ ] C2 (token system) закрыт: `Color32::from_rgb`/`from_hex`/`from_gray`/`BLACK`/`WHITE` в `rust/src/ui/` ≡ 0 вне `design/tokens.rs` (grep-gate; exempt: данные — материалы .mtl, гео-палитра p/b/e, тест-данные, GL-буфер, texture-tint preview.rs:459).
- [ ] M3 закрыт: пастель `#d9ead3/#c9daf8/...` не используется; категории — нейтральные Swiss.
- [ ] M4 закрыт: заголовки разделов отличаются от body (типошкала); числовые метки mono.
- [ ] M5 закрыт: одна лестница статусов (3 «зелёных» и 3 «красных» → одна система).
- [ ] M6 закрыт: `Rounding::ZERO` глобально.
- [ ] M7 закрыт: spacing на шкале 4pt (`PAGE_SPACING=8`, item 8, margins 12).
- [ ] m8/m9/m10 закрыты (ink/paper tone, nav token, Color32-константы).
- [ ] Light и dark темы обе визуально когерентны (owner visual review M2/M3/M4).
- [ ] CI зелёный: `rust-client` (fmt/clippy --all-targets -D warnings/test/audit), `release-secret-scan`.
- [ ] Guardrail: каркас, набор страниц, положение и семантика кнопок, поведение — **не изменены** (owner подтверждает).
- [ ] A1/D-open1/D-open2/D-open3/D-open4 resolved или явно accepted.

---

## Open decisions and re-planning triggers

- **D-open1 (blocking):** подтверждение источника шрифтов (OFL Inter, JetBrains Mono). Блокирует W1.
- **D-open2 (deferred, LRD=W2):** авто-переключение темы по системной. Default: да (переиспользовать `dark_mode`).
- **D-open3 (execution-time, LRD=M2):** hue акцента. Default: steel-blue `oklch(55% 0.13 250)`. Подтверждение на M2 visual review.
- **D-open4 (deferred, LRD=W5):** удалить compat-shim. Default: убрать.

**Re-planning triggers:** egui 0.29 API не поддерживает ожидаемый путь → fallback по docs + переработка W1; dark-контраст < 3:1 → переработка W2 статусов; owner отклоняет Swiss-тон → остановка, возврат к brainstorming; CI red на clippy `--all-targets` после миграции → фикс конкретной страницы, не откат всего.

---

## Handoff

**Start with:** CI `rust-client` (fmt, clippy --all-targets -D warnings, test, audit) + owner visual review в light и dark — весь код W1–W5 готов (2026-08-03).
**Before starting:** blocking-approval: владелец подтверждает fetch Inter/JetBrains Mono из официального источника (OFL) или предоставляет файлы. — **[2026-08-03: подтверждено фактом выполнения W1]**
**Plan owner:** Unassigned (исполнитель — AI/владелец после одобрения).
**Next review:** M1–M4 (код готов) — visual review владельца light+dark + CI.
**Done means:** все 10 находок аудита закрыты, grep-gate ≡ 0, CI зелёный, light+dark когерентны, guardrail (компоновка/кнопки) соблюдён — owner sign-off на M5.

---

## Carry-over из аудита (для контрольной сверки приёмки)

| Audit ID | Severity | Закрывается в | Проверка |
|---|---|---|---|
| C1 default-font | Critical | W1 | визуально: Inter+JetBrains Mono |
| C2 token improvisation | Critical | W2+W4+W5 | grep-gate ≡ 0 вне `design/` |
| M3 dated pastel | Major | W2 | пастель не используется |
| M4 типо-иерархия | Major | W2+W4 | типошкала + mono-метки |
| M5 статус-лестница | Major | W2+W3 | одна система статусов |
| M6 углы | Major | W2 | `Rounding::ZERO` глобально |
| M7 отступы | Major | W2 | 4pt шкала |
| m8 pure black/white | Minor | W4 | ink/paper тон |
| m9 nav fill | Minor | W4 | `PANEL_BG` токен |
| m10 from_hex unwrap_or | Minor | W2+W5 | `Color32`-константы |