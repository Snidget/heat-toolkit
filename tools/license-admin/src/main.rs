use fs2::FileExt;
use hmac::{Hmac, Mac};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

const JSON_API: &str = "application/vnd.api+json";
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

fn main() {
    if let Err(error) = run() {
        eprintln!("Ошибка: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let command = Command::parse(env::args().skip(1).collect())?;
    if matches!(command, Command::MigrateAuditState) {
        let mut config = AuditConfig::from_environment()?;
        AuditLog::migrate_state(config.audit_path.clone(), config.audit_key.as_bytes())?;
        config.zeroize_secrets();
        println!("Audit state migrated, signed, and saved.");
        return Ok(());
    }
    let mut config = Config::from_environment()?;
    let mut audit = AuditLog::open(config.audit_path.clone(), config.audit_key.as_bytes())?;
    let client = AdminClient::new(&config)?;
    let operation = command.operation_name();
    let target = command.audit_target();
    let correlation_id = generate_correlation_id();
    audit.append(
        operation,
        &target,
        "attempt",
        None,
        &correlation_id,
        &AuditOutcome::default(),
    )?;
    let (audit_outcome, outcome) = execute(&client, command);
    let audit_result = audit.append(
        operation,
        &target,
        "result",
        Some(outcome.is_ok()),
        &correlation_id,
        &audit_outcome,
    );
    config.zeroize_secrets();
    audit_result?;
    outcome
}

/// Redacted, HMAC-covered audit metadata produced by a command. Never contains
/// license keys or tokens.
#[derive(Default, Clone)]
struct AuditOutcome {
    license_id: Option<String>,
    before_state: Option<String>,
    after_state: Option<String>,
}

fn status_of(response: &Value) -> Option<String> {
    string_at(response, "/data/attributes/status").map(str::to_owned)
}

/// Capture the compact license status before each mutation. A failed read
/// cancels the operation so the audit journal can reconstruct the change.
fn capture_before_and_mutate<T>(
    id: &str,
    get: impl FnOnce(&str) -> Result<Value, String>,
    mutate: impl FnOnce() -> Result<T, String>,
) -> Result<(String, Result<T, String>), String> {
    let response = get(id)?;
    let before =
        status_of(&response).ok_or("Keygen did not return license status; mutation cancelled")?;
    Ok((before, mutate()))
}

fn execute(client: &AdminClient, command: Command) -> (AuditOutcome, Result<(), String>) {
    let mut outcome = AuditOutcome::default();
    let result = (|| -> Result<(), String> {
        match command {
            Command::Issue { name } => {
                let response = client.issue(name.as_deref())?;
                let id = string_at(&response, "/data/id").unwrap_or("<unknown>");
                let key = string_at(&response, "/data/attributes/key")
                    .ok_or("Keygen не вернул ключ новой лицензии")?;
                outcome.license_id = Some(id.to_owned());
                outcome.after_state = status_of(&response).or_else(|| Some("active".to_owned()));
                println!("Лицензия создана\nID: {id}\nКлюч: {key}");
            }
            Command::List { limit } => print_safe(client.list(limit)?)?,
            Command::Show { id } => print_safe(client.get(&id)?)?,
            Command::Suspend { id } => {
                let (before, response) = capture_before_and_mutate(
                    &id,
                    |id| client.get(id),
                    || client.action(&id, "suspend", Method::POST),
                )?;
                outcome.before_state = Some(before);
                let response = response?;
                outcome.license_id = Some(id.clone());
                outcome.after_state = status_of(&response);
                print_safe(response)?;
            }
            Command::Reinstate { id } => {
                let (before, response) = capture_before_and_mutate(
                    &id,
                    |id| client.get(id),
                    || client.action(&id, "reinstate", Method::POST),
                )?;
                outcome.before_state = Some(before);
                let response = response?;
                outcome.license_id = Some(id.clone());
                outcome.after_state = status_of(&response);
                print_safe(response)?;
            }
            Command::Renew { id } => {
                let (before, response) = capture_before_and_mutate(
                    &id,
                    |id| client.get(id),
                    || client.action(&id, "renew", Method::POST),
                )?;
                outcome.before_state = Some(before);
                let response = response?;
                outcome.license_id = Some(id.clone());
                outcome.after_state = status_of(&response);
                print_safe(response)?;
            }
            Command::ResetUsage { id, confirm } => {
                require_confirmation(&id, &confirm)?;
                let (before, response) = capture_before_and_mutate(
                    &id,
                    |id| client.get(id),
                    || client.action(&id, "reset-usage", Method::POST),
                )?;
                outcome.before_state = Some(before);
                let response = response?;
                outcome.license_id = Some(id.clone());
                outcome.after_state = Some("usage-reset".to_owned());
                print_safe(response)?;
            }
            Command::ResetMachines { id, confirm } => {
                require_confirmation(&id, &confirm)?;
                let (before, ids) = capture_before_and_mutate(
                    &id,
                    |id| client.get(id),
                    || {
                        let ids = client.list_all_machines(&id)?;
                        for machine_id in &ids {
                            client.delete_machine(machine_id)?;
                        }
                        Ok(ids)
                    },
                )?;
                outcome.before_state = Some(before);
                let ids = ids?;
                outcome.license_id = Some(id.clone());
                outcome.after_state = Some(format!("machines-deleted:{}", ids.len()));
                println!("Удалено активаций: {}", ids.len());
            }
            Command::Revoke { id, confirm } => {
                require_confirmation(&id, &confirm)?;
                let (before, result) = capture_before_and_mutate(
                    &id,
                    |id| client.get(id),
                    || client.action(&id, "revoke", Method::DELETE).map(|_| ()),
                )?;
                outcome.before_state = Some(before);
                result?;
                outcome.license_id = Some(id.clone());
                outcome.after_state = Some("revoked".to_owned());
                println!("Лицензия {id} безвозвратно отозвана.");
            }
            Command::MigrateAuditState => {
                return Err(
                    "Команда миграции audit-state должна выполняться до API-запросов".to_owned(),
                );
            }
        }
        Ok(())
    })();
    (outcome, result)
}

fn generate_correlation_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{nanos:032x}{:08x}", std::process::id())
}

#[derive(Debug)]
enum Command {
    Issue { name: Option<String> },
    List { limit: u8 },
    Show { id: String },
    Suspend { id: String },
    Reinstate { id: String },
    Renew { id: String },
    ResetUsage { id: String, confirm: String },
    ResetMachines { id: String, confirm: String },
    Revoke { id: String, confirm: String },
    MigrateAuditState,
}

impl Command {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let Some(action) = args.first().map(String::as_str) else {
            return Err(usage());
        };
        match action {
            "issue" => Ok(Self::Issue {
                name: option_value(&args[1..], "--name")?,
            }),
            "list" => {
                let limit = option_value(&args[1..], "--limit")?
                    .unwrap_or_else(|| "25".to_owned())
                    .parse::<u8>()
                    .map_err(|_| "--limit должен быть числом 1..100")?;
                if !(1..=100).contains(&limit) {
                    return Err("--limit должен быть числом 1..100".to_owned());
                }
                Ok(Self::List { limit })
            }
            "show" => Ok(Self::Show {
                id: required_id(&args)?,
            }),
            "suspend" => Ok(Self::Suspend {
                id: required_id(&args)?,
            }),
            "reinstate" => Ok(Self::Reinstate {
                id: required_id(&args)?,
            }),
            "renew" => Ok(Self::Renew {
                id: required_id(&args)?,
            }),
            "reset-usage" => destructive(&args, |id, confirm| Self::ResetUsage { id, confirm }),
            "reset-machines" => {
                destructive(&args, |id, confirm| Self::ResetMachines { id, confirm })
            }
            "revoke" => destructive(&args, |id, confirm| Self::Revoke { id, confirm }),
            "audit-migrate-state" => {
                let confirmation = option_value(&args[1..], "--confirm")?
                    .ok_or("Для миграции audit-state укажите --confirm MIGRATE-AUDIT-STATE")?;
                if confirmation != "MIGRATE-AUDIT-STATE" {
                    return Err(
                        "Для миграции audit-state требуется точное подтверждение MIGRATE-AUDIT-STATE"
                            .to_owned(),
                    );
                }
                Ok(Self::MigrateAuditState)
            }
            _ => Err(usage()),
        }
    }

    fn operation_name(&self) -> &'static str {
        match self {
            Self::Issue { .. } => "license.issue",
            Self::List { .. } => "license.list",
            Self::Show { .. } => "license.show",
            Self::Suspend { .. } => "license.suspend",
            Self::Reinstate { .. } => "license.reinstate",
            Self::Renew { .. } => "license.renew",
            Self::ResetUsage { .. } => "license.reset_usage",
            Self::ResetMachines { .. } => "license.reset_machines",
            Self::Revoke { .. } => "license.revoke",
            Self::MigrateAuditState => "audit.migrate_state",
        }
    }

    fn audit_target(&self) -> String {
        match self {
            Self::Issue { .. } | Self::List { .. } => "account".to_owned(),
            Self::Show { id }
            | Self::Suspend { id }
            | Self::Reinstate { id }
            | Self::Renew { id }
            | Self::ResetUsage { id, .. }
            | Self::ResetMachines { id, .. }
            | Self::Revoke { id, .. } => id.clone(),
            Self::MigrateAuditState => "audit".to_owned(),
        }
    }
}

