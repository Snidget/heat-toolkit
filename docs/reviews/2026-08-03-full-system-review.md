# Полное ревью HEAT3 «Поворотник» v2.0 — готовность к проде

**Снимок:** рабочее пространство по состоянию на 2026-08-03
**Объём:** весь репозиторий (`rust/`, `tools/license-admin/`, CI, документация)
**Основание:** предыдущие ревью `2026-07-28-full-system-review.md` (F1–F14) и `2026-07-17-keygen-licensing-security-review.md` (R1–R11), архитектурный план, Rust/Windows best practices
**Метод:** Gate mode, Deep depth — полный последовательный анализ исходников

## Вердикт

**NOT READY FOR PRODUCTION — готов с оговорками к staging.**

11 из 14 находок полного ревью устранены. 9 из 11 находок security-ревью закрыты. Доменная логика и лицензионный стек — эталонного качества. Однако остаются **3 блокирующих фактора** — все внешние по отношению к коду, но необходимые для production.

## Статус устранения (2026-08-03, после сессии правок)

| Находка | Статус | Что сделано |
|---|---|---|
| N1 (Low) | ✅ Закрыт | doc-комментарии в `rust/src/transforms.rs` у `mirror_xy_y` и `mirror_xz_z` |
| N2 (Low) | ✅ Закрыт | job `release-secret-scan` в `.github/workflows/rust.yml`: release-сборка с env-секретами = sentinel-строке, ASCII-скан бинарника, throw при совпадении |
| N4 (Medium) | ✅ Закрыт | кэш `cached_script`/`cached_entries` в `report_page.rs`; кэш `detect_constant_pair` и preview `parse_script` в `corner_page.rs` |
| F8 (Low) | ✅ Закрыт (документация) | SHA-256(license_key + fingerprint) — **требование протокола Keygen** («Hashing these values with SHA256 is required», keygen.sh/docs/api/cryptography/), не упрощение. Изменение KDF сломает расшифровку файлов сервера. Добавлен поясняющий комментарий в `certificate.rs` |
| B2/R6 | ✅ Принят как documented risk (решение владельца) | 2026-08-03: cgmath заморожен на 0.18.0 (последний релиз 2021-01-03); three-d 0.19.0 (актуальный) тоже зависит от `cgmath ^0.18` — апгрейд не помогает. `swap_columns` в коде не вызывается. Компенсирующий контроль: ежегодный `cargo audit` в CI (job `rust-client`). Замена рендерера — отдельная задача |
| B1/R7 | ✅ Закрыт со стороны кода; запуск — за владельцем стенда | `rust/src/bin/e2e_staging.rs` — headless-зонд (validate/activate/checkout/checkin/deactivate/verify-offline/expect-network-failure, выходы 0/2/1); `tools/e2e/staging-e2e.ps1` — харнесс сценариев 1–20 (автоматизирует 1, 2, 3-сеть, 5, 6-сеть, 7, 8-сеть, 11, 12-сеть, 13, 14, 15, 16, 17, 18; GUI/DPAPI/backup — MANUAL-шаги в отчёте). Компиляция зонда проверяется CI (`rust-client`: clippy --all-targets, test) |
| B3 | 🔶 Готов, нужен сертификат | `tools/release/sign-and-package.ps1` (Authenticode via Set-AuthenticodeSignature + timestamp + ZIP + MSI через WiX candle/light + двойная подпись exe/MSI) + `tools/release/heat3_povorotnik.wxs` + CI-job `release-sign` (при заданном секрете `HEAT3_CODESIGN_CERT_BASE64` подписывает и публикует артефакты, иначе честно сообщает «UNSIGNED»). Осталось: сертификат + секреты + WiX на release-машине |
| B4 | 🔶 Лист решений готов, нужно подтверждение | `docs/decisions/2026-08-03-dg-decision-sheet.md` — DG-01…DG-05 с рекомендуемыми дефолтами и полями `[x]`/дата/кто; подтверждение снимает блокер B4 |
| N3, F13 (Info) | ⏳ При рефакторинге | Консолидация версий требует cargo (windows-sys ×6, glutin ×2, memmap2 ×2) — локально недоступен |

