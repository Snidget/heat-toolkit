use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime};

const EXPECTED_ALGORITHM: &str = "ed25519";
const EXPECTED_HEADERS: &str = "(request-target) host date digest";
const MAX_RESPONSE_AGE: Duration = Duration::from_secs(5 * 60);

pub struct SignedResponse<'a> {
    pub method: &'a str,
    pub path_and_query: &'a str,
    pub host: &'a str,
    pub date: &'a str,
    pub digest: &'a str,
    pub signature: &'a str,
    pub body: &'a [u8],
}

#[derive(Clone)]
pub struct ResponseSignatureVerifier {
    account_id: String,
    public_key: VerifyingKey,
}

impl ResponseSignatureVerifier {
    pub fn from_encoded_public_key(
        account_id: impl Into<String>,
        public_key: &str,
    ) -> Result<Self, ResponseSignatureError> {
        let account_id = account_id.into();
        if account_id.trim().is_empty() {
            return Err(ResponseSignatureError::InvalidConfiguration);
        }
        let encoded = public_key.trim();
        let decoded = if encoded.len() == 64 && encoded.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            (0..32)
                .map(|index| u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ResponseSignatureError::InvalidPublicKey)?
        } else {
            STANDARD
                .decode(encoded)
                .map_err(|_| ResponseSignatureError::InvalidPublicKey)?
        };
        Self::from_decoded(account_id, decoded)
    }

    pub fn from_base64(
        account_id: impl Into<String>,
        public_key: &str,
    ) -> Result<Self, ResponseSignatureError> {
        let account_id = account_id.into();
        if account_id.trim().is_empty() {
            return Err(ResponseSignatureError::InvalidConfiguration);
        }
        let decoded = STANDARD
            .decode(public_key.trim())
            .map_err(|_| ResponseSignatureError::InvalidPublicKey)?;
        Self::from_decoded(account_id, decoded)
    }

    fn from_decoded(account_id: String, decoded: Vec<u8>) -> Result<Self, ResponseSignatureError> {
        let bytes: [u8; 32] = decoded
            .try_into()
            .map_err(|_| ResponseSignatureError::InvalidPublicKey)?;
        let public_key = VerifyingKey::from_bytes(&bytes)
            .map_err(|_| ResponseSignatureError::InvalidPublicKey)?;
        Ok(Self {
            account_id,
            public_key,
        })
    }

    pub fn verify(
        &self,
        response: &SignedResponse<'_>,
        now: SystemTime,
    ) -> Result<SystemTime, ResponseSignatureError> {
        self.verify_inner(response, now, true)
    }

    /// Verifies authenticity/integrity exactly as [`Self::verify`] but does not
    /// reject the response for disagreeing with the local wall clock.
    ///
    /// This is only safe while recovering from a known clock rollback: the
    /// authenticated server `Date` is used to repair the trusted-time floor, so
    /// the rolled-back local clock must not be the sole freshness oracle.
    /// Signature, key id, request target, host and digest are still enforced.
    pub fn verify_clock_recovery(
        &self,
        response: &SignedResponse<'_>,
        now: SystemTime,
    ) -> Result<SystemTime, ResponseSignatureError> {
        self.verify_inner(response, now, false)
    }

    fn verify_inner(
        &self,
        response: &SignedResponse<'_>,
        now: SystemTime,
        enforce_freshness: bool,
    ) -> Result<SystemTime, ResponseSignatureError> {
        validate_component(response.method)?;
        validate_component(response.path_and_query)?;
        validate_component(response.host)?;
        validate_component(response.date)?;
        validate_component(response.digest)?;
        if !response.path_and_query.starts_with('/') {
            return Err(ResponseSignatureError::InvalidRequestTarget);
        }

        let server_time = httpdate::parse_http_date(response.date)
            .map_err(|_| ResponseSignatureError::InvalidDate)?;
        if enforce_freshness && absolute_difference(now, server_time) > MAX_RESPONSE_AGE {
            return Err(ResponseSignatureError::StaleResponse);
        }

        let calculated_digest =
            format!("sha-256={}", STANDARD.encode(Sha256::digest(response.body)));
        if response.digest != calculated_digest {
            return Err(ResponseSignatureError::DigestMismatch);
        }

        let parameters = parse_signature_header(response.signature)?;
        if parameters.get("keyid").map(String::as_str) != Some(self.account_id.as_str()) {
            return Err(ResponseSignatureError::WrongKeyId);
        }
        if parameters.get("algorithm").map(String::as_str) != Some(EXPECTED_ALGORITHM) {
            return Err(ResponseSignatureError::WrongAlgorithm);
        }
        if parameters.get("headers").map(String::as_str) != Some(EXPECTED_HEADERS) {
            return Err(ResponseSignatureError::WrongHeaders);
        }
        let encoded_signature = parameters
            .get("signature")
            .ok_or(ResponseSignatureError::MissingParameter)?;
        let signature = STANDARD
            .decode(encoded_signature)
            .map_err(|_| ResponseSignatureError::InvalidSignature)?;
        let signature: [u8; 64] = signature
            .try_into()
            .map_err(|_| ResponseSignatureError::InvalidSignature)?;
        let signature = Signature::from_bytes(&signature);

        let signing_data = signing_data(response, &calculated_digest);
        self.public_key
            .verify(signing_data.as_bytes(), &signature)
            .map_err(|_| ResponseSignatureError::InvalidSignature)?;
        Ok(server_time)
    }
}