fn destructive<T>(args: &[String], make: impl FnOnce(String, String) -> T) -> Result<T, String> {
    let id = required_id(args)?;
    let confirm = option_value(&args[2..], "--confirm")?
        .ok_or("Для необратимой операции укажите --confirm <тот-же-license-id>")?;
    Ok(make(id, confirm))
}

fn required_id(args: &[String]) -> Result<String, String> {
    args.get(1)
        .filter(|value| valid_id(value))
        .cloned()
        .ok_or_else(|| "Требуется корректный license ID".to_owned())
}

fn option_value(args: &[String], name: &str) -> Result<Option<String>, String> {
    let Some(index) = args.iter().position(|value| value == name) else {
        return Ok(None);
    };
    args.get(index + 1)
        .cloned()
        .map(Some)
        .ok_or_else(|| format!("После {name} требуется значение"))
}

fn require_confirmation(id: &str, confirmation: &str) -> Result<(), String> {
    if id == confirmation {
        Ok(())
    } else {
        Err("Подтверждение не совпадает с license ID; операция отменена".to_owned())
    }
}

fn usage() -> String {
    "Команды: issue [--name TEXT] | list [--limit N] | show ID | suspend ID | reinstate ID | renew ID | reset-usage ID --confirm ID | reset-machines ID --confirm ID | revoke ID --confirm ID | audit-migrate-state --confirm MIGRATE-AUDIT-STATE".to_owned()
}
struct AuditConfig {
    audit_key: String,
    audit_path: PathBuf,
}