**Не проверено локально:** cargo/rustc отсутствуют на рабочей машине (запущено не было) — сборку, fmt, clippy -D warnings и тесты подтвердит CI (`rust-client`). Новый зонд `e2e_staging` проходит те же проверки CI (включён в `--all-targets`).

## Карта находок: повторное ревью предыдущих finding'ов

### Полное системное ревью (F1–F14)

| ID | Sev | Статус | Сводка |
|---|---|---|---|
| F1 | Medium | ✅ **Закрыт** | `total_cmp()` в model_check.rs:603,721. Парсер отклоняет NaN/inf в parser.rs:63 (`v.is_finite()`). Двойная защита. |
| F2 | Medium | ✅ **Закрыт** | renderer3d.rs:239 — guard `magnitude() < 1e-9` перед `normalize()`, fallback на `vec3(0,0,0)`. |
| F3 | Medium | ✅ **Закрыт** | ui/mod.rs:358-361 — используются `windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute` и `FindWindowW` из крейта. Ручной `extern "system"` удалён. |
| F4 | Low | ⚠️ **Частично** | Заголовок transforms.rs объясняет повтор оригинала, но конкретная эквивалентность `mirror_xy_y` и `mirror_xz_z` не поясняется. См. N1. |
| F5 | Low | ✅ **Закрыт** | HashSet с `v.to_bits()` в model_check.rs:590,595,709,714 вместо `Vec::contains()`. |
| F6 | Low | ✅ **Закрыт** | step_export.rs:60,68 — guards `if u_len < 1e-9` и `if v_len < 1e-9` перед делением. |
| F7 | Low | ✅ **Принят** | material_sort.rs:46-52 — комментарий документирует ограничение `to_lowercase()` vs `casefold()`. Осознанное решение. |
| F8 | Low | ⚠️ **Принят** | certificate.rs:158 — всё ещё однократный SHA-256 для AES-ключа. Энтропии достаточно, но не HKDF. Низкий приоритет. |
| F9 | Low | ✅ **Закрыт** | renderer3d.rs:19,21-22,94-115 — текстуры кэшируются, пересоздаются только при ресайзе. |
| F10 | Low | ✅ **Закрыт** | air_cavities.rs:504-512 — возвращает `Result<Vec<u8>, String>` с описанием ошибки. |
| F11 | Info | ⚠️ **Принят** | i18n отсутствует. Приемлемо для внутреннего инструмента. |
| F12 | Info | ✅ **Закрыт (см. N2)** | CI получил post-build проверку на admin/product token: job `release-secret-scan` (sentinel-сборка + ASCII-скан бинарника + throw). |
| F13 | Info | ⚠️ **Не изменился** | 6 версий windows-sys, 2 glutin, 2 memmap2 в Cargo.lock. См. N3. |
| F14 | Medium | ✅ **Закрыт (см. N4)** | Кэши добавлены во все 4 страницы: air_cavities_page, material_sort_page, report_page.rs, corner_page.rs. |

### Security-ревью (R1–R11)

| ID | Sev | Статус | Сводка |
|---|---|---|---|
| R1–R5 | Major | ✅ **Закрыты** | Подтверждено в security review. transient 5xx/429, disconnected worker, exclusive lock, audit-before-API, sensitive headers. |
| **R6** | Moderate/high | ✅ **Принят как documented risk** | `cgmath 0.18.0` (заморожен, 2021) через `three-d`; актуальный three-d 0.19.0 тоже требует `cgmath ^0.18`. `swap_columns` не вызывается. Компенсация: `cargo audit` в CI. Решение владельца от 2026-08-03. |
| **R7** | Blocker | ✅ **Закрыт со стороны кода; запуск за владельцем стенда** | E2E-зонд `rust/src/bin/e2e_staging.rs` (реальные подписи/машины/файлы Keygen, headless) + харнесс `tools/e2e/staging-e2e.ps1` (сценарии 1–20, отчёт в markdown). Осталось: поднять staging Keygen CE и прогнать. |
| R8–R11 | Major | ✅ **Закрыты** | Recovery path, stale-state overwrite, schema-v2 migration, CI pinning. |

