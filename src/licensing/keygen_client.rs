use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, DATE};
use reqwest::{Method, StatusCode, Url};
use serde_json::{json, Value};
use std::fmt;
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    HardwareIdentity, MachineCertificateError, MachineCertificateVerifier, MachineFileContext,
    ResponseSignatureError, ResponseSignatureVerifier, SecureLicenseRecord, SignedResponse,
    VerifiedMachineFile, MACHINE_FILE_ALGORITHM,
};

const JSON_API: &str = "application/vnd.api+json";
const MAX_RESPONSE_SIZE: usize = 16 * 1024 * 1024;
const DEFAULT_OFFLINE_TTL: i64 = 7 * 24 * 60 * 60;

#[derive(Clone)]
pub struct KeygenConfig {
    base_url: Url,
    account_id: String,
    product_id: String,
    policy_id: String,
    public_key: String,
    offline_ttl: i64,
}

impl KeygenConfig {
    pub fn new(
        base_url: &str,
        account_id: impl Into<String>,
        product_id: impl Into<String>,
        policy_id: impl Into<String>,
        public_key: impl Into<String>,
        offline_ttl: i64,
    ) -> Result<Self, KeygenClientError> {
        let base_url = Url::parse(base_url).map_err(|_| KeygenClientError::InvalidConfiguration)?;
        let account_id = account_id.into();
        let product_id = product_id.into();
        let policy_id = policy_id.into();
        let public_key = public_key.into();

        if base_url.scheme() != "https"
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || !matches!(base_url.path(), "" | "/")
            || !valid_identifier(&account_id)
            || !valid_identifier(&product_id)
            || !valid_identifier(&policy_id)
            || !(3600..=31 * 24 * 60 * 60).contains(&offline_ttl)
        {
            return Err(KeygenClientError::InvalidConfiguration);
        }

        ResponseSignatureVerifier::from_encoded_public_key(&account_id, &public_key)?;
        MachineCertificateVerifier::from_encoded_public_key(&public_key)?;
        Ok(Self {
            base_url,
            account_id,
            product_id,
            policy_id,
            public_key,
            offline_ttl,
        })
    }

    pub fn from_compile_time() -> Result<Self, KeygenClientError> {
        let base_url = option_env!("HEAT3_KEYGEN_API_URL")
            .ok_or(KeygenClientError::MissingBuildConfiguration)?;
        let account = option_env!("HEAT3_KEYGEN_ACCOUNT_ID")
            .ok_or(KeygenClientError::MissingBuildConfiguration)?;
        let product = option_env!("HEAT3_KEYGEN_PRODUCT_ID")
            .ok_or(KeygenClientError::MissingBuildConfiguration)?;
        let policy = option_env!("HEAT3_KEYGEN_POLICY_ID")
            .ok_or(KeygenClientError::MissingBuildConfiguration)?;
        let public_key = option_env!("HEAT3_KEYGEN_PUBLIC_KEY")
            .ok_or(KeygenClientError::MissingBuildConfiguration)?;
        Self::new(
            base_url,
            account,
            product,
            policy,
            public_key,
            DEFAULT_OFFLINE_TTL,
        )
    }

    pub fn offline_ttl(&self) -> i64 {
        self.offline_ttl
    }

    fn endpoint(&self, suffix: &str) -> Result<Url, KeygenClientError> {
        if suffix.starts_with('/') || suffix.contains("..") {
            return Err(KeygenClientError::InvalidConfiguration);
        }
        let mut url = self.base_url.clone();
        url.set_path(&format!("/v1/accounts/{}/{}", self.account_id, suffix));
        Ok(url)
    }
}

pub struct KeygenClient {
    config: KeygenConfig,
    http: Client,
    response_verifier: ResponseSignatureVerifier,
    certificate_verifier: MachineCertificateVerifier,
    /// When true, authenticated responses are accepted even if their `Date`
    /// disagrees with a known-rolled-back local clock; the signed server time
    /// is then used to repair the trusted-time floor.
    clock_recovery: bool,
}