impl AuditConfig {
    fn from_environment() -> Result<Self, String> {
        let audit_key = required_env("HEAT3_ADMIN_AUDIT_KEY")?;
        if audit_key.len() < 32 {
            return Err("Admin token or audit key is too short".to_owned());
        }
        let audit_path = env::var_os("HEAT3_ADMIN_AUDIT_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("audit/license-admin.jsonl"));
        Ok(Self {
            audit_key,
            audit_path,
        })
    }

    fn zeroize_secrets(&mut self) {
        self.audit_key.zeroize();
    }
}

impl Drop for AuditConfig {
    fn drop(&mut self) {
        self.zeroize_secrets();
    }
}

struct Config {
    base_url: Url,
    account: String,
    policy: String,
    token: String,
    audit_key: String,
    audit_path: PathBuf,
}

impl Config {
    fn from_environment() -> Result<Self, String> {
        let audit = AuditConfig::from_environment()?;
        let base_url = Url::parse(&required_env("HEAT3_ADMIN_API_URL")?)
            .map_err(|_| "HEAT3_ADMIN_API_URL is invalid")?;
        if base_url.scheme() != "https"
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || !matches!(base_url.path(), "" | "/")
        {
            return Err("HEAT3_ADMIN_API_URL must be a clean HTTPS origin".to_owned());
        }
        let account = required_env("HEAT3_ADMIN_ACCOUNT_ID")?;
        let policy = required_env("HEAT3_ADMIN_POLICY_ID")?;
        if !valid_id(&account) || !valid_id(&policy) {
            return Err("Account/policy ID contains invalid characters".to_owned());
        }
        let token = required_env("HEAT3_KEYGEN_ADMIN_TOKEN")?;
        if token.len() < 16 {
            return Err("Admin token is too short".to_owned());
        }
        Ok(Self {
            base_url,
            account,
            policy,
            token,
            audit_key: audit.audit_key.clone(),
            audit_path: audit.audit_path.clone(),
        })
    }

    fn zeroize_secrets(&mut self) {
        self.token.zeroize();
        self.audit_key.zeroize();
    }
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("Missing environment variable {name}"))
}

impl Drop for Config {
    fn drop(&mut self) {
        self.zeroize_secrets();
    }
}

struct AdminClient {
    http: Client,
    base: Url,
    auth: HeaderValue,
    policy: String,
}

impl AdminClient {
    fn new(config: &Config) -> Result<Self, String> {
        let mut base = config.base_url.clone();
        base.set_path(&format!("/v1/accounts/{}/", config.account));
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", config.token))
            .map_err(|_| "Admin token нельзя поместить в HTTP header")?;
        auth.set_sensitive(true);
        let http = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .user_agent("HEAT3-License-Admin/0.1")
            .build()
            .map_err(|_| "Не удалось создать HTTPS client")?;
        Ok(Self {
            http,
            base,
            auth,
            policy: config.policy.clone(),
        })
    }