## Новые находки (N)

### N1 — Дубликат mirror_xy_y / mirror_xz_z без пояснения (Low)

**Локация:** `rust/src/transforms.rs:36-39` vs `46-49`

**Наблюдение:** `mirror_xy_y` и `mirror_xz_z` математически идентичны: обе `(-x2, y1, z1, -x1, y2, z2)`. Enable-маппинги тоже идентичны: `[1,0,2,3,4,5]`. Заголовок файла объясняет, что логика повторяет оригинал, но не поясняет *почему* разные концепции дают одну формулу.

**Рекомендация:** Однострочный комментарий у `mirror_xz_z`: «Математически эквивалентно `mirror_xy_y` для axis-aligned боксов; отдельная кнопка сохранена для UX-совместимости с v1».

### N2 — Нет CI-шага проверки бинарника на секреты (Info → Low для production)

**Локация:** `.github/workflows/rust.yml`

**Наблюдение:** CI проверяет hygiene, fmt, clippy, test, audit — но не имеет post-build шага, проверяющего, что клиентский бинарник не содержит строк `ADMIN_TOKEN`, `PRODUCT_TOKEN` или подобных секретов. `option_env!` контролирует это compile-time, но дополнительная runtime-проверка усилила бы гарантию.

**Рекомендация:** Добавить шаг после `cargo build --release`:

```powershell
$matches = strings target/release/heat3_povorotnik.exe | Select-String -Pattern "ADMIN_TOKEN|PRODUCT_TOKEN"
if ($matches) { throw "Secret detected in binary" }
```

### N3 — Дублирование версий крейтов (Info)

**Локация:** `rust/Cargo.lock`

**Подтверждено:** windows-sys ×6 (0.36.1, 0.48.0, 0.52.0, 0.59.0, 0.60.2, 0.61.2), glutin ×2 (0.29.1 dev-dep + 0.32.3), memmap2 ×2 (0.5.10 + 0.9.11). cgmath 0.18.0 — единственная версия, но через three-d.

**Рекомендация:** `cargo tree -d` для аудита. Обновление прямых зависимостей может консолидировать. Низкий приоритет.

### N4 — Остаточная синхронная доменная логика в UI (Medium)

**Локация:**
- `rust/src/ui/report_page.rs:223` — `extract_material_entries(script_text)` каждый кадр без кэша (сравните с material_sort_page.rs:87-92, где кэш добавлен)
- `rust/src/ui/corner_page.rs:62` — `detect_constant_pair(script_text)` каждый кадр
- `rust/src/ui/corner_page.rs:116-119` — `parse_script(text)` каждый кадр для preview

**Последствие:** На скрипте с сотнями боксов — видимые подёргивания UI. Не блокирует функциональность, но деградирует UX на больших моделях.

**Рекомендация:** Добавить кэш «грязный/чистый» по образцу `material_sort_page.rs` (cached_script + сравнение). Для `detect_constant_pair` достаточно лёгкого кэша; для `parse_script` в preview — аналогично.

## Блокирующие факторы для production

### B1 — R7: Нет staging E2E (Blocker)

**Состояние (обновлено 2026-08-03):** Инструмент E2E реализован: `rust/src/bin/e2e_staging.rs` (headless-зонд: validate/activate/checkout/checkin/deactivate/verify-offline/expect-network-failure с выходными кодами 0/2/1; проверяет реальные подписи Ed25519, AES-GCM machine files, dedup машин, suspend/reinstate/revoke) и `tools/e2e/staging-e2e.ps1` (харнесс: issue → activate → checkin → verify-offline → tamper → suspend → reinstate → network-failure → deactivate → revoke → secret-scan; GUI/DPAPI/backup сценарии — MANUAL-шаги; итог — markdown-отчёт в `docs/reviews/staging-e2e-report-*.md`). Не выполнено против live Keygen CE (стенда нет). Production account/product/policy IDs и public key не предоставлены.