fn signing_data(response: &SignedResponse<'_>, calculated_digest: &str) -> String {
    format!(
        "(request-target): {} {}\nhost: {}\ndate: {}\ndigest: {}",
        response.method.to_ascii_lowercase(),
        response.path_and_query,
        response.host,
        response.date,
        calculated_digest
    )
}

fn absolute_difference(left: SystemTime, right: SystemTime) -> Duration {
    left.duration_since(right)
        .or_else(|_| right.duration_since(left))
        .unwrap_or(Duration::MAX)
}

fn validate_component(value: &str) -> Result<(), ResponseSignatureError> {
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(ResponseSignatureError::InvalidHeader);
    }
    Ok(())
}

fn parse_signature_header(header: &str) -> Result<HashMap<String, String>, ResponseSignatureError> {
    validate_component(header)?;
    let mut parameters = HashMap::new();
    for part in header.split(',') {
        let (name, value) = part
            .trim()
            .split_once('=')
            .ok_or(ResponseSignatureError::InvalidHeader)?;
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or(ResponseSignatureError::InvalidHeader)?;
        if name.is_empty()
            || value.is_empty()
            || !name.chars().all(|character| character.is_ascii_lowercase())
            || parameters
                .insert(name.to_owned(), value.to_owned())
                .is_some()
        {
            return Err(ResponseSignatureError::InvalidHeader);
        }
    }
    if parameters.len() != 4 {
        return Err(ResponseSignatureError::MissingParameter);
    }
    Ok(parameters)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseSignatureError {
    InvalidConfiguration,
    InvalidPublicKey,
    InvalidHeader,
    InvalidRequestTarget,
    InvalidDate,
    StaleResponse,
    DigestMismatch,
    MissingParameter,
    WrongKeyId,
    WrongAlgorithm,
    WrongHeaders,
    InvalidSignature,
}

impl fmt::Display for ResponseSignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidConfiguration => "response verifier configuration is invalid",
            Self::InvalidPublicKey => "response verifier public key is invalid",
            Self::InvalidHeader => "response signature header is invalid",
            Self::InvalidRequestTarget => "response request target is invalid",
            Self::InvalidDate => "response date is invalid",
            Self::StaleResponse => "response date is outside the accepted window",
            Self::DigestMismatch => "response digest does not match the raw body",
            Self::MissingParameter => "response signature parameter is missing",
            Self::WrongKeyId => "response signature key id is unexpected",
            Self::WrongAlgorithm => "response signature algorithm is unexpected",
            Self::WrongHeaders => "response signature header list is unexpected",
            Self::InvalidSignature => "response signature is invalid",
        })
    }
}