impl KeygenClient {
    pub fn new(config: KeygenConfig) -> Result<Self, KeygenClientError> {
        let response_verifier = ResponseSignatureVerifier::from_encoded_public_key(
            &config.account_id,
            &config.public_key,
        )?;
        let certificate_verifier =
            MachineCertificateVerifier::from_encoded_public_key(&config.public_key)?;
        let http = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("HEAT3-Povorotnik/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| KeygenClientError::Transport)?;
        Ok(Self {
            config,
            http,
            response_verifier,
            certificate_verifier,
            clock_recovery: false,
        })
    }

    /// Enables clock-rollback recovery for this client: authenticated responses
    /// are accepted regardless of local-clock disagreement, and the signed
    /// server time is returned so the caller can repair its trusted floor.
    pub fn with_clock_recovery(mut self, enabled: bool) -> Self {
        self.clock_recovery = enabled;
        self
    }

    pub fn validate_key(
        &self,
        license_key: &str,
        hardware: &HardwareIdentity,
    ) -> Result<ValidationResult, KeygenClientError> {
        validate_license_key(license_key)?;
        if hardware.fingerprint.is_empty() || hardware.components.is_empty() {
            return Err(KeygenClientError::InvalidHardwareIdentity);
        }

        let url = self.config.endpoint("licenses/actions/validate-key")?;
        let body = validation_body(&self.config, license_key, hardware);
        let response = self.execute(
            Method::POST,
            self.http
                .post(url)
                .header(CONTENT_TYPE, JSON_API)
                .json(&body),
        )?;
        parse_validation(&response.body, response.server_time)
    }

    pub fn activate_machine(
        &self,
        license_key: &str,
        license_id: &str,
        hardware: &HardwareIdentity,
        machine_name: &str,
    ) -> Result<MachineActivation, KeygenClientError> {
        validate_license_key(license_key)?;
        validate_resource_id(license_id)?;
        if hardware.fingerprint.is_empty() || hardware.components.is_empty() {
            return Err(KeygenClientError::InvalidHardwareIdentity);
        }

        let components = hardware
            .components
            .iter()
            .map(|component| {
                json!({
                    "type": "components",
                    "attributes": {
                        "fingerprint": component.digest,
                        "name": component.kind.as_str(),
                    }
                })
            })
            .collect::<Vec<_>>();
        let body = json!({
            "data": {
                "type": "machines",
                "attributes": {
                    "fingerprint": hardware.fingerprint,
                    "platform": "Windows",
                    "name": safe_machine_name(machine_name),
                    "metadata": {
                        "hardwareSchema": hardware.schema,
                        "clientVersion": env!("CARGO_PKG_VERSION"),
                    }
                },
                "relationships": {
                    "license": {
                        "data": {"type": "licenses", "id": license_id}
                    },
                    "components": {"data": components}
                }
            }
        });

        let url = self.config.endpoint("machines")?;
        let response = self.execute(
            Method::POST,
            with_license_auth(
                self.http
                    .post(url)
                    .header(CONTENT_TYPE, JSON_API)
                    .json(&body),
                license_key,
            )?,
        )?;
        parse_machine(
            &response.body,
            license_id,
            &hardware.fingerprint,
            response.server_time,
        )
    }

    pub fn find_machine(
        &self,
        license_key: &str,
        license_id: &str,
        fingerprint: &str,
    ) -> Result<Option<MachineActivation>, KeygenClientError> {
        validate_license_key(license_key)?;
        validate_resource_id(license_id)?;
        if fingerprint.is_empty() {
            return Err(KeygenClientError::InvalidHardwareIdentity);
        }
        let mut url = self.config.endpoint("machines")?;
        url.query_pairs_mut()
            .append_pair("fingerprint", fingerprint);
        let response = self.execute(
            Method::GET,
            with_license_auth(self.http.get(url), license_key)?,
        )?;
        parse_machine_list(
            &response.body,
            license_id,
            fingerprint,
            response.server_time,
        )
    }

    pub fn check_in(&self, license_key: &str, license_id: &str) -> Result<i64, KeygenClientError> {
        validate_license_key(license_key)?;
        validate_resource_id(license_id)?;
        let url = self
            .config
            .endpoint(&format!("licenses/{license_id}/actions/check-in"))?;
        let response = self.execute(
            Method::POST,
            with_license_auth(self.http.post(url), license_key)?,
        )?;
        Ok(response.server_time)
    }

    pub fn checkout_machine(
        &self,
        license_key: &str,
        license_id: &str,
        machine_id: &str,
        fingerprint: &str,
    ) -> Result<CheckedOutMachine, KeygenClientError> {
        validate_license_key(license_key)?;
        validate_resource_id(license_id)?;
        validate_resource_id(machine_id)?;
        if fingerprint.is_empty() {
            return Err(KeygenClientError::InvalidHardwareIdentity);
        }

        let mut url = self
            .config
            .endpoint(&format!("machines/{machine_id}/actions/check-out"))?;
        url.query_pairs_mut()
            .append_pair("ttl", &self.config.offline_ttl.to_string())
            .append_pair("include", "license")
            .append_pair("algorithm", MACHINE_FILE_ALGORITHM);
        let response = self.execute(
            Method::GET,
            with_license_auth(self.http.get(url), license_key)?,
        )?;
        let verified = self.certificate_verifier.verify_and_decrypt(
            &response.body,
            &MachineFileContext {
                account_id: &self.config.account_id,
                product_id: &self.config.product_id,
                policy_id: &self.config.policy_id,
                license_id,
                machine_id,
                fingerprint,
                license_key,
                now: response.server_time,
                maximum_ttl: self.config.offline_ttl,
            },
        )?;
        Ok(CheckedOutMachine {
            certificate: response.body,
            verified,
            server_time: response.server_time,
        })
    }

    pub fn deactivate_machine(
        &self,
        license_key: &str,
        machine_id: &str,
    ) -> Result<i64, KeygenClientError> {
        validate_license_key(license_key)?;
        validate_resource_id(machine_id)?;
        let url = self.config.endpoint(&format!("machines/{machine_id}"))?;
        let response = self.execute(
            Method::DELETE,
            with_license_auth(self.http.delete(url), license_key)?,
        )?;
        if !response.body.is_empty() {
            return Err(KeygenClientError::InvalidResponse);
        }
        Ok(response.server_time)
    }

    /// Verifies a cached machine file without contacting Keygen.
    ///
    /// All identity fields come from the DPAPI-protected record, while the
    /// authoritative binding and expiry are taken only from the signed file.
    pub fn verify_cached_machine(
        &self,
        record: &SecureLicenseRecord,
        now: i64,
    ) -> Result<VerifiedMachineFile, KeygenClientError> {
        let verified = self.certificate_verifier.verify_and_decrypt(
            record.encrypted_signed_machine_file(),
            &MachineFileContext {
                account_id: &self.config.account_id,
                product_id: &self.config.product_id,
                policy_id: &self.config.policy_id,
                license_id: record.license_id(),
                machine_id: record.machine_id(),
                fingerprint: record.activation_fingerprint(),
                license_key: record.license_key(),
                now,
                maximum_ttl: self.config.offline_ttl,
            },
        )?;
        if verified.expires_at != record.offline_valid_until()
            || verified.issued_at > record.last_successful_online_server_time()
        {
            return Err(KeygenClientError::ContextMismatch);
        }
        Ok(verified)
    }

    fn execute(
        &self,
        method: Method,
        request: RequestBuilder,
    ) -> Result<TrustedResponse, KeygenClientError> {
        let response = request
            .header(ACCEPT, JSON_API)
            .header("Keygen-Accept-Signature", "algorithm=\"ed25519\"")
            .send()
            .map_err(classify_transport)?;
        let status = response.status();
        let url = response.url().clone();
        // A reverse proxy, tunnel, or load balancer in front of Keygen can
        // generate its own unsigned 429/5xx page during an outage. Such a
        // response must never become an authoritative licensing decision, but
        // it can be classified as a transient outage so a still-valid signed
        // offline lease is preserved instead of being dropped.
        let has_signature_headers = response.headers().contains_key(DATE.as_str())
            && response.headers().contains_key("digest")
            && response.headers().contains_key("keygen-signature");
        if !status.is_success() && !has_signature_headers && is_transient_status(status) {
            return Err(KeygenClientError::Api {
                status: status.as_u16(),
                code: None,
            });
        }
        let date = header(&response, DATE.as_str())?;
        let digest = header(&response, "digest")?;
        let signature = header(&response, "keygen-signature")?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_SIZE as u64)
        {
            return Err(KeygenClientError::ResponseTooLarge);
        }
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE_SIZE as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| KeygenClientError::Transport)?;
        if body.len() > MAX_RESPONSE_SIZE {
            return Err(KeygenClientError::ResponseTooLarge);
        }