    fn issue(&self, name: Option<&str>) -> Result<Value, String> {
        let mut data = json!({
            "type": "licenses",
            "relationships": {"policy": {"data": {"type": "policies", "id": self.policy}}}
        });
        if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
            data["attributes"] = json!({"name": name.chars().take(200).collect::<String>()});
        }
        self.request(Method::POST, "licenses", Some(json!({"data": data})))
    }

    fn list(&self, limit: u8) -> Result<Value, String> {
        self.request(Method::GET, &format!("licenses?limit={limit}"), None)
    }

    fn get(&self, id: &str) -> Result<Value, String> {
        self.request(Method::GET, &format!("licenses/{id}"), None)
    }

    fn action(&self, id: &str, action: &str, method: Method) -> Result<Value, String> {
        self.request(method, &format!("licenses/{id}/actions/{action}"), None)
    }

    /// Collects every machine id attached to a license, following Keygen's
    /// cursor pagination via `links.next` until the link is absent. Pagination
    /// is completed before any deletion so deleting one page's items cannot
    /// invalidate the cursor for the next page.
    fn list_all_machines(&self, license_id: &str) -> Result<Vec<String>, String> {
        let mut all_ids: Vec<String> = Vec::new();
        let mut next: Option<String> = Some(format!("machines?license={license_id}&limit=100"));
        let mut pages = 0u32;
        while let Some(suffix) = next.take() {
            let response = match suffix.strip_prefix("ABSOLUTE:") {
                Some(absolute) => self.request_absolute(Method::GET, absolute)?,
                None => self.request(Method::GET, &suffix, None)?,
            };
            let ids: Vec<String> = response
                .pointer("/data")
                .and_then(Value::as_array)
                .ok_or("Некорректный список машин")?
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect();
            if ids.is_empty() {
                break;
            }
            all_ids.extend(ids);
            if let Some(link) = response.pointer("/links/next").and_then(Value::as_str) {
                next = Some(format!("ABSOLUTE:{link}"));
            }
            pages += 1;
            if pages > 10_000 {
                return Err("Слишком много страниц машин".to_owned());
            }
        }
        Ok(all_ids)
    }

    /// Issues a GET to an absolute URL returned by Keygen (`links.next`) while
    /// enforcing the same HTTPS origin as the configured API base.
    fn request_absolute(&self, method: Method, url: &str) -> Result<Value, String> {
        let parsed = Url::parse(url).map_err(|_| "Некорректный URL пагинации Keygen")?;
        if parsed.scheme() != self.base.scheme()
            || parsed.host_str() != self.base.host_str()
            || parsed.port_or_known_default() != self.base.port_or_known_default()
        {
            return Err("Ссылка пагинации Keygen ведёт на другой origin".to_owned());
        }
        self.send(method, parsed)
    }

    fn delete_machine(&self, id: &str) -> Result<(), String> {
        self.request(Method::DELETE, &format!("machines/{id}"), None)
            .map(|_| ())
    }

    fn request(&self, method: Method, suffix: &str, body: Option<Value>) -> Result<Value, String> {
        if suffix.contains("..") || suffix.starts_with('/') {
            return Err("Некорректный API path".to_owned());
        }
        let url = self.base.join(suffix).map_err(|_| "Некорректный API URL")?;
        self.send_with_body(method, url, body)
    }

    fn send(&self, method: Method, url: Url) -> Result<Value, String> {
        self.send_with_body(method, url, None)
    }

    fn send_with_body(
        &self,
        method: Method,
        url: Url,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut request: RequestBuilder = self
            .http
            .request(method, url)
            .header(ACCEPT, JSON_API)
            .header(AUTHORIZATION, self.auth.clone());
        if let Some(body) = body {
            request = request.header(CONTENT_TYPE, JSON_API).json(&body);
        }
        let response = request.send().map_err(|error| {
            if error.is_timeout() {
                "Таймаут Keygen API"
            } else {
                "Ошибка соединения с Keygen API"
            }
            .to_owned()
        })?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Не удалось прочитать ответ Keygen")?;
        if bytes.len() > MAX_RESPONSE {
            return Err("Ответ Keygen слишком большой".to_owned());
        }
        if !status.is_success() {
            let code = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/errors/0/code")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            return Err(format!(
                "Keygen API {}{}",
                status.as_u16(),
                code.map(|c| format!(" ({c})")).unwrap_or_default()
            ));
        }
        if status == StatusCode::NO_CONTENT || bytes.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_slice(&bytes).map_err(|_| "Keygen вернул некорректный JSON".to_owned())
        }
    }
}

fn print_safe(mut value: Value) -> Result<(), String> {
    redact_secrets(&mut value);
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|_| "Ошибка вывода JSON")?
    );
    Ok(())
}

fn redact_secrets(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if matches!(key.as_str(), "key" | "token" | "secret") {
                    *child = Value::String("<redacted>".to_owned());
                } else {
                    redact_secrets(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_secrets),
        _ => {}
    }
}

fn string_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

type HmacSha256 = Hmac<Sha256>;

#[derive(Deserialize, Serialize)]
struct AuditEntry {
    timestamp: String,
    operator: String,
    operation: String,
    target: String,
    phase: String,
    success: Option<bool>,
    previous_mac: String,
    mac: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    correlation_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    workstation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    license_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after_state: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuditState {
    entry_count: u64,
    byte_len: u64,
    previous_mac: String,
    mac: String,
}

impl AuditState {
    fn signed(
        entry_count: u64,
        byte_len: u64,
        previous_mac: String,
        key: &[u8],
    ) -> Result<Self, String> {
        let mac = compute_audit_state_mac(key, entry_count, byte_len, &previous_mac)?;
        Ok(Self {
            entry_count,
            byte_len,
            previous_mac,
            mac,
        })
    }

    fn verify(&self, key: &[u8]) -> Result<(), String> {
        let expected =
            compute_audit_state_mac(key, self.entry_count, self.byte_len, &self.previous_mac)?;
        if self.mac == expected {
            Ok(())
        } else {
            Err("Подпись audit state не прошла проверку; операции заблокированы".to_owned())
        }
    }
}

struct AuditLog {
    file: fs::File,
    state_path: PathBuf,
    key: Vec<u8>,
    previous_mac: String,
    entry_count: u64,
    byte_len: u64,
}

impl Drop for AuditLog {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
        self.key.zeroize();
    }
}

#[cfg(test)]
impl AuditLog {
    fn append_test(
        &mut self,
        operation: &str,
        target: &str,
        phase: &str,
        success: Option<bool>,
    ) -> Result<(), String> {
        self.append(
            operation,
            target,
            phase,
            success,
            &generate_correlation_id(),
            &AuditOutcome::default(),
        )
    }
}