**Действие:** Поднять staging Keygen CE (policy: LICENSE, maxMachines=1, fingerprint/components scope, check-in requirements, machine-file TTL 7 суток), собрать `e2e_staging` и `heat3-license-admin`, запустить харнесс, выполнить MANUAL-шаги на чистой VM, затем поднять production Keygen CE с отдельными секретами.

### B2 — R6: Unmaintained cgmath через three-d (Moderate/High)

**Состояние:** `cgmath 0.18.0` в Cargo.lock через `three-d 0.18`. RustSec сообщает unsound `swap_columns`. Поиск в коде показал, что `swap_columns` нигде не вызывается — недостижимая failure. Однако unmaintained dependency в production-стеке 3D-рендеринга — технический долг, требующий решения.

**Действие:** (решение владельца от 2026-08-03) — **принято как documented risk**: `swap_columns` недостижим, cgmath используется только внутри three-d; компенсирующий контроль — `cargo audit` в CI. Замена рендерера вынесена в отдельную задачу.

### B3 — Отсутствие подписи бинарника и installer (Blocker для production)

**Состояние:** Архитектурный план (раздел 8.1, W8) требует Authenticode-подпись и подписанный канал обновления. Текущая сборка — просто `cargo build --release`. Нет installer, нет кодо-подписи.

**Состояние (обновлено 2026-08-03):** Пайплайн реализован — `tools/release/sign-and-package.ps1` (Set-AuthenticodeSignature + RFC-3161 timestamp + ZIP-пакет + MSI через WiX candle/light + двойная подпись), `tools/release/heat3_povorotnik.wxs` (MSI v3.14, per-machine, major upgrade) и CI-job `release-sign`. Без секрета `HEAT3_CODESIGN_CERT_BASE64` job собирает release и честно помечает его UNSIGNED; без WiX на машине MSI пропускается с предупреждением. Осталось: приобрести сертификат, добавить секреты в репозиторий, установить WiX на release-машину.

**Действие:** Внедрить Authenticode-подпись release-бинарника и installer (MSI/NSIS).

### B4 — Незакрытые бизнес decision gates (DG-01 … DG-05)

**Состояние:** Из архитектурного плана — DG-01 (офлайн TTL), DG-02 (тарифы/expiry), DG-03 (rehost policy), DG-04 (production domain), DG-05 (legal/privacy) — не подтверждены. Без них нельзя строить production-конфигурацию.

**Действие:** Владелец продукта подтверждает каждый DG с дедлайнами.

## Подтверждённые сильные стороны

### Лицензионный стек — эталонный

- **signature.rs**: fail-closed Ed25519 верификация; CRLF-санитизация против header injection
- **certificate.rs**: signature **до** decryption; exhaustive assertion JSON-API полей; TTL ∈ [MIN, maximum]; zeroization секретов
- **keygen_client.rs**: HTTPS-only, no redirects, bounded timeouts, streaming size limit с overflow detection; path traversal guard
- **gate.rs**: seqlock-snapshot; fail-closed при отравлении lock; clock rollback detection с tolerance 5 мин
- **storage.rs**: DPAPI CurrentUser, envelope v2 с SHA-256 digest, атомарная замена, exclusive lock, migration v1→v2
- **hardware.rs**: domain-separated SHA-256, majority recovery, санитизация placeholder-заполнителей
- **manager.rs**: mpsc lifecycle, `TryRecvError::Disconnected` обработан, Drop persist trusted time

### Доменная логика

