# HEAT3 «Поворотник»

Десктопная утилита для преобразования, проверки и подготовки моделей HEAT3
(типы линий `p`, `b`, `e`). Работает через буфер обмена: вставил скрипт →
применил преобразования → скопировал обратно.

Приложение реализовано на Rust с интерфейсом на iced 0.14 (backend: wgpu).

## Быстрый старт

Требуется установленный Rust toolchain (MSRV 1.90, см. `package.rust-version`).

```powershell
cargo run --release
```

На системах, где с сетевого диска запрещен запуск build scripts, задайте
локальный каталог сборки:

```powershell
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\CodexBuild\heat3_povorotnik"
cargo test --all-targets
cargo build --release
```

## Структура

```
├── Cargo.toml
├── src/
│   ├── main.rs              # Точка входа (iced)
│   ├── config.rs            # Константы, цвета, размеры
│   ├── models.rs            # Модели данных (Segment, ScriptLine)
│   ├── parser.rs            # Парсер/сериализатор скрипта
│   ├── transforms.rs        # 7 геометрических преобразований
│   ├── turner2d.rs          # 2D-преобразования прямоугольников
│   ├── text.rs              # Разбиение текста на строки
│   ├── clipboard.rs         # Буфер обмена (arboard)
│   ├── material_sort.rs     # MTL-файлы, сортировка материалов
│   ├── air_cavities.rs      # Воздушные прослойки из HEAT2 лога
│   ├── model_check.rs       # Подсчёт плоскостей, пустоты, упрощение
│   ├── step_export.rs       # Экспорт в STEP (ISO 10303-21)
│   ├── corner.rs            # Создание угла окна
│   └── ui2/                 # Интерфейс на iced: app, страницы, widgets, theme
├── tests/                   # Интеграционные тесты
├── assets/fonts/            # Шрифты (Inter, JetBrains Mono)
├── resources/logocube.ico   # Иконка приложения
├── scripts/                 # Windows-aware dependency audit
├── dist/                    # Release-артефакты (exe в репозиторий не коммитить)
├── docs/                    # Спецификации, планы, решения, обзоры
│   └── superpowers/specs/   # Дизайн-спецификации страниц
├── tools/
│   ├── check-source-hygiene.ps1
│   ├── license-admin/       # Операторский CLI Keygen CE (в клиент не входит)
│   ├── e2e/                 # End-to-end проверки
│   └── release/             # Подпись и упаковка релиза
└── samples/                 # Справочные входные данные и примеры
```

## Сборка и запуск

Лицензионные параметры читаются через `option_env!` и поэтому должны быть
заданы именно во время компиляции. В бинарник допускаются только публичный
ключ и публичные идентификаторы; admin/product token в клиентскую сборку
добавлять запрещено.

```powershell
$env:HEAT3_KEYGEN_API_URL = "https://license.example.com"
$env:HEAT3_KEYGEN_ACCOUNT_ID = "<account-id-or-slug>"
$env:HEAT3_KEYGEN_PRODUCT_ID = "<product-id>"
$env:HEAT3_KEYGEN_POLICY_ID = "<policy-id>"
$env:HEAT3_KEYGEN_PUBLIC_KEY = "<ed25519-public-key-base64-or-hex>"

cargo build --release
```

Без полного набора параметров приложение намеренно запускается в закрытом
состоянии и показывает ошибку конфигурации. API URL обязан быть HTTPS origin
без path, query, credentials или fragment. Подписанный офлайн-период по
умолчанию — 7 суток; работающий клиент пытается обновить его каждые 15 минут.

DPAPI-запись и локальный trusted-time checkpoint обнаруживают повреждение
состояния и обычный откат часов, но не гарантируют свежесть при согласованном
восстановлении более старого корректного состояния пользователя или всей VM.
Это фундаментальное ограничение семисуточного офлайн-запуска: строгая защита
от такого replay требует онлайн-проверки перед каждым запуском либо внешнего
монотонного якоря (например, broker/TPM-протокола), который нельзя откатить
вместе с локальным диском.

## Проверка

```powershell
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Windows-aware dependency audit (requires `cargo-audit`):

```powershell
.\scripts\audit-windows.ps1
```

## Сборка .exe

```powershell
cargo build --release
```

Результат: `target/release/heat3_povorotnik.exe`. Подписанный релизный пакет
собирается через `tools/release/sign-and-package.ps1` (CI job `release-sign`).

## Лицензирование

Архитектура Keygen CE, офлайн-проверки, аппаратной привязки и защищенного
хранилища зафиксирована в
[`docs/plans/2026-07-17-keygen-licensing-architecture-plan.md`](docs/plans/2026-07-17-keygen-licensing-architecture-plan.md).