impl AuditLog {
    fn open(path: PathBuf, key: &[u8]) -> Result<Self, String> {
        let state_path = audit_state_path(&path);
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|_| "Не удалось создать каталог audit")?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|_| "Не удалось открыть audit log")?;
        file.lock_exclusive()
            .map_err(|_| "Audit log уже используется другим оператором")?;
        let state = verified_audit_state(&mut file, key)?;
        match load_audit_state(&state_path)? {
            Some(saved) => {
                saved.verify(key)?;
                if saved.entry_count == state.entry_count
                    && saved.byte_len == state.byte_len
                    && saved.previous_mac == state.previous_mac
                {
                    // The durable journal and its signed checkpoint agree.
                } else if saved.entry_count < state.entry_count
                    && saved.byte_len < state.byte_len
                    && audit_state_is_exact_prefix(&mut file, key, &saved)?
                {
                    // The entry was synced before the sidecar replacement. The verified
                    // HMAC-chain suffix is a crash-forward state, not a rollback.
                    persist_audit_state(&state_path, &state)?;
                } else {
                    return Err("Audit log усечён или заменён; операции заблокированы".to_owned());
                }
            }
            None if state.entry_count == 0 && state.byte_len == 0 => {}
            None => {
                return Err(
                    "Audit state отсутствует для непустого журнала; выполните явную миграцию"
                        .to_owned(),
                );
            }
        }
        Ok(Self {
            file,
            state_path,
            key: key.to_vec(),
            previous_mac: state.previous_mac,
            entry_count: state.entry_count,
            byte_len: state.byte_len,
        })
    }

    fn migrate_state(path: PathBuf, key: &[u8]) -> Result<(), String> {
        let state_path = audit_state_path(&path);
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|_| "Не удалось открыть существующий audit log для миграции")?;
        file.lock_exclusive()
            .map_err(|_| "Audit log уже используется другим оператором")?;
        if load_audit_state(&state_path)?.is_some() {
            return Err("Audit state уже существует; миграция не требуется".to_owned());
        }
        let state = verified_audit_state(&mut file, key)?;
        if state.entry_count == 0 || state.byte_len == 0 {
            return Err("Пустой audit log не требует миграции".to_owned());
        }
        let mut audit = Self {
            file,
            state_path,
            key: key.to_vec(),
            previous_mac: state.previous_mac,
            entry_count: state.entry_count,
            byte_len: state.byte_len,
        };
        audit.append(
            "audit.migrate_state",
            "audit",
            "result",
            Some(true),
            &generate_correlation_id(),
            &AuditOutcome::default(),
        )
    }

    fn append(
        &mut self,
        operation: &str,
        target: &str,
        phase: &str,
        success: Option<bool>,
        correlation_id: &str,
        outcome: &AuditOutcome,
    ) -> Result<(), String> {
        let operator = current_operator();
        let mut entry = AuditEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            operator: operator.chars().take(100).collect(),
            operation: operation.to_owned(),
            target: target.to_owned(),
            phase: phase.to_owned(),
            success,
            previous_mac: self.previous_mac.clone(),
            mac: String::new(),
            correlation_id: correlation_id.to_owned(),
            workstation: current_workstation(),
            license_id: outcome.license_id.clone(),
            before_state: outcome.before_state.clone(),
            after_state: outcome.after_state.clone(),
        };
        let payload = entry_without_mac(&entry)?;
        entry.mac = compute_mac(&self.key, &payload)?;
        let line = serde_json::to_string(&entry).map_err(|_| "Не удалось сериализовать audit")?;
        self.file
            .seek(SeekFrom::End(0))
            .map_err(|_| "Не удалось позиционировать audit log")?;
        writeln!(self.file, "{line}").map_err(|_| "Не удалось записать audit log")?;
        self.file
            .sync_all()
            .map_err(|_| "Не удалось синхронизировать audit log")?;
        self.previous_mac = entry.mac.clone();
        self.entry_count += 1;
        self.byte_len = self
            .file
            .metadata()
            .map_err(|_| "Не удалось прочитать размер audit log")?
            .len();
        let state = AuditState::signed(
            self.entry_count,
            self.byte_len,
            self.previous_mac.clone(),
            &self.key,
        )?;
        persist_audit_state(&self.state_path, &state)?;
        Ok(())
    }
}

fn verified_audit_state(file: &mut fs::File, key: &[u8]) -> Result<AuditState, String> {
    let mut content = String::new();
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "Не удалось позиционировать audit log")?;
    file.read_to_string(&mut content)
        .map_err(|_| "Не удалось прочитать audit log")?;
    let mut previous = "GENESIS".to_owned();
    let mut entry_count = 0u64;
    for line in content.lines() {
        let entry: AuditEntry = serde_json::from_str(line)
            .map_err(|_| "Audit log повреждён; операции заблокированы")?;
        let payload = entry_without_mac(&entry)?;
        if entry.previous_mac != previous || compute_mac(key, &payload)? != entry.mac {
            return Err("Цепочка audit log не прошла проверку; операции заблокированы".to_owned());
        }
        previous = entry.mac;
        entry_count += 1;
    }
    let byte_len = file
        .metadata()
        .map_err(|_| "Не удалось прочитать размер audit log")?
        .len();
    AuditState::signed(entry_count, byte_len, previous, key)
}

fn audit_state_is_exact_prefix(
    file: &mut fs::File,
    key: &[u8],
    saved: &AuditState,
) -> Result<bool, String> {
    let byte_len = file
        .metadata()
        .map_err(|_| "Не удалось прочитать размер audit log")?
        .len();
    if saved.byte_len > byte_len {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "Не удалось позиционировать audit log")?;
    let mut prefix = vec![0; saved.byte_len as usize];
    file.read_exact(&mut prefix)
        .map_err(|_| "Не удалось прочитать audit log")?;
    let text =
        std::str::from_utf8(&prefix).map_err(|_| "Audit log повреждён; операции заблокированы")?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Ok(false);
    }
    let mut previous = "GENESIS".to_owned();
    let mut entry_count = 0u64;
    for line in text.lines() {
        let entry: AuditEntry = serde_json::from_str(line)
            .map_err(|_| "Audit log повреждён; операции заблокированы")?;
        let payload = entry_without_mac(&entry)?;
        if entry.previous_mac != previous || compute_mac(key, &payload)? != entry.mac {
            return Ok(false);
        }
        previous = entry.mac;
        entry_count += 1;
    }
    Ok(entry_count == saved.entry_count && previous == saved.previous_mac)
}