- **parser.rs**: round-trip stable, сохранение оригинальных разделителей, **теперь** отклоняет NaN/inf (parser.rs:63)
- **model_check.rs**: 3D BFS flood-fill для cavity detection, cell_limit, **теперь** `total_cmp` и HashSet
- **step_export.rs**: ISO 10303-21 соответствие, **теперь** guards от деления на ноль
- **transforms.rs**: полные тесты enable-маппингов и pipeline

### CI и инфраструктура

- Pinned SHAs для actions (R11 закрыт)
- 3 job'а: rust-msrv (1.88.0 check), rust-client (hygiene + no-exe + fmt + clippy -D warnings + test + audit-windows), license-admin (fmt + clippy + test + audit)
- `check-source-hygiene.ps1` — mojibake-детектор (UTF-8↔Windows-1251) и sync-conflict сканер
- `audit-windows.ps1` — проверка отсутствия quick-xml/memmap2@0.5.10 в Windows graph

### Тестовое покрытие

- Интеграционные: test_transforms (enable flags, pipeline), test_turner2d, test_material_sort, test_air_cavities, test_model_check (cavity detection edge cases)
- Встроенные unit в licensing (manager, storage, signature, certificate, gate, hardware), parser, text, step_export, renderer3d (geometry tests + ignored GL tests)
- renderer3d.rs: тесты на вырожденные сегменты (sample_segments содержит zero-thickness)

## Сводка: что сделано с предыдущего ревью

| Категория | Было | Стало |
|---|---|---|
| medium findings (код): F1, F2, F3, F14 | 4 открыты | все закрыты (F14 — см. N4, кэши во всех 4 страницах) |
| low findings (код): F4-F10 | 7 открыты | 5 закрыты, 2 приняты как осознанные ограничения |
| info findings: F11-F13 | 3 открыты | F11 принят, F12 закрыт (см. N2), F13 ждёт cargo (см. N3) |
| security (R1-R11) | 2 открыты (R6, R7) | R6 принят как documented risk, R7 закрыт со стороны кода (E2E-зонд + харнесс готовы) |
| блокеры (B1-B4) | 4 открыты | B1-код, B3-код, B4-лист готовы; N3 не закрыт (нет cargo) |

## Рекомендуемый порядок действий (обновлён 2026-08-03, финальный)

Все задачи, закрываемые кодом/документацией, закрыты. Остаток — внешние шаги владельца:

1. **Проверка CI** — проект не в git-репозитории (сетевой диск); инициализировать repo и push в GitHub, чтобы CI (`rust-client`: fmt/clippy/test/audit, `release-secret-scan`) подтвердил компиляцию всех изменений, включая зонд `e2e_staging`
2. **B4** — владелец продукта проставляет `[x]` в `docs/decisions/2026-08-03-dg-decision-sheet.md` (DG-01…DG-05)
3. **B1** — поднять staging Keygen CE (policy: LICENSE, maxMachines=1, check-in requirements, machine-file TTL 7 суток), собрать `e2e_staging` + `heat3-license-admin` (cargo), запустить `tools/e2e/staging-e2e.ps1` с env `HEAT3_ADMIN_*` + `HEAT3_E2E_*`, пройти MANUAL-шаги (DPAPI cross-user, смена железа, откат часов, backup, GUI) на чистой VM
4. **B3** — приобрести кодо-подписывающий сертификат, добавить секреты `HEAT3_CODESIGN_CERT_BASE64`/`HEAT3_CODESIGN_CERT_PASSWORD` (и локально на release-машине), установить WiX Toolset v3.14 — подпись exe+MSI и ZIP-пакет собираются автоматически
5. **N3, F13** — `cargo update`/консолидация версий (windows-sys ×6, glutin ×2, memmap2 ×2) на машине с cargo, затем push

## Ограничения ревью

Статический анализ исходного кода. Не выполнено: динамический fuzzing парсера/step_export, нагрузочное тестирование model_check на моделях >150 плоскостей, ручное UI-тестирование всех 9 страниц под нагрузкой, независимый penetration test криптографии, live staging E2E. Рекомендуется для production-решения.