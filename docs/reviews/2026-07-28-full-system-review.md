# Полное системное ревью HEAT3 «Поворотник» v2.0

**Снимок:** рабочее пространство по состоянию на 2026-07-28
**Объём:** весь репозиторий (`rust/`, `tools/license-admin/`, CI, документация)
**Основание:** предыдущее ревью `2026-07-17-keygen-licensing-security-review.md` (R1–R11), архитектурный план, Best Practices Rust 2021 edition, Windows-специфика
**Метод:** последовательное чтение исходников главным агентом (скаут-субагенты оказались недоступны из-за 503 на апстриме)

## Вердикт

**Система в целом готова к staging. Production по-прежнему заблокирован открытыми пунктами предыдущего ревью (R6 cgmath, R7 staging E2E, подпись/инсталлятор).**

Доменная логика (геометрия, материалы, модельные проверки) реализована аккуратно и с понятными инвариантами. Лицензионный стек — один из самых проработанных участков кода: fail-closed на каждом шаге, zeroization секретов, атомарные записи, детальная криптографическая верификация. UI-слой функционален, но содержит несколько хрупких workaround-ов вокруг Windows/OpenGL, заслуживающих рефакторинга до production, плюс архитектурный момент: большинство страниц выполняют доменную логику синхронно в UI-потоке каждый кадр (F14), тогда как `check_page.rs` и `LicenseManager` уже реализовали правильный паттерн кэша/фонового worker.

Новых критических или высоких уязвимостей в лицензионных путях не обнаружено. Находки: 4 medium (числовая устойчивость, вырожденные боксы, unsafe-FFI, синхронная доменная логика в UI) и 9 low/info — O(n²) на горячих путях, дублирование кода, хардкод строк, дубликаты версий крейтов.

## Карта находок

| ID | Severity | Подсистема | Краткое описание |
|---|---|---|---|
| F1 | Medium | model_check | `partial_cmp(...).unwrap()` panic на NaN-координатах |
| F2 | Medium | renderer3d | `(face_center - center).normalize()` panic на вырожденных боксах |
| F3 | Medium | ui/mod | Хрупкий unsafe FFI (`FindWindowW`/`DwmSetWindowAttribute`) без windows-sys |
| F4 | Low | transforms | `mirror_xy_y` и `mirror_xz_z` — идентичные функции (намеренно, но недокументировано) |
| F5 | Low | model_check | O(n²) `unique.contains()` на горячем пути упрощения |
| F6 | Low | step_export | Деление на ноль в `ccw_order_indices` без guard |
| F7 | Low | material_sort | `normalize_material_name` — частичная эмуляция Python `casefold()` |
| F8 | Low | certificate | AES-ключ из одного SHA-256 вместо HKDF/KDF |
| F9 | Low | renderer3d | Texture2D/DepthTexture2D создаются каждый кадр без кэширования |
| F10 | Low | air_cavities | `windows_1251_encode` возвращает `Err(())` без контекста |
| F11 | Info | UI | Хардкод русских строк (нет i18n-слоя) |
| F12 | Info | CI | Нет явного CI-стопа на admin/product token в клиентском бинарнике |
| F13 | Info | Cargo.lock | 6 версий `windows-sys`, 2 версии `glutin`/`memmap2` |
| F14 | Medium | UI pages | Доменная логика выполняется синхронно в UI-потоке каждый кадр без кэширования |

## Детали находок

### F1 — panic на NaN в model_check (Medium)

**Локация:** `rust/src/model_check.rs:602, 720`
```rust
sorted_coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
```
`parse::<f64>()` в парсере принимает `nan`, `inf`, `-inf`. Если во входном скрипте встретится координата `nan` (например, из повреждённого импорта), `partial_cmp` вернёт `None`, `.unwrap()` запаникует, унеся процесс. UI работает в том же потоке.

**Рекомендация:** использовать `total_cmp` (стабильно с Rust 1.62) или `partial_cmp(...).unwrap_or(Ordering::Equal)`. Дополнительно — валидировать координаты на конечность в `parse_line`.