fn audit_state_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.state", path.as_os_str().to_string_lossy()))
}

fn load_audit_state(path: &Path) -> Result<Option<AuditState>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "Audit state повреждён; операции заблокированы".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Не удалось прочитать audit state".to_owned()),
    }
}

fn persist_audit_state(path: &Path, state: &AuditState) -> Result<(), String> {
    let bytes =
        serde_json::to_vec(state).map_err(|_| "Не удалось сериализовать audit state".to_owned())?;
    atomic_write_bytes(path, &bytes)
}

/// Nanosecond timestamp plus a process-local counter: parallel writers in the
/// same nanosecond must still get distinct temp file names on Windows.
fn atomic_temp_suffix() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{}-{}-{}", std::process::id(), nanos, counter)
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Не удалось определить родительский каталог audit state".to_owned())?;
    let unique = atomic_temp_suffix();
    let temporary = parent.join(format!(".audit-state-{unique}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "Не удалось открыть временный audit state")?;
        file.write_all(bytes)
            .map_err(|_| "Не удалось записать audit state")?;
        file.sync_all()
            .map_err(|_| "Не удалось синхронизировать audit state")?;
        atomic_replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let ok = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err("Не удалось заменить audit state".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|_| "Не удалось удалить старый audit state".to_owned())?;
    }
    fs::rename(source, destination).map_err(|_| "Не удалось заменить audit state".to_owned())
}

#[cfg(windows)]
fn current_operator() -> String {
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

    let mut buffer = vec![0u16; 256];
    let mut length = buffer.len() as u32;
    let ok = unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut length) };
    if ok != 0 && length > 0 {
        return String::from_utf16_lossy(&buffer[..length.saturating_sub(1) as usize]);
    }
    env::var("USERNAME").unwrap_or_else(|_| "unknown".to_owned())
}

#[cfg(not(windows))]
fn current_operator() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn current_workstation() -> String {
    env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_owned())
        .chars()
        .take(100)
        .collect()
}

fn audit_payload(entry: &AuditEntry) -> Result<serde_json::Map<String, Value>, String> {
    // New contextual fields are inserted only when present so that entries
    // written by older tool versions keep verifying against their original
    // HMAC payload.
    let mut payload = serde_json::Map::new();
    payload.insert("timestamp".to_owned(), json!(entry.timestamp));
    payload.insert("operator".to_owned(), json!(entry.operator));
    payload.insert("operation".to_owned(), json!(entry.operation));
    payload.insert("target".to_owned(), json!(entry.target));
    payload.insert("phase".to_owned(), json!(entry.phase));
    payload.insert("success".to_owned(), json!(entry.success));
    payload.insert("previous_mac".to_owned(), json!(entry.previous_mac));
    if !entry.correlation_id.is_empty() {
        payload.insert("correlation_id".to_owned(), json!(entry.correlation_id));
    }
    if !entry.workstation.is_empty() {
        payload.insert("workstation".to_owned(), json!(entry.workstation));
    }
    if let Some(license_id) = &entry.license_id {
        payload.insert("license_id".to_owned(), json!(license_id));
    }
    if let Some(before_state) = &entry.before_state {
        payload.insert("before_state".to_owned(), json!(before_state));
    }
    if let Some(after_state) = &entry.after_state {
        payload.insert("after_state".to_owned(), json!(after_state));
    }
    Ok(payload)
}

fn entry_without_mac(entry: &AuditEntry) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&Value::Object(audit_payload(entry)?))
        .map_err(|_| "Не удалось вычислить audit MAC".to_owned())
}

fn compute_audit_state_mac(
    key: &[u8],
    entry_count: u64,
    byte_len: u64,
    previous_mac: &str,
) -> Result<String, String> {
    let payload = serde_json::to_vec(&json!({
        "domain": "heat3.audit-state.v1",
        "entry_count": entry_count,
        "byte_len": byte_len,
        "previous_mac": previous_mac,
    }))
    .map_err(|_| "Не удалось вычислить подпись audit state")?;
    compute_mac(key, &payload)
}