        let path_and_query = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        };
        let host = signed_host(&url)?;
        let signed = SignedResponse {
            method: method.as_str(),
            path_and_query: &path_and_query,
            host: &host,
            date: &date,
            digest: &digest,
            signature: &signature,
            body: &body,
        };
        let now = SystemTime::now();
        let server_time = if self.clock_recovery {
            self.response_verifier.verify_clock_recovery(&signed, now)?
        } else {
            self.response_verifier.verify(&signed, now)?
        };
        let server_time = server_time
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_secs()).ok())
            .ok_or(KeygenClientError::InvalidResponse)?;

        if !status.is_success() {
            return Err(parse_api_error(status, &body));
        }
        Ok(TrustedResponse { body, server_time })
    }
}

struct TrustedResponse {
    body: Vec<u8>,
    server_time: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineActivation {
    pub machine_id: String,
    pub server_time: i64,
}

pub struct CheckedOutMachine {
    pub certificate: Vec<u8>,
    pub verified: VerifiedMachineFile,
    pub server_time: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationResult {
    pub valid: bool,
    pub code: ValidationCode,
    pub license_id: Option<String>,
    pub server_time: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationCode {
    Valid,
    NotFound,
    Suspended,
    Expired,
    Overdue,
    NoMachine,
    TooManyMachines,
    FingerprintMismatch,
    ComponentsMismatch,
    ScopeRequired,
    Other(String),
}

impl ValidationCode {
    fn parse(value: &str) -> Self {
        match value {
            "VALID" => Self::Valid,
            "NOT_FOUND" => Self::NotFound,
            "SUSPENDED" => Self::Suspended,
            "EXPIRED" => Self::Expired,
            "OVERDUE" => Self::Overdue,
            "NO_MACHINE" | "NO_MACHINES" => Self::NoMachine,
            "TOO_MANY_MACHINES" => Self::TooManyMachines,
            "FINGERPRINT_SCOPE_MISMATCH" => Self::FingerprintMismatch,
            "COMPONENTS_SCOPE_MISMATCH" => Self::ComponentsMismatch,
            value if value.ends_with("_SCOPE_REQUIRED") => Self::ScopeRequired,
            other => Self::Other(other.to_owned()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeygenClientError {
    MissingBuildConfiguration,
    InvalidConfiguration,
    InvalidLicenseKey,
    InvalidResourceId,
    InvalidHardwareIdentity,
    Timeout,
    Connection,
    Transport,
    ResponseTooLarge,
    InvalidResponse,
    ContextMismatch,
    Api { status: u16, code: Option<String> },
    Signature(ResponseSignatureError),
    Certificate(MachineCertificateError),
}

impl fmt::Display for KeygenClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBuildConfiguration => f.write_str("Keygen build configuration is missing"),
            Self::InvalidConfiguration => f.write_str("Keygen configuration is invalid"),
            Self::InvalidLicenseKey => f.write_str("license key format is invalid"),
            Self::InvalidResourceId => f.write_str("Keygen resource id is invalid"),
            Self::InvalidHardwareIdentity => f.write_str("hardware identity is invalid"),
            Self::Timeout => f.write_str("Keygen request timed out"),
            Self::Connection => f.write_str("Keygen service is unreachable"),
            Self::Transport => f.write_str("Keygen transport error"),
            Self::ResponseTooLarge => f.write_str("Keygen response exceeds the size limit"),
            Self::InvalidResponse => f.write_str("Keygen response is invalid"),
            Self::ContextMismatch => f.write_str("Keygen response context does not match"),
            Self::Api { status, code } => {
                write!(f, "Keygen API error {status}")?;
                if let Some(code) = code {
                    write!(f, " ({code})")?;
                }
                Ok(())
            }
            Self::Signature(error) => write!(f, "{error}"),
            Self::Certificate(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for KeygenClientError {}

impl From<ResponseSignatureError> for KeygenClientError {
    fn from(error: ResponseSignatureError) -> Self {
        Self::Signature(error)
    }
}

impl From<MachineCertificateError> for KeygenClientError {
    fn from(error: MachineCertificateError) -> Self {
        Self::Certificate(error)
    }
}

fn validation_body(config: &KeygenConfig, license_key: &str, hardware: &HardwareIdentity) -> Value {
    json!({
        "meta": {
            "key": license_key,
            "scope": {
                "product": config.product_id,
                "policy": config.policy_id,
                "fingerprint": hardware.fingerprint,
                "components": hardware.components
                    .iter()
                    .map(|component| component.digest.as_str())
                    .collect::<Vec<_>>(),
            }
        }
    })
}

fn parse_validation(body: &[u8], server_time: i64) -> Result<ValidationResult, KeygenClientError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| KeygenClientError::InvalidResponse)?;
    let reported_valid = value
        .pointer("/meta/valid")
        .and_then(Value::as_bool)
        .ok_or(KeygenClientError::InvalidResponse)?;
    let raw_code = value
        .pointer("/meta/code")
        .and_then(Value::as_str)
        .ok_or(KeygenClientError::InvalidResponse)?;
    let code = ValidationCode::parse(raw_code);
    let license_id = value
        .pointer("/data/id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let valid = reported_valid && code == ValidationCode::Valid && license_id.is_some();
    Ok(ValidationResult {
        valid,
        code,
        license_id,
        server_time,
    })
}

fn parse_machine(
    body: &[u8],
    expected_license: &str,
    expected_fingerprint: &str,
    server_time: i64,
) -> Result<MachineActivation, KeygenClientError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| KeygenClientError::InvalidResponse)?;
    if value.pointer("/data/type").and_then(Value::as_str) != Some("machines")
        || value
            .pointer("/data/attributes/fingerprint")
            .and_then(Value::as_str)
            != Some(expected_fingerprint)
        || value
            .pointer("/data/relationships/license/data/id")
            .and_then(Value::as_str)
            != Some(expected_license)
    {
        return Err(KeygenClientError::ContextMismatch);
    }
    let machine_id = value
        .pointer("/data/id")
        .and_then(Value::as_str)
        .filter(|id| valid_identifier(id))
        .ok_or(KeygenClientError::InvalidResponse)?;
    Ok(MachineActivation {
        machine_id: machine_id.to_owned(),
        server_time,
    })
}

fn parse_machine_list(
    body: &[u8],
    expected_license: &str,
    expected_fingerprint: &str,
    server_time: i64,
) -> Result<Option<MachineActivation>, KeygenClientError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| KeygenClientError::InvalidResponse)?;
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(KeygenClientError::InvalidResponse)?;
    if items.len() > 1 {
        return Err(KeygenClientError::ContextMismatch);
    }
    let Some(item) = items.first() else {
        return Ok(None);
    };
    let wrapped = json!({"data": item});
    parse_machine(
        &serde_json::to_vec(&wrapped).map_err(|_| KeygenClientError::InvalidResponse)?,
        expected_license,
        expected_fingerprint,
        server_time,
    )
    .map(Some)
}

fn is_transient_status(status: StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

fn parse_api_error(status: StatusCode, body: &[u8]) -> KeygenClientError {
    let code = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/errors/0/code")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    KeygenClientError::Api {
        status: status.as_u16(),
        code,
    }
}

fn with_license_auth(
    request: RequestBuilder,
    license_key: &str,
) -> Result<RequestBuilder, KeygenClientError> {
    validate_license_key(license_key)?;
    let mut value = HeaderValue::from_str(&format!("License {license_key}"))
        .map_err(|_| KeygenClientError::InvalidLicenseKey)?;
    value.set_sensitive(true);
    Ok(request.header(AUTHORIZATION, value))
}

fn header(response: &reqwest::blocking::Response, name: &str) -> Result<String, KeygenClientError> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .ok_or(KeygenClientError::InvalidResponse)
}

fn signed_host(url: &Url) -> Result<String, KeygenClientError> {
    let host = url.host_str().ok_or(KeygenClientError::InvalidResponse)?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

fn classify_transport(error: reqwest::Error) -> KeygenClientError {
    if error.is_timeout() {
        KeygenClientError::Timeout
    } else if error.is_connect() {
        KeygenClientError::Connection
    } else {
        KeygenClientError::Transport
    }
}

fn validate_license_key(key: &str) -> Result<(), KeygenClientError> {
    if key.trim() != key || key.is_empty() || key.len() > 7_000 || key.contains(['\r', '\n', '\0'])
    {
        return Err(KeygenClientError::InvalidLicenseKey);
    }
    Ok(())
}

fn validate_resource_id(id: &str) -> Result<(), KeygenClientError> {
    if valid_identifier(id) {
        Ok(())
    } else {
        Err(KeygenClientError::InvalidResourceId)
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn safe_machine_name(value: &str) -> String {
    let value = value
        .chars()
        .filter(|character| !character.is_control())
        .take(100)
        .collect::<String>();
    if value.trim().is_empty() {
        "Windows PC".to_owned()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::licensing::{
        HardwareComponent, HardwareComponentKind, HardwareIdentityStrength, HARDWARE_SCHEMA_VERSION,
    };
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use ed25519_dalek::SigningKey;

    fn public_key() -> String {
        STANDARD.encode(
            SigningKey::from_bytes(&[4u8; 32])
                .verifying_key()
                .as_bytes(),
        )
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_compile_time_configuration_is_valid() {
        assert!(KeygenConfig::from_compile_time().is_ok());
    }

    fn config() -> KeygenConfig {
        KeygenConfig::new(
            "https://licensing.example.test",
            "account-1",
            "product-1",
            "policy-1",
            public_key(),
            DEFAULT_OFFLINE_TTL,
        )
        .unwrap()
    }

    fn hardware() -> HardwareIdentity {
        HardwareIdentity {
            schema: HARDWARE_SCHEMA_VERSION.to_owned(),
            fingerprint: "machine-fingerprint".to_owned(),
            components: vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            strength: HardwareIdentityStrength::Weak,
        }
    }

    #[test]
    fn configuration_rejects_non_https_and_path_injection() {
        assert!(matches!(
            KeygenConfig::new(
                "http://licensing.example.test",
                "account",
                "product",
                "policy",
                public_key(),
                DEFAULT_OFFLINE_TTL
            ),
            Err(KeygenClientError::InvalidConfiguration)
        ));
        assert!(matches!(
            KeygenConfig::new(
                "https://licensing.example.test",
                "../account",
                "product",
                "policy",
                public_key(),
                DEFAULT_OFFLINE_TTL
            ),
            Err(KeygenClientError::InvalidConfiguration)
        ));
    }

    #[test]
    fn validation_body_contains_all_required_scopes() {
        let value = validation_body(&config(), "LICENSE-KEY", &hardware());
        assert_eq!(
            value.pointer("/meta/scope/product").and_then(Value::as_str),
            Some("product-1")
        );
        assert_eq!(
            value.pointer("/meta/scope/policy").and_then(Value::as_str),
            Some("policy-1")
        );
        assert_eq!(
            value
                .pointer("/meta/scope/fingerprint")
                .and_then(Value::as_str),
            Some("machine-fingerprint")
        );
        assert_eq!(
            value
                .pointer("/meta/scope/components/0")
                .and_then(Value::as_str),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn unknown_validation_code_never_grants_access() {
        let body = br#"{
            "data": {"type": "licenses", "id": "license-1"},
            "meta": {"valid": true, "code": "FUTURE_CODE"}
        }"#;
        let result = parse_validation(body, 123).unwrap();
        assert!(!result.valid);
        assert_eq!(result.code, ValidationCode::Other("FUTURE_CODE".to_owned()));
    }

    #[test]
    fn machine_response_is_bound_to_license_and_fingerprint() {
        let body = br#"{
          "data": {
            "type": "machines",
            "id": "machine-1",
            "attributes": {"fingerprint": "fingerprint-1"},
            "relationships": {"license": {"data": {"id": "license-1"}}}
          }
        }"#;
        assert!(parse_machine(body, "license-1", "fingerprint-1", 10).is_ok());
        assert_eq!(
            parse_machine(body, "license-2", "fingerprint-1", 10),
            Err(KeygenClientError::ContextMismatch)
        );
    }

    #[test]
    fn license_key_is_rejected_before_header_construction() {
        for key in ["", " KEY", "KEY\nINJECT", "KEY\0VALUE"] {
            assert_eq!(
                validate_license_key(key),
                Err(KeygenClientError::InvalidLicenseKey)
            );
        }
    }
}