### F2 — normalize() на вырожденных боксах (Medium)

**Локация:** `rust/src/ui/renderer3d.rs:213`
```rust
let normal = (face_center - center).normalize();
```
Если бокс вырожденный (x1==x2 ИЛИ y1==y2 ИЛИ z1==z2), `face_center == center` для двух граней, `(face_center - center)` = (0,0,0), `.normalize()` даёт NaN, каскад в `dot`/`max`. `set_segments` не фильтрует вырожденные боксы (в отличие от `step_export::write_step`, который фильтрует `> 1e-9`).

**Рекомендация:** либо фильтровать вырожденные сегменты в `set_segments`, либо guard: `let len = v.magnitude(); if len < 1e-9 { continue; } let normal = v / len;`.

### F3 — unsafe FFI без windows-sys (Medium)

**Локация:** `rust/src/ui/mod.rs:355-407`
Ручной `extern "system"` блок для `FindWindowW` и `DwmSetWindowAttribute`, тогда как `windows-sys 0.61.2` уже в зависимостях с нужными фичами. Ручное объявление сигнатур — риск расхождения с реальным ABI при обновлении Windows.

**Рекомендация:** использовать готовые биндинги из `windows-sys::Win32::Graphics::Dwm::DwmSetWindowAttribute` и `windows-sys::Win32::UI::WindowsAndMessaging::FindWindowW`. Снизит риск и упростит код.

### F4 — дублирующие преобразования (Low)

**Локация:** `rust/src/transforms.rs:36-39` vs `46-49`, и `120-125` vs `129-131`
`mirror_xy_y` и `mirror_xz_z` математически идентичны: обе `(-x2, y1, z1, -x1, y2, z2)`. Их enable-маппинги тоже идентичны: `[1, 0, 2, 3, 4, 5]`. Подтверждено тестами `test_transforms.rs:12-13, 33-34` — обе возвращают `102345` и `110111`.

Это **намеренное** повторение оригинального Povorotnik.py (комментарий в шапке файла подтверждает), но в коде нет пояснения, *почему* математически разные концепции (зеркало по Y в плоскости XY vs зеркало по Z в плоскости XZ) дают одну формулу.

**Рекомендация:** добавить комментарий у одной из пары: «математически эквивалентно `mirror_xy_y` для прямоугольных боксов; отдельная кнопка сохранена для UX-совместимости с v1». Или объединить в один указатель.

### F5 — O(n²) на горячем пути (Low)

**Локация:** `rust/src/model_check.rs:593-598, 711-716`
```rust
for line in lines.iter() {
    if let Some(seg) = &line.segment {
        let (v1, v2) = axis_vals(seg, axis);
        if !unique.contains(&v1) { unique.push(v1); }
        if !unique.contains(&v2) { unique.push(v2); }
    }
}
```
`Vec::contains` внутри цикла = O(n²). На 150 плоскостях терпимо, но `check_page` пересчитывает анализ при каждом изменении tolerance (с debounce 300 мс).

**Рекомендация:** заменить на `HashSet<f64>` или `BTreeSet` для O(n log n). Учитывая float-равенство, использовать `OrderedFloat` или нормализованный ключ.

### F6 — деление на ноль в step_export (Low)

**Локация:** `rust/src/step_export.rs:59-65`
```rust
let u_len = norm(u);
for value in &mut u { *value /= u_len; }
let v = cross(normal, u);
let v_len = norm(v);
let v = [v[0] / v_len, ...];
```
Если первая вершина грани совпадает с центроидом (дегенерированный бокс) или `normal` коллинеарна `u`, деление на ноль даёт NaN/inf. На практике боксы фильтруются `> 1e-9` перед вызовом `add_box` (строка 348-353), так что недостижимо, но guard всё равно уместен.

**Рекомендация:** `if u_len < 1e-9 { return indices.to_vec(); }` перед делением.

### F7 — частичная эмуляция casefold (Low)