fn compute_mac(key: &[u8], bytes: &[u8]) -> Result<String, String> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| "Некорректный audit key")?;
    mac.update(bytes);
    Ok(hex_lower(&mac.finalize().into_bytes()))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_state_recovers_a_valid_synced_suffix_after_a_crash() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-recovery-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            log.append_test("license.show", "license-1", "attempt", None)
                .unwrap();
        }
        let stale_state = fs::read(&state_path).unwrap();
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            log.append_test("license.show", "license-1", "result", Some(true))
                .unwrap();
            log.append_test("license.list", "account", "attempt", None)
                .unwrap();
        }
        fs::write(&state_path, stale_state).unwrap();

        assert!(AuditLog::open(path.clone(), key).is_ok());
        let state = load_audit_state(&state_path).unwrap().unwrap();
        assert_eq!(state.entry_count, 3);
        state.verify(key).unwrap();

        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }
    use std::sync::{Mutex, OnceLock};

    fn environment_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn destructive_command_requires_exact_confirmation() {
        assert!(Command::parse(vec!["revoke".into(), "license-1".into()]).is_err());
        let command = Command::parse(vec![
            "revoke".into(),
            "license-1".into(),
            "--confirm".into(),
            "license-1".into(),
        ])
        .unwrap();
        assert!(matches!(command, Command::Revoke { .. }));
        assert!(require_confirmation("a", "b").is_err());
    }

    #[test]
    fn audit_state_migration_requires_explicit_confirmation() {
        assert!(Command::parse(vec!["audit-migrate-state".into()]).is_err());
        assert!(Command::parse(vec![
            "audit-migrate-state".into(),
            "--confirm".into(),
            "wrong".into(),
        ])
        .is_err());
        assert!(Command::parse(vec![
            "audit-migrate-state".into(),
            "--confirm".into(),
            "MIGRATE-AUDIT-STATE".into(),
        ])
        .is_ok());
    }

    #[test]
    fn audit_migration_config_does_not_require_api_credentials() {
        let _guard = environment_lock().lock().unwrap();
        env::remove_var("HEAT3_ADMIN_API_URL");
        env::remove_var("HEAT3_ADMIN_ACCOUNT_ID");
        env::remove_var("HEAT3_ADMIN_POLICY_ID");
        env::remove_var("HEAT3_KEYGEN_ADMIN_TOKEN");
        env::set_var("HEAT3_ADMIN_AUDIT_KEY", "01234567890123456789012345678901");
        env::set_var(
            "HEAT3_ADMIN_AUDIT_PATH",
            env::temp_dir().join("heat3-admin-config-test.jsonl"),
        );

        let config = AuditConfig::from_environment().unwrap();
        assert_eq!(config.audit_key, "01234567890123456789012345678901");
    }

    #[test]
    fn recursive_redaction_removes_keys_and_tokens() {
        let mut value =
            json!({"data": {"attributes": {"key": "SECRET", "name": "safe"}}, "token": "TOKEN"});
        redact_secrets(&mut value);
        let output = value.to_string();
        assert!(!output.contains("SECRET"));
        assert!(!output.contains("TOKEN"));
        assert!(output.contains("safe"));
    }

    #[test]
    fn audit_chain_detects_modification() {
        let path = env::temp_dir().join(format!("heat3-admin-audit-{}.jsonl", std::process::id()));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(audit_state_path(&path));
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            log.append_test("license.show", "license-1", "attempt", None)
                .unwrap();
            log.append_test("license.show", "license-1", "result", Some(true))
                .unwrap();
        }
        assert!(AuditLog::open(path.clone(), key).is_ok());
        let mut content = fs::read_to_string(&path).unwrap();
        content = content.replacen("\"phase\":\"result\"", "\"phase\":\"tampered\"", 1);
        fs::write(&path, content).unwrap();
        assert!(AuditLog::open(path.clone(), key).is_err());
        let _ = fs::remove_file(audit_state_path(&path));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn audit_state_detects_truncation() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-truncate-{}.jsonl",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(audit_state_path(&path));
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            log.append_test("license.show", "license-1", "attempt", None)
                .unwrap();
            log.append_test("license.show", "license-1", "result", Some(true))
                .unwrap();
        }

        let content = fs::read_to_string(&path).unwrap();
        let first_line = content.lines().next().unwrap().to_owned();
        fs::write(&path, format!("{first_line}\n")).unwrap();

        assert!(AuditLog::open(path.clone(), key).is_err());
        let _ = fs::remove_file(audit_state_path(&path));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn nonempty_audit_requires_existing_state() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-missing-state-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            log.append_test("license.show", "license-1", "attempt", None)
                .unwrap();
        }

        fs::remove_file(&state_path).unwrap();

        assert!(AuditLog::open(path.clone(), key).is_err());
        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn forged_audit_state_cannot_hide_truncation() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-forged-state-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            log.append_test("license.show", "license-1", "attempt", None)
                .unwrap();
            log.append_test("license.show", "license-1", "result", Some(true))
                .unwrap();
        }

        let content = fs::read_to_string(&path).unwrap();
        let first_line = content.lines().next().unwrap();
        let first_entry: AuditEntry = serde_json::from_str(first_line).unwrap();
        let truncated = format!("{first_line}\n");
        fs::write(&path, truncated.as_bytes()).unwrap();
        fs::write(
            &state_path,
            serde_json::to_vec(&json!({
                "entry_count": 1,
                "byte_len": truncated.len(),
                "previous_mac": first_entry.mac,
                "mac": "forged"
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(AuditLog::open(path.clone(), key).is_err());
        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn explicit_migration_creates_authenticated_audit_state() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-migration-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            log.append_test("license.show", "license-1", "attempt", None)
                .unwrap();
        }
        fs::remove_file(&state_path).unwrap();

        AuditLog::migrate_state(path.clone(), key).unwrap();

        let state = load_audit_state(&state_path).unwrap().unwrap();
        state.verify(key).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let entries = content
            .lines()
            .map(|line| serde_json::from_str::<AuditEntry>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].operation, "audit.migrate_state");
        assert_eq!(entries[1].phase, "result");
        assert_eq!(entries[1].success, Some(true));
        assert!(AuditLog::open(path.clone(), key).is_ok());
        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }

    fn read_entries(path: &Path) -> Vec<AuditEntry> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<AuditEntry>(line).unwrap())
            .collect()
    }

    #[test]
    fn mutations_are_blocked_when_before_state_cannot_be_captured() {
        let mutated = std::cell::Cell::new(false);
        let result = capture_before_and_mutate(
            "license-1",
            |_| Err("injected GET failure".to_owned()),
            || {
                mutated.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(
            !mutated.get(),
            "mutation must not run after a failed state read"
        );

        let result = capture_before_and_mutate(
            "license-1",
            |_| Ok(json!({"data": {"attributes": {}}})),
            || {
                mutated.set(true);
                Ok(())
            },
        );
        assert!(result.is_err(), "missing status must also fail closed");
        assert!(!mutated.get());
    }

    #[test]
    fn captured_before_state_is_retained_when_the_mutation_fails() {
        let (before, mutation) = capture_before_and_mutate(
            "license-1",
            |_| Ok(json!({"data": {"attributes": {"status": "active"}}})),
            || Err::<(), _>("injected action failure".to_owned()),
        )
        .unwrap();

        assert_eq!(before, "active");
        assert_eq!(mutation.unwrap_err(), "injected action failure");
    }

    #[test]
    fn consecutive_issue_operations_map_to_their_license_ids() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-issue-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            for (correlation, license_id) in [("corr-1", "license-aaa"), ("corr-2", "license-bbb")]
            {
                log.append(
                    "license.issue",
                    "account",
                    "attempt",
                    None,
                    correlation,
                    &AuditOutcome::default(),
                )
                .unwrap();
                let outcome = AuditOutcome {
                    license_id: Some(license_id.to_owned()),
                    before_state: None,
                    after_state: Some("active".to_owned()),
                };
                log.append(
                    "license.issue",
                    "account",
                    "result",
                    Some(true),
                    correlation,
                    &outcome,
                )
                .unwrap();
            }
        }

        let entries = read_entries(&path);
        let correlation_ids: Vec<&str> = entries
            .iter()
            .map(|entry| entry.correlation_id.as_str())
            .collect();
        assert!(correlation_ids.iter().all(|id| !id.is_empty()));
        let paired = entries
            .chunks(2)
            .map(|pair| (pair[0].correlation_id.clone(), pair[1].license_id.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            paired,
            vec![
                ("corr-1".to_owned(), Some("license-aaa".to_owned())),
                ("corr-2".to_owned(), Some("license-bbb".to_owned())),
            ]
        );
        assert!(entries.iter().all(|entry| !entry.workstation.is_empty()));
        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn before_state_is_captured_and_hmac_covered() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-before-state-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            let outcome = AuditOutcome {
                license_id: Some("license-dd".to_owned()),
                before_state: Some("active".to_owned()),
                after_state: Some("suspended".to_owned()),
            };
            log.append(
                "license.suspend",
                "account",
                "result",
                Some(true),
                "corr-s",
                &outcome,
            )
            .unwrap();
        }
        let content = fs::read_to_string(&path).unwrap();
        let entry: AuditEntry = serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert_eq!(entry.before_state.as_deref(), Some("active"));
        assert_eq!(entry.after_state.as_deref(), Some("suspended"));
        // The captured before/after values must be covered by the entry MAC:
        // removing them from the payload must change the MAC.
        let mut without_before = audit_payload(&entry).unwrap();
        without_before.remove("before_state");
        let mac_without_before = compute_mac(
            key,
            &serde_json::to_vec(&Value::Object(without_before)).unwrap(),
        )
        .unwrap();
        assert_ne!(&mac_without_before, &entry.mac);
        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn audit_entries_never_contain_license_keys_or_tokens() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-secrets-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        let secret = "SECRET-LICENSE-KEY-0123456789";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            let outcome = AuditOutcome {
                license_id: Some("license-cc".to_owned()),
                before_state: None,
                after_state: Some("active".to_owned()),
            };
            log.append(
                "license.issue",
                "account",
                "attempt",
                None,
                "corr-x",
                &outcome,
            )
            .unwrap();
            log.append(
                "license.issue",
                "account",
                "result",
                Some(true),
                "corr-x",
                &outcome,
            )
            .unwrap();
            assert!(!secret.is_empty());
        }
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains(secret));
        assert!(!content.contains("Bearer "));
        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn tampering_with_audit_context_is_detected() {
        let path = env::temp_dir().join(format!(
            "heat3-admin-audit-context-{}.jsonl",
            std::process::id()
        ));
        let state_path = audit_state_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&state_path);
        let key = b"01234567890123456789012345678901";
        {
            let mut log = AuditLog::open(path.clone(), key).unwrap();
            let outcome = AuditOutcome {
                license_id: Some("license-original".to_owned()),
                before_state: None,
                after_state: Some("active".to_owned()),
            };
            log.append(
                "license.issue",
                "account",
                "attempt",
                None,
                "corr-y",
                &outcome,
            )
            .unwrap();
        }
        let mut content = fs::read_to_string(&path).unwrap();
        content = content.replacen("license-original", "license-tampered", 1);
        fs::write(&path, content).unwrap();

        assert!(AuditLog::open(path.clone(), key).is_err());
        let _ = fs::remove_file(state_path);
        let _ = fs::remove_file(path);
    }
}