impl std::error::Error for ResponseSignatureError {}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_fixture(
        body: &[u8],
        date: &'static str,
    ) -> (ResponseSignatureVerifier, String, String) {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let verifier = ResponseSignatureVerifier::from_base64(
            "account-id",
            &STANDARD.encode(verifying_key.as_bytes()),
        )
        .unwrap();
        let digest = format!("sha-256={}", STANDARD.encode(Sha256::digest(body)));
        let unsigned = SignedResponse {
            method: "POST",
            path_and_query: "/v1/accounts/account-id/licenses/actions/validate-key",
            host: "licensing.example.test",
            date,
            digest: &digest,
            signature: "unused",
            body,
        };
        let signature = signing_key.sign(signing_data(&unsigned, &digest).as_bytes());
        let header = format!(
            "keyid=\"account-id\", algorithm=\"ed25519\", signature=\"{}\", headers=\"(request-target) host date digest\"",
            STANDARD.encode(signature.to_bytes())
        );
        (verifier, digest, header)
    }

    #[test]
    fn verifies_raw_body_digest_date_context_and_signature() {
        let body = br#"{"meta":{"valid":true}}"#;
        let date = "Wed, 09 Jun 2021 16:08:15 GMT";
        let now = httpdate::parse_http_date(date).unwrap();
        let (verifier, digest, signature) = signed_fixture(body, date);
        let response = SignedResponse {
            method: "POST",
            path_and_query: "/v1/accounts/account-id/licenses/actions/validate-key",
            host: "licensing.example.test",
            date,
            digest: &digest,
            signature: &signature,
            body,
        };
        assert_eq!(verifier.verify(&response, now), Ok(now));
    }

    #[test]
    fn rejects_modified_body_and_stale_response() {
        let body = br#"{"meta":{"valid":true}}"#;
        let date = "Wed, 09 Jun 2021 16:08:15 GMT";
        let server_time = httpdate::parse_http_date(date).unwrap();
        let (verifier, digest, signature) = signed_fixture(body, date);
        let modified = SignedResponse {
            method: "POST",
            path_and_query: "/v1/accounts/account-id/licenses/actions/validate-key",
            host: "licensing.example.test",
            date,
            digest: &digest,
            signature: &signature,
            body: br#"{"meta":{"valid":false}}"#,
        };
        assert_eq!(
            verifier.verify(&modified, server_time),
            Err(ResponseSignatureError::DigestMismatch)
        );

        let valid = SignedResponse { body, ..modified };
        assert_eq!(
            verifier.verify(&valid, server_time + Duration::from_secs(301)),
            Err(ResponseSignatureError::StaleResponse)
        );
    }

    #[test]
    fn clock_recovery_accepts_authenticated_time_without_local_freshness() {
        let body = br#"{"meta":{"valid":true}}"#;
        let date = "Wed, 09 Jun 2021 16:08:15 GMT";
        let server_time = httpdate::parse_http_date(date).unwrap();
        let (verifier, digest, signature) = signed_fixture(body, date);
        let response = SignedResponse {
            method: "POST",
            path_and_query: "/v1/accounts/account-id/licenses/actions/validate-key",
            host: "licensing.example.test",
            date,
            digest: &digest,
            signature: &signature,
            body,
        };
        // Local clock is an hour behind; strict verification rejects it.
        let rolled_back_now = server_time - Duration::from_secs(3600);
        assert_eq!(
            verifier.verify(&response, rolled_back_now),
            Err(ResponseSignatureError::StaleResponse)
        );
        // Recovery still requires a valid signature and returns server time.
        assert_eq!(
            verifier.verify_clock_recovery(&response, rolled_back_now),
            Ok(server_time)
        );
        // A tampered body is still rejected during recovery.
        let tampered = SignedResponse {
            body: br#"{"meta":{"valid":false}}"#,
            ..response
        };
        assert_eq!(
            verifier.verify_clock_recovery(&tampered, rolled_back_now),
            Err(ResponseSignatureError::DigestMismatch)
        );
    }

    #[test]
    fn rejects_algorithm_or_signed_header_downgrade() {
        let body = b"{}";
        let date = "Wed, 09 Jun 2021 16:08:15 GMT";
        let now = httpdate::parse_http_date(date).unwrap();
        let (verifier, digest, signature) = signed_fixture(body, date);
        let downgraded = signature.replace("algorithm=\"ed25519\"", "algorithm=\"rsa-sha256\"");
        let response = SignedResponse {
            method: "GET",
            path_and_query: "/v1/test",
            host: "licensing.example.test",
            date,
            digest: &digest,
            signature: &downgraded,
            body,
        };
        assert_eq!(
            verifier.verify(&response, now),
            Err(ResponseSignatureError::WrongAlgorithm)
        );
    }
}
