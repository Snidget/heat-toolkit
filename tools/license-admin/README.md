# HEAT3 License Admin

Отдельный операторский CLI для Keygen CE. Он не является частью клиентского Cargo-пакета и не должен попадать в пользовательский installer.

## Секреты и конфигурация

Все параметры читаются только во время запуска:

```powershell
$env:HEAT3_ADMIN_API_URL = "https://license.example.com"
$env:HEAT3_ADMIN_ACCOUNT_ID = "<account-id-or-slug>"
$env:HEAT3_ADMIN_POLICY_ID = "<policy-id>"
$env:HEAT3_KEYGEN_ADMIN_TOKEN = "<admin-or-product-bearer-token>"
$env:HEAT3_ADMIN_AUDIT_KEY = "<random-secret-at-least-32-characters>"
$env:HEAT3_ADMIN_AUDIT_PATH = "D:\secure-audit\license-admin.jsonl"
```

`HEAT3_KEYGEN_ADMIN_TOKEN` и `HEAT3_ADMIN_AUDIT_KEY` нельзя сохранять в репозитории, командных файлах, CI logs или клиентском бинарнике. Для production предпочтителен product token с минимально необходимыми правами; admin token допустим только в изолированной операторской среде.

## Сборка и команды

```powershell
cd tools/license-admin
cargo build --release

cargo run -- issue --name "Покупатель / заказ 123"
cargo run -- list --limit 25
cargo run -- show <license-id>
cargo run -- suspend <license-id>
cargo run -- reinstate <license-id>
cargo run -- renew <license-id>
cargo run -- reset-usage <license-id> --confirm <license-id>
cargo run -- reset-machines <license-id> --confirm <license-id>
cargo run -- revoke <license-id> --confirm <license-id>
cargo run -- audit-migrate-state --confirm MIGRATE-AUDIT-STATE
```

Ключ новой лицензии выводится только командой `issue` для непосредственной безопасной передачи покупателю. В `list` и `show` поля `key`, `token`, `secret` рекурсивно маскируются. Необратимые операции требуют повторить ID через `--confirm`.

Каждая попытка операции записывается в HMAC-SHA256 цепочку JSONL. Подписанный sidecar `<audit-path>.state` закрепляет длину и последний MAC, поэтому несогласованное удаление хвоста журнала или самого sidecar блокирует дальнейшие операции.

Локальные JSONL и sidecar не являются независимым WORM-аудитом: согласованное восстановление их обеих из одной старой корректной копии локально не обнаруживается. Если production-требования включают защиту от такого replay, каждое событие или checkpoint необходимо дополнительно отправлять во внешний append-only/WORM sink (SIEM, удалённый broker или object storage с retention lock), который нельзя откатить вместе с операторской машиной.

`audit-migrate-state` предназначена только для однократного перехода существующего непустого журнала, созданного до появления подписанного sidecar. Перед миграцией сохраните резервную копию и расследуйте причину отсутствия state. Команда проверяет всю HMAC-цепочку, создаёт подписанный state и записывает событие миграции в журнал.