**Локация:** `rust/src/material_sort.rs:43-52`
Комментарий честно признаёт: Python использует `str.casefold()` (агрессивное Unicode-folding), Rust использует `to_lowercase()` + точечные замены `ß→ss`, `ſ→s`, `ℌ→h`. Для HEAT3-имён (рус+лат) практически безопасно, но `casefold()` делает гораздо больше (fi/ﬂ-лигатуры, греческая final sigma σ/ς, армянские и т.д.).

**Рекомендация:** либо принять как осознанное ограничение и добавить тест с экзотическим именем, либо использовать `String::to_lowercase` + ICU-библиотеку. Первый вариант прагматичнее.

### F8 — AES-ключ из однократного SHA-256 (Low)

**Локация:** `rust/src/licensing/certificate.rs:154-161`
```rust
let mut secret = Sha256::digest(&secret_input);  // license_key + fingerprint
let cipher = Aes256Gcm::new_from_slice(&secret)...
```
Не PBKDF2/scrypt/argon2/HKDF. Однако: `license_key` — высокоэнтропийный (выдан сервером, типично 128+ бит), `fingerprint` — SHA-256 хэш. Вместе дают достаточно энтропии для AES-256, так что однократный SHA-256 приемлем. Спорное, но не уязвимое решение.

**Рекомендация:** для соответствия best practice заменить на HKDF-SHA256 с salt. Низкий приоритет.

### F9 — создание GL-текстур каждый кадр (Low)

**Локация:** `rust/src/ui/renderer3d.rs:100-114`
`Texture2D::new_empty` и `DepthTexture2D::new` вызываются в `render()`, который вызывается при изменении `render_key` (сцена/угол/размер). При drag-to-rotate это каждый кадр. three-d освобождает через Drop, но churn может деградировать GPU-память на слабых картах.

**Рекомендация:** кэшировать render-target по `(w, h)` в поле `Renderer3D`, перераспределять только при ресайзе.

### F10 — потеря контекста ошибки (Low)

**Локация:** `rust/src/air_cavities.rs:507-515`
`windows_1251_encode` возвращает `Result<Vec<u8>, ()>`. Вызывающий код (`format_air_cavity_name`) теряет информацию о том, *какой именно* символ не удалось закодировать.

**Рекомендация:** `Result<Vec<u8>, String>` с указанием проблемного символа/позиции.

### F11 — хардкод русских строк (Info)

**Локация:** весь `rust/src/ui/`
Все UI-строки на русском, разбросаны по страницам. Нет слоя локализации. Для внутреннего инструмента (русские инженеры) приемлемо, но блокирует любую будущую локализацию.

**Рекомендация:** вынести в `const`-таблицу или `fluent`-bundle, если планируется i18n. Низкий приоритет.

### F12 — нет CI-проверки на отсутствие admin token (Info)

**Локация:** `.github/workflows/rust.yml`
README требует: «admin/product token в клиентскую сборку добавлять запрещено». Контролируется через `option_env!` во время компиляции, но в CI нет явного шага, проверяющего, что клиентский бинарник не содержит строку `HEAT3_KEYGEN_ADMIN_TOKEN` или похожих секретов.

**Рекомендация:** добавить post-build шаг `strings target/release/heat3_povorotnik.exe | findstr /i "ADMIN_TOKEN PRODUCT_TOKEN"` или использовать `cargo cargo-bloat`/символьный анализ. Низкий приоритет, так как `option_env!` compile-time + hygiene-скрипт частично покрывают.

### F13 — дублирование версий крейтов (Info)

**Локация:** `rust/Cargo.lock`
- `windows-sys`: 6 версий (0.36.1, 0.48.0, 0.52.0, 0.59.0, 0.60.2, 0.61.2) — типично для Windows-экосистемы
- `glutin`: 0.29.1 (dev-dep для real-GL тестов) + 0.32.3 (основной)
- `memmap2`: 0.5.10 + 0.9.11 — 0.5.10 проверяется на исключение из Windows graph
- `cgmath 0.18.0` — открытый R6 из предыдущего ревью

**Рекомендация:** `cargo tree -d` для аудита дубликатов. Обновление прямых зависимостей может консолидировать версии. Низкий приоритет.

### F14 — синхронная доменная логика в UI-потоке (Medium)

**Локация:** `rust/src/ui/air_cavities_page.rs:125-164`, `rust/src/ui/material_sort_page.rs:85-86`, `rust/src/ui/report_page.rs` (перестроение таблицы), `rust/src/ui/corner_page.rs:62` (`detect_constant_pair` каждый кадр)

Большинство страниц заново выполняют нетривиальную доменную работу внутри `show()` каждый кадр egui: `parse_air_cavities_info_log` + `build_air_cavity_materials`, `extract_material_entries`, `detect_constant_pair`, парсинг всего скрипта. Только `check_page.rs` реализовал правильный паттерн — кэш (`cached_text`/`cached_tol`/`cached_pct`) + debounce 300 мс + `analysis_pending_since`.

egui требует ответа за миллисекунды; на большом логе (десятки прослоек) или скрипте с сотнями боксов это вызовет видимые подёргивания интерфейса. Дополнительно — если доменная функция panic-ёт (см. F1), это уносит весь UI-процесс, а не отдельную операцию.

**Рекомендация:** вынести доменную логику в фоновый `thread::spawn` + `mpsc::channel` (как уже сделано в `LicenseManager`), либо добавить кэш «грязный/чистый» по образцу `check_page.rs` во все страницы. Первое надёжнее для тяжёлых операций (cavity detection, material sort), второе достаточно для лёгких (`detect_constant_pair`, `extract_material_entries`).

## Подтверждённые сильные стороны

### Лицензионный стек — эталонный
- **`signature.rs`**: последовательная fail-closed верификация Ed25519 (method/path/host/date/digest sanitize → date parse → staleness 5 мин → digest → signature). `validate_component` отвергает CRLF → защита от header injection.
- **`certificate.rs`**: signature **до** decryption; exhaustive assertion всех JSON-API полей (type/id/fingerprint/account/product/license/policy); проверка TTL ∈ [MIN, maximum], expiry > issued, |expiry−issued−ttl| ≤ 2. Все секреты zeroized.
- **`keygen_client.rs`**: HTTPS-only, no redirects, bounded timeouts (5/15 с), streaming size limit с `MAX+1` для overflow detection. Верификация подписи **до** проверки status — защита от поддельных ошибок MITM. Path traversal guard в `endpoint`.
- **`gate.rs`**: seqlock-подобный snapshot (revision + RwLock + retry-loop); fail-closed при отравлении lock. `Arc<GateInner>` для разделяемого состояния UI/worker.
- **`storage.rs`**: DPAPI CurrentUser, envelope v2 с SHA-256 digest, атомарная замена через `MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH)`, exclusive lock через fs2, миграция schema v1→v2 in-place. Проверка размера `MAX_ENVELOPE_SIZE = 16 MiB`.
- **`hardware.rs`**: хэширование через SHA-256 с доменным разделителем (нет утечки персональных данных); консервативное majority-recovery в `matches_stored_hardware`; санитизация OEM-заполнителей (`TOBEFILLEDBYOEM`, all-0, all-F).
- **`manager.rs`**: жизненный цикл activate/refresh/suspend/reinstate/deactivate через mpsc-канал; обработка `TryRecvError::Disconnected` (R2 закрыт); `Drop` persist trusted time; clock rollback detection с tolerance 5 мин.

### Лицензионный CLI — зрелый аудит
- **`tools/license-admin/src/main.rs`**: HMAC-SHA256 цепочка от GENESIS; exclusive OS lock; полная перепроверка цепочки при каждом открытии (`verified_audit_state`); подписанный sidecar `.state` с entry_count/byte_len/previous_mac; обнаружение усечения/замены/отсутствия state. `redact_secrets` рекурсивно маскирует `key`/`token`/`secret` в stdout. Sensitive auth header.

### CI и инфраструктура
- **Pinned SHAs** (R11 закрыт и подтверждён): `actions/checkout@11d5960`, `dtolnay/rust-toolchain@2c7215`, `Swatinem/rust-cache@c193711`.
- **Три job'а**: `rust-msrv` (cargo check на 1.88.0), `rust-client` (hygiene + no-exe + fmt + clippy `-D warnings` + test + audit-windows), `license-admin` (fmt + clippy + test + audit).
- **`audit-windows.ps1`**: проверяет, что `quick-xml` и `memmap2@0.5.10` не вошли в Windows target graph (`cargo tree -i`), игнорирует 3 конкретных RustSec advisory.
- **`check-source-hygiene.ps1`**: детектор mojibake (UTF-8↔Windows-1251) и sync-conflict файлов.

### Доменная логика
- **`parser.rs`**: сохранение оригинальных разделителей при сериализации; round-trip stable; fail-safe на мусорном входе.
- **`model_check.rs`**: корректный 3D BFS flood-fill для cavity detection с проверкой границ; cell_limit защита от memory exhaustion.
- **`step_export.rs`**: соответствие ISO 10303-21 (HEADER/DATA/ENDSEC, FILE_SCHEMA AUTOMOTIVE_DESIGN, единицы, UNCERTAINTY); правильная топология EDGE_CURVE/ORIENTED_EDGE/ADVANCED_FACE/CLOSED_SHELL.

## Неопределённости и вопросы

1. **F4 (дубликат transforms):** подтвердить у автора оригинального Povorotnik.py, что идентичность `mirror_xy_y` и `mirror_xz_z` — осознанная математическая эквивалентность для axis-aligned боксов, а не copy-paste баг.
2. **F2 (renderer3d вырожденные боксы):** каков реальный риск встречи вырожденного сегмента во входных данных? `samples/model.step` содержит только ненулевые толщины, но HEAT3-скрипты от пользователей могут содержать zero-thickness плоскости.
3. **R6 (cgmath):** остаётся открытым для production — нужно решение: заменить three-d на поддерживаемый крейт, или форкнуть/запатчить three-d, или принять с компенсирующим контролем.
4. **R7 (staging):** остаётся открытым — требуется живой Keygen CE для E2E сценариев 1–20.

## Сводка по тестам

| Набор | Файлов | Покрытие |
|---|---|---|
| `rust/tests/` | 5 файлов | transforms (enable flags, pipeline), turner2d, material_sort, air_cavities, model_check (cavity detection edge cases) |
| `rust/src/**/*.rs` `#[cfg(test)]` | встроенные | parser, text, licensing (manager 500+ строк тестов, storage, signature, certificate, gate, hardware) |
| `tools/license-admin/src/main.rs` `#[cfg(test)]` | встроенные | audit chain tamper/truncation, миграция, redaction |

Предыдущее ревью зафиксировало: клиент 62 unit-теста + 31 интеграционный; CLI 9/9. С тех пор тесты расширены (видно по объёму `#[cfg(test)]` блоков).

**Пробел в покрытии:** нет тестов на F1 (NaN-координаты в model_check), F2 (вырожденные боксы в renderer3d). Оба стоит добавить при устранении находок.

## Рекомендуемый порядок устранения

1. **F1, F2, F14** (medium, надёжность/производительность) — guards от panic, кэш доменной логики в UI + тесты. ~3 часа.
2. **F3** (medium, поддерживаемость) — миграция на windows-sys. ~2 часа.
3. **F5, F9** (low, производительность) — HashSet + кэш текстур. ~2 часа.
4. **F6, F10** (low, robustness) — guards + контекст ошибок. ~30 мин.
5. **F4, F7, F8, F11, F12, F13** (info) — документация/рефакторинг по возможности.

## Ограничения ревью

Это статическое ревью исходного кода. Не выполнено: динамический fuzzing парсера/step_export, нагрузочное тестирование model_check на больших моделях, ручное UI-тестирование всех страниц, независимый penetration test криптографии. Рекомендуется для production-решения.
